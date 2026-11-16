//! Finding the AI command-line tools on this computer. A CLI installed after OnSet started is
//! not on its PATH yet, so each tool's usual install folders are searched too.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use super::Provider;

/// How to install a CLI, shown when it is missing. OnSet never installs anything itself.
#[derive(Debug, Clone, Serialize)]
pub struct InstallHelp {
    pub command: &'static str,
    pub url: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub provider: Provider,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub install: Option<InstallHelp>,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// The executable's file names and the folders its installers use, besides PATH.
fn candidates(provider: Provider) -> (&'static str, &'static [&'static str], Vec<PathBuf>) {
    let home = env_dir("USERPROFILE");
    let local = env_dir("LOCALAPPDATA");
    let roaming = env_dir("APPDATA");
    let join = |base: &Option<PathBuf>, rel: &str| base.as_ref().map(|b| b.join(rel));
    match provider {
        Provider::Claude => (
            "ONSET_CLAUDE",
            &["claude.exe", "claude.cmd", "claude"],
            [join(&home, ".local/bin"), join(&roaming, "npm")]
                .into_iter()
                .flatten()
                .collect(),
        ),
        Provider::Codex => (
            "ONSET_CODEX",
            &["codex.exe", "codex.cmd", "codex"],
            [
                join(&local, "Programs/OpenAI/Codex/bin"),
                join(&roaming, "npm"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        ),
        Provider::Gemini => (
            "ONSET_AGY",
            &["agy.exe", "agy"],
            [join(&local, "agy/bin")].into_iter().flatten().collect(),
        ),
        Provider::ChatGpt => ("", &[], Vec::new()),
    }
}

/// The first matching executable: an `ONSET_<TOOL>` override, then PATH, then install folders.
pub fn locate(provider: Provider) -> Option<PathBuf> {
    let (var, names, extra) = candidates(provider);
    if !var.is_empty()
        && let Some(p) = env_dir(var).filter(|p| p.is_file())
    {
        return Some(p);
    }
    let path_dirs = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .unwrap_or_default();
    path_dirs
        .iter()
        .chain(extra.iter())
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .find(|p| p.is_file())
}

/// `--version`'s first line, or `None` when the tool does not answer within ten seconds.
fn version(exe: &Path) -> Option<String> {
    let mut cmd = Command::new(exe);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    super::run::no_window(&mut cmd);
    let out = super::run::with_timeout(cmd, None, Duration::from_secs(10)).ok()?;
    out.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .map(str::to_owned)
}

pub fn install_help(provider: Provider) -> Option<InstallHelp> {
    match provider {
        Provider::Claude => Some(InstallHelp {
            command: "npm install -g @anthropic-ai/claude-code",
            url: "https://docs.anthropic.com/en/docs/claude-code/setup",
        }),
        Provider::Codex => Some(InstallHelp {
            command: "npm install -g @openai/codex",
            url: "https://github.com/openai/codex",
        }),
        Provider::Gemini => Some(InstallHelp {
            command: "Install Google Antigravity, which provides the agy command",
            url: "https://antigravity.google",
        }),
        Provider::ChatGpt => None,
    }
}

/// Which CLIs are installed, with their versions.
pub fn find(provider: Provider) -> Found {
    let path = locate(provider);
    let version = path.as_deref().and_then(version);
    Found {
        provider,
        install: path.is_none().then(|| install_help(provider)).flatten(),
        path,
        version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cli_has_an_override_and_install_help() {
        for p in Provider::CLIS {
            assert!(candidates(p).0.starts_with("ONSET_"));
            assert!(install_help(p).is_some());
        }
        assert!(install_help(Provider::ChatGpt).is_none());
    }
}
