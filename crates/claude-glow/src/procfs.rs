use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

const CLAUDE_COMM: &str = "claude";
const INTERRUPT_MARKER: &str = "Request interrupted by user";
const TAIL_BYTES: u64 = 8192;

fn comm(pid: u32) -> Option<String> {
    fs::read_to_string(format!("/proc/{pid}/comm")).ok().map(|c| c.trim().to_owned())
}

fn parent_of(pid: u32) -> Option<u32> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat.rsplit_once(')')?.1.split_whitespace().nth(1)?.parse().ok()
}

/// The `claude` process that (transitively) spawned this hook, if any.
pub fn claude_ancestor() -> Option<u32> {
    let mut pid = std::os::unix::process::parent_id();
    for _ in 0..8 {
        if pid <= 1 {
            return None;
        }
        if comm(pid)? == CLAUDE_COMM {
            return Some(pid);
        }
        pid = parent_of(pid)?;
    }
    None
}

/// True while `pid` is still a running `claude` process (guards against pid reuse).
pub fn claude_alive(pid: u32) -> bool {
    comm(pid).is_some_and(|c| c == CLAUDE_COMM)
}

/// True when the transcript's last entry is Claude Code's "interrupted by user" marker and the file
/// was written after `since` (so a marker left over from an earlier turn doesn't count).
pub fn transcript_interrupted(path: &Path, since: SystemTime) -> bool {
    let Ok(mut file) = File::open(path) else { return false };
    let Ok(meta) = file.metadata() else { return false };
    if !meta.modified().is_ok_and(|m| m > since) {
        return false;
    }
    if file.seek(SeekFrom::Start(meta.len().saturating_sub(TAIL_BYTES))).is_err() {
        return false;
    }
    let mut tail = Vec::new();
    if file.read_to_end(&mut tail).is_err() {
        return false;
    }
    String::from_utf8_lossy(&tail)
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .is_some_and(|l| l.contains(INTERRUPT_MARKER))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn transcript(name: &str, body: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("claude-glow-test-{}-{name}", std::process::id()));
        fs::write(&p, body).expect("write test transcript");
        p
    }

    #[test]
    fn detects_interrupt_as_the_last_entry() {
        let p = transcript("last", "{\"a\":1}\n{\"text\":\"[Request interrupted by user for tool use]\"}\n");

        assert!(transcript_interrupted(&p, SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn ignores_interrupt_followed_by_a_newer_entry() {
        let p = transcript("stale", "{\"text\":\"[Request interrupted by user]\"}\n{\"text\":\"next prompt\"}\n");

        assert!(!transcript_interrupted(&p, SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn ignores_transcripts_not_written_since_the_event() {
        let p = transcript("old", "{\"text\":\"[Request interrupted by user]\"}\n");

        assert!(!transcript_interrupted(&p, SystemTime::now() + Duration::from_secs(3600)));
    }

    #[test]
    fn missing_transcript_is_not_an_interrupt() {
        assert!(!transcript_interrupted(Path::new("/nonexistent/claude-glow.jsonl"), SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn non_claude_pid_is_not_alive() {
        assert!(!claude_alive(u32::MAX));
        assert!(!claude_alive(std::process::id()));
    }
}
