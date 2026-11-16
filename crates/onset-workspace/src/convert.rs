//! Turning the prepared sections and approved cues into the phrase map and cue list the show
//! director already reads from rekordbox, placed on rekordbox's own beat grid.
use std::collections::HashMap;

use onset_core::grid::BeatGrid;
use onset_core::phrase::{Mood, Phrase, PhraseKind, PhraseMap};
use onset_core::track::HotCue;

use crate::{ApprovedCue, Prepared, SectionKind};

/// The prepared structure as a phrase map on `grid`. Sections map onto rekordbox's High-mood
/// vocabulary so the director's drop countdown and build-ups follow them: a drop is a
/// Chorus, a build an Up, a breakdown a Down. Labels keep the service's words ("Drop 2").
/// `None` when the grid is empty or no section spans a beat.
pub fn phrase_map(prepared: &Prepared, grid: &BeatGrid) -> Option<PhraseMap> {
    if grid.is_empty() {
        return None;
    }
    let last = u32::try_from(grid.len())
        .unwrap_or(u32::MAX)
        .saturating_add(1);
    let mut counts: HashMap<SectionKind, u32> = HashMap::new();
    let mut phrases: Vec<Phrase> = Vec::new();
    for (i, s) in prepared.sections.iter().enumerate() {
        let start_beat = beat_number(grid, s.start_s);
        let end_beat = if i + 1 == prepared.sections.len() {
            last
        } else {
            beat_number(grid, s.end_s)
        };
        // Start where the previous phrase ended, so rounding never overlaps two phrases.
        let start_beat = phrases
            .last()
            .map_or(start_beat, |p| start_beat.max(p.end_beat));
        if end_beat <= start_beat {
            continue;
        }
        let n = counts.entry(s.kind).or_insert(0);
        *n += 1;
        phrases.push(Phrase {
            kind: phrase_kind(s.kind),
            label: format!("{} {n}", s.kind.name()),
            start_beat,
            end_beat,
            fill_from_beat: None,
        });
    }
    let end_beat = phrases.last()?.end_beat;
    Some(PhraseMap {
        mood: Mood::High,
        phrases,
        end_beat,
    })
}

fn phrase_kind(kind: SectionKind) -> PhraseKind {
    match kind {
        SectionKind::Intro => PhraseKind::Intro,
        SectionKind::Groove | SectionKind::Vocal => PhraseKind::Verse,
        SectionKind::Stripped => PhraseKind::Bridge,
        SectionKind::Breakdown => PhraseKind::Down,
        SectionKind::Build => PhraseKind::Up,
        SectionKind::Drop => PhraseKind::Chorus,
        SectionKind::Outro => PhraseKind::Outro,
    }
}

/// The 1-based number of the grid beat nearest `t_s`, as rekordbox numbers phrase beats.
fn beat_number(grid: &BeatGrid, seconds: f64) -> u32 {
    let ms = seconds * 1000.0;
    let i = grid.beat_at(ms).unwrap_or(0);
    let nearest = match (grid.time_of(i), grid.time_of(i + 1)) {
        (Some(a), Some(b)) if (b - ms).abs() < (ms - a).abs() => i + 1,
        _ => i,
    };
    u32::try_from(nearest + 1).unwrap_or(u32::MAX)
}

