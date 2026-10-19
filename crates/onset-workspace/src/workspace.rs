//! Opening the preparation workspace (SQLite) read-only and reading one track's profile from it.
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use crate::{ApprovedCue, Prepared, Section, SectionKind, SectionSource};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("workspace {0}: {1}")]
    Sqlite(PathBuf, rusqlite::Error),
    #[error("{0} is not an OnSet workspace (no tracks table)")]
    NotAWorkspace(PathBuf),
    #[error("profile for track {0} is not valid JSON: {1}")]
    Profile(String, serde_json::Error),
}

/// The workspace database's file name inside [`default_home`].
pub const DB_NAME: &str = "workspace.db";
const LEGACY_DB_NAME: &str = "autocue.db";

/// Where the service keeps its workspace: `%ONSET_WORKSPACE%`, else `workspace` in OnSet's
/// local data folder, as `service/onset_service/util.py` resolves it.
pub fn default_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("ONSET_WORKSPACE") {
        return Some(dir.into());
    }
    directories::ProjectDirs::from("", "Onset", "Onset")
        .map(|d| d.data_local_dir().join("workspace"))
}

/// The folder OnSet's predecessor, Autocue, kept its workspace in.
fn legacy_home() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join("Autocue"))
}

/// Moves a pre-OnSet workspace folder to `home` once, renaming its database to [`DB_NAME`].
/// Leaves everything in place when `home` already exists, there is nothing to move, or the
/// old files are in use; returns whether it moved. Only the desktop app calls this, before
/// it starts the service; everything else in this crate is read-only.
pub fn move_legacy_home(home: &Path) -> std::io::Result<bool> {
    let Some(legacy) = legacy_home() else {
        return Ok(false);
    };
    if home.exists() || !legacy.join(LEGACY_DB_NAME).exists() {
        return Ok(false);
    }
    if let Some(parent) = home.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(&legacy, home)?;
    for suffix in ["", "-wal", "-shm"] {
        let old = home.join(format!("{LEGACY_DB_NAME}{suffix}"));
        if old.exists() {
            std::fs::rename(old, home.join(format!("{DB_NAME}{suffix}")))?;
        }
    }
    Ok(true)
}

pub struct Workspace {
    conn: Connection,
    path: PathBuf,
}

impl Workspace {
    /// The workspace database: `workspace.db` in [`default_home`], or the database a
    /// pre-OnSet install (Autocue) left in [`legacy_home`] until the desktop app moves it.
    pub fn default_path() -> Option<PathBuf> {
        let path = default_home()?.join(DB_NAME);
        if path.exists() {
            return Some(path);
        }
        let legacy = legacy_home().map(|h| h.join(LEGACY_DB_NAME));
        Some(legacy.filter(|l| l.exists()).unwrap_or(path))
    }

    /// Opens the default workspace when one exists; logs and returns `None` otherwise.
    pub fn open_default() -> Option<Self> {
        let path = Self::default_path().filter(|p| p.exists())?;
        Self::open(&path).map_err(|e| tracing::warn!("{e}")).ok()
    }

