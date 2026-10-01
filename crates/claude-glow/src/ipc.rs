use std::io::Write;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::state::Event;

const WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// One event from a hook, sent to the daemon as a single JSON line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub event: Event,
    pub session: String,
    /// The `claude` process that owns the session, used to drop sessions that die without a `SessionEnd`.
    pub pid: Option<u32>,
    /// The session transcript, used to notice interrupts (which fire no hook).
    pub transcript: Option<PathBuf>,
}

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.join("claude-glow.sock")
}

pub fn send(msg: &Message) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(socket_path())?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let mut line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())
}

pub fn parse_line(line: &str) -> Option<Message> {
    serde_json::from_str(line.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_message() {
        let msg = Message { event: Event::Working, session: "abc-1".into(), pid: Some(42), transcript: Some("/t.jsonl".into()) };

        let line = serde_json::to_string(&msg).unwrap();

        assert_eq!(parse_line(&line), Some(msg));
    }

    #[test]
    fn optional_fields_may_be_absent() {
        let parsed = parse_line("{\"event\":\"done\",\"session\":\"s\"}");

        assert_eq!(parsed, Some(Message { event: Event::Done, session: "s".into(), pid: None, transcript: None }));
    }

    #[test]
    fn rejects_garbage_and_unknown_events() {
        assert_eq!(parse_line("not json"), None);
        assert_eq!(parse_line("{\"event\":\"dance\",\"session\":\"s\"}"), None);
        assert_eq!(parse_line(""), None);
    }
}