/// rekordbox's cues plus the cues approved in preparation. An approved cue already in rekordbox
/// (within 50 ms) is not repeated; the rest become memory cues named as in preparation.
pub fn merged_cues(rekordbox: &[HotCue], approved: &[ApprovedCue]) -> Vec<HotCue> {
    let mut out = rekordbox.to_vec();
    for c in approved {
        let time_ms = (c.time_s * 1000.0).round().max(0.0) as u32;
        if rekordbox.iter().any(|r| r.time_ms.abs_diff(time_ms) <= 50) {
            continue;
        }
        out.push(HotCue {
            slot: 0,
            time_ms,
            name: if c.name.is_empty() {
                c.role.clone()
            } else {
                c.name.clone()
            },
            color: None,
        });
    }
    out.sort_by_key(|c| c.time_ms);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Section, SectionSource};
    use onset_core::grid::Beat;
    use onset_core::structure::Structure;

    /// 120 BPM, 128 beats from t=0: beat i at i × 0.5 s.
    fn grid() -> BeatGrid {
        BeatGrid::new(
            (0..128u32)
                .map(|i| Beat {
                    time_ms: i * 500,
                    beat_in_bar: u8::try_from(i % 4 + 1).unwrap(),
                    bpm: 120.0,
                })
                .collect(),
        )
    }

    fn prepared(sections: &[(SectionKind, f64, f64)]) -> Prepared {
        Prepared {
            track_id: "t".into(),
            sections: sections
                .iter()
                .map(|&(kind, start_s, end_s)| Section {
                    kind,
                    start_s,
                    end_s,
                })
                .collect(),
            section_source: SectionSource::Resolved,
            approved_cues: Vec::new(),
            energy_per_bar: Vec::new(),
        }
    }

    #[test]
    fn sections_land_on_the_nearest_beats() {
        let p = prepared(&[
            (SectionKind::Intro, 0.0, 16.1),
            (SectionKind::Build, 16.1, 31.9),
            (SectionKind::Drop, 31.9, 50.0),
            (SectionKind::Drop, 50.0, 64.0),
        ]);
        let map = phrase_map(&p, &grid()).unwrap();
        let got: Vec<_> = map
            .phrases
            .iter()
            .map(|p| (p.kind, p.label.as_str(), p.start_beat, p.end_beat))
            .collect();
        assert_eq!(
            got,
            [
                (PhraseKind::Intro, "Intro 1", 1, 33),
                (PhraseKind::Up, "Build 1", 33, 65),
                (PhraseKind::Chorus, "Drop 1", 65, 101),
                (PhraseKind::Chorus, "Drop 2", 101, 129),
            ]
        );
        assert_eq!(map.mood, Mood::High);
        assert_eq!(map.end_beat, 129);
    }

    #[test]
    fn the_director_counts_down_to_the_prepared_drop() {
        let g = grid();
        let p = prepared(&[
            (SectionKind::Intro, 0.0, 16.0),
            (SectionKind::Drop, 16.0, 64.0),
        ]);
        let map = phrase_map(&p, &g).unwrap();
        // 10 s is grid beat 20 (rekordbox beat 21); the drop starts at beat 33.
        let st = Structure::new(&g, Some(&map), &[]).at(10_000.0);
        assert_eq!(st.drop_countdown_beats, Some(12));
        assert_eq!(st.phrase_label.as_deref(), Some("Intro 1"));
    }

    #[test]
    fn an_empty_grid_or_no_sections_give_none() {
        let p = prepared(&[(SectionKind::Drop, 0.0, 10.0)]);
        assert!(phrase_map(&p, &BeatGrid::default()).is_none());
        assert!(phrase_map(&prepared(&[]), &grid()).is_none());
    }

    #[test]
    fn approved_cues_join_rekordbox_cues_without_duplicates() {
        let rb = vec![HotCue {
            slot: 1,
            time_ms: 32_000,
            name: String::new(),
            color: Some([255, 0, 0]),
        }];
        let approved = vec![
            ApprovedCue {
                role: "D".into(),
                name: "D groove".into(),
                time_s: 32.03,
                end_s: None,
            },
            ApprovedCue {
                role: "E".into(),
                name: String::new(),
                time_s: 96.0,
                end_s: None,
            },
        ];
        let got = merged_cues(&rb, &approved);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].slot, 1);
        assert_eq!(
            (got[1].slot, got[1].time_ms, got[1].name.as_str()),
            (0, 96_000, "E")
        );
    }
}
