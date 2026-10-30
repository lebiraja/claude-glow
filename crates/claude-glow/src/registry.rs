use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// A running Claude Code session as Claude Code itself records it in `~/.claude/sessions/<pid>.json`.
/// The file is internal to Claude Code, so every field is optional and unreadable files are skipped.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Live {
    pub pid: u32,
    pub session_id: String,
    /// `busy` while a turn runs, `idle` at the prompt.
    #[serde(default)]
    pub status: Option<String>,
}

impl Live {
    pub fn idle(&self) -> bool {
        self.status.as_deref() == Some("idle")
    }

    pub fn busy(&self) -> bool {
        self.status.as_deref() == Some("busy")
    }
}

pub fn dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/sessions"))
}

pub fn read(dir: &Path) -> Vec<Live> {
    let Ok(entries) = fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| serde_json::from_str(&fs::read_to_string(p).ok()?).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_session_files_and_skips_junk() {
        let d = std::env::temp_dir().join(format!("claude-glow-registry-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("1.json"), r#"{"pid":1,"sessionId":"a","status":"idle","cwd":"/x"}"#).unwrap();
        fs::write(d.join("2.json"), "not json").unwrap();
        fs::write(d.join("3.key"), "secret").unwrap();

        let live = read(&d);

        assert_eq!(live, vec![Live { pid: 1, session_id: "a".into(), status: Some("idle".into()) }]);
        assert!(live[0].idle() && !live[0].busy());
    }

    #[test]
    fn missing_dir_is_empty() {
        assert!(read(Path::new("/nonexistent/claude-glow")).is_empty());
    }
}