    /// Opens a workspace read-only. The service keeps it in WAL mode, so reads here never
    /// block the service's writes and see each commit whole.
    pub fn open(path: &Path) -> Result<Self, Error> {
        let err = |e| Error::Sqlite(path.to_path_buf(), e);
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(err)?;
        let has_tracks: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tracks')",
                [],
                |r| r.get(0),
            )
            .map_err(err)?;
        if !has_tracks {
            return Err(Error::NotAWorkspace(path.to_path_buf()));
        }
        Ok(Self {
            conn,
            path: path.to_path_buf(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// What the workspace knows about a track, found by rekordbox content id first and then by
    /// file path. `None` when it has no structure for it.
    pub fn prepared(
        &self,
        rekordbox_id: Option<u64>,
        file: Option<&Path>,
    ) -> Result<Option<Prepared>, Error> {
        let err = |e| Error::Sqlite(self.path.clone(), e);
        let mut profile: Option<String> = None;
        if let Some(id) = rekordbox_id {
            // The service stores the id as text; accept a number too.
            profile = self
                .conn
                .query_row(
                    "SELECT profile FROM tracks
                     WHERE CAST(json_extract(profile, '$.rekordbox_id') AS TEXT) = ?1
                     LIMIT 1",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .optional()
                .map_err(err)?;
        }
        if profile.is_none()
            && let Some(file) = file
        {
            // The service records every location of a track's audio as an alias. Compare without
            // regard to case or slash direction, as Windows does.
            let wanted = file.to_string_lossy().replace('/', "\\");
            profile = self
                .conn
                .query_row(
                    "SELECT t.profile FROM aliases a JOIN tracks t ON t.id = a.track_id
                     WHERE replace(a.path, '/', '\\') = ?1 COLLATE NOCASE
                     LIMIT 1",
                    [wanted],
                    |r| r.get(0),
                )
                .optional()
                .map_err(err)?;
        }
        let Some(text) = profile else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(&text).map_err(|e| {
            Error::Profile(rekordbox_id.map_or_else(String::new, |i| i.to_string()), e)
        })?;
        Ok(prepared_from_profile(&value))
    }
}

/// Reads the parts of a track profile the show uses. Missing or malformed fields are
/// skipped rather than failing the whole track.
pub(crate) fn prepared_from_profile(p: &Value) -> Option<Prepared> {
    let analysis = p.get("analysis")?;
    let duration = number(p.get("duration"))
        .or_else(|| number(analysis.get("duration")))
        .unwrap_or(0.0);

    let resolved = resolved_sections(analysis);
    let (sections, section_source) = if resolved.is_empty() {
        (event_sections(analysis, duration), SectionSource::Events)
    } else {
        (resolved, SectionSource::Resolved)
    };
    if sections.is_empty() {
        return None;
    }

    let approved_cues = p
        .get("cues")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| c.get("review").and_then(Value::as_str) == Some("approved"))
        .filter_map(|c| {
            Some(ApprovedCue {
                role: text(c.get("role")),
                name: text(c.get("name")),
                time_s: number(c.get("time"))?,
                end_s: number(c.get("end")),
            })
        })
        .collect();

    let energy_per_bar = analysis
        .get("energy")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_f64)
        .map(|e| e as f32)
        .collect();

    Some(Prepared {
        track_id: text(p.get("id")),
        sections,
        section_source,
        approved_cues,
        energy_per_bar,
    })
}

/// `analysis.structure.sections`: the duration-aware sequence model's output.
fn resolved_sections(analysis: &Value) -> Vec<Section> {
    analysis
        .pointer("/structure/sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| {
            let kind = SectionKind::from_label(s.get("label")?.as_str()?)?;
            let start_s = number(s.get("start"))?;
            let end_s = number(s.get("end"))?;
            (end_s > start_s).then_some(Section {
                kind,
                start_s,
                end_s,
            })
        })
        .collect()
}

/// `analysis.sections`: bar indices of the key events (first full groove, first breakdown,
/// later drop, outro), turned into a run of sections over `analysis.downbeats`.
fn event_sections(analysis: &Value, duration: f64) -> Vec<Section> {
    let downbeats: Vec<f64> = analysis
        .get("downbeats")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_f64)
        .collect();
    let Some(events) = analysis.get("sections").and_then(Value::as_object) else {
        return Vec::new();
    };
    let at_bar = |key: &str| {
        let bar = usize::try_from(events.get(key)?.as_u64()?).ok()?;
        downbeats.get(bar).copied()
    };
    let mut starts: Vec<(f64, SectionKind)> = [
        ("drop", SectionKind::Drop),
        ("breakdown", SectionKind::Breakdown),
        ("second_drop", SectionKind::Drop),
        ("outro", SectionKind::Outro),
    ]
    .into_iter()
    .filter_map(|(key, kind)| Some((at_bar(key)?, kind)))
    .collect();
    if starts.is_empty() {
        return Vec::new();
    }
    starts.sort_by(|a, b| a.0.total_cmp(&b.0));
    starts.dedup_by(|b, a| (b.0 - a.0).abs() < 1e-3);
    starts.insert(0, (0.0, SectionKind::Intro));
    let end = duration.max(downbeats.last().copied().unwrap_or(0.0));

    starts
        .iter()
        .enumerate()
        .filter_map(|(i, &(start_s, kind))| {
            let end_s = starts.get(i + 1).map_or(end, |n| n.0);
            (end_s > start_s).then_some(Section {
                kind,
                start_s,
                end_s,
            })
        })
        .collect()
}

fn number(v: Option<&Value>) -> Option<f64> {
    v?.as_f64().filter(|x| x.is_finite())
}

fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolved_structure_wins_and_keeps_only_known_labels() {
        let p = json!({
            "id": "t1",
            "duration": 200.0,
            "analysis": {
                "structure": {"sections": [
                    {"start": 0.0, "end": 30.0, "label": "intro"},
                    {"start": 30.0, "end": 60.0, "label": "build"},
                    {"start": 60.0, "end": 120.0, "label": "drop"},
                    {"start": 120.0, "end": 130.0, "label": "silence"},
                    {"start": 130.0, "end": 200.0, "label": "outro"}
                ]},
                "sections": {"drop": 1},
                "downbeats": [0.0, 2.0]
            }
        });
        let got = prepared_from_profile(&p).unwrap();
        assert_eq!(got.section_source, SectionSource::Resolved);
        let kinds: Vec<_> = got.sections.iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds,
            [
                SectionKind::Intro,
                SectionKind::Build,
                SectionKind::Drop,
                SectionKind::Outro
            ]
        );
    }

    #[test]
    fn event_bars_become_a_run_of_sections() {
        let downbeats: Vec<f64> = (0..100).map(|b| f64::from(b) * 2.0).collect();
        let p = json!({
            "id": "t2",
            "duration": 200.0,
            "analysis": {
                "downbeats": downbeats,
                "sections": {"drop": 16, "breakdown": 48, "second_drop": 64, "outro": 88}
            }
        });
        let got = prepared_from_profile(&p).unwrap();
        assert_eq!(got.section_source, SectionSource::Events);
        let spans: Vec<_> = got
            .sections
            .iter()
            .map(|s| (s.kind, s.start_s, s.end_s))
            .collect();
        assert_eq!(
            spans,
            [
                (SectionKind::Intro, 0.0, 32.0),
                (SectionKind::Drop, 32.0, 96.0),
                (SectionKind::Breakdown, 96.0, 128.0),
                (SectionKind::Drop, 128.0, 176.0),
                (SectionKind::Outro, 176.0, 200.0),
            ]
        );
    }

    #[test]
    fn missing_events_and_a_drop_on_bar_zero_are_handled() {
        let p = json!({
            "duration": 100.0,
            "analysis": {"downbeats": [0.0, 2.0, 4.0], "sections": {"drop": 0, "breakdown": null}}
        });
        let got = prepared_from_profile(&p).unwrap();
        let kinds: Vec<_> = got.sections.iter().map(|s| s.kind).collect();
        assert_eq!(kinds, [SectionKind::Drop]);
    }

    #[test]
    fn only_approved_cues_are_taken() {
        let p = json!({
            "analysis": {"downbeats": [0.0, 2.0], "sections": {"drop": 1}},
            "cues": [
                {"role": "A", "name": "A intro", "time": 0.0, "end": null, "review": "proposed"},
                {"role": "D", "name": "D groove", "time": 2.0, "end": null, "review": "approved"},
                {"role": "E", "name": "E break", "time": 4.0, "end": 6.0, "review": "rejected"}
            ]
        });
        let got = prepared_from_profile(&p).unwrap();
        assert_eq!(
            got.approved_cues,
            [ApprovedCue {
                role: "D".into(),
                name: "D groove".into(),
                time_s: 2.0,
                end_s: None
            }]
        );
    }

    #[test]
    fn a_profile_without_structure_is_none() {
        assert!(prepared_from_profile(&json!({"analysis": {"downbeats": []}})).is_none());
        assert!(prepared_from_profile(&json!({"id": "x"})).is_none());
    }

    #[test]
    fn opens_read_only_and_finds_by_id_or_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(DB_NAME);
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch(
                "PRAGMA journal_mode=WAL;
                 CREATE TABLE tracks(id TEXT PRIMARY KEY, fingerprint TEXT UNIQUE, profile TEXT NOT NULL);
                 CREATE TABLE aliases(path TEXT PRIMARY KEY, track_id TEXT NOT NULL);",
            )
            .unwrap();
            let profile = json!({
                "id": "t1", "rekordbox_id": "126351799", "duration": 60.0,
                "analysis": {"downbeats": [0.0, 2.0, 4.0], "sections": {"drop": 2}}
            });
            c.execute(
                "INSERT INTO tracks VALUES ('t1', 'f1', ?1)",
                [profile.to_string()],
            )
            .unwrap();
            c.execute(
                "INSERT INTO aliases VALUES ('C:/Music/Stop It.flac', 't1')",
                [],
            )
            .unwrap();
        }
        let ws = Workspace::open(&path).unwrap();
        assert_eq!(
            ws.prepared(Some(126_351_799), None)
                .unwrap()
                .unwrap()
                .track_id,
            "t1"
        );
        let by_path = ws
            .prepared(Some(1), Some(Path::new(r"c:\music\STOP IT.flac")))
            .unwrap();
        assert_eq!(by_path.unwrap().track_id, "t1");
        assert!(ws.prepared(Some(2), None).unwrap().is_none());
        // Read-only: a write through this connection must fail.
        assert!(ws.conn.execute("DELETE FROM tracks", []).is_err());
    }

    #[test]
    fn a_database_without_tracks_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE x(a)")
            .unwrap();
        assert!(matches!(
            Workspace::open(&path),
            Err(Error::NotAWorkspace(_))
        ));
    }
}
