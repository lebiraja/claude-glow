use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use clap::ValueEnum;

use crate::state::Event;

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    dir.join("claude-glow.sock")
}

pub fn send(event: Event, session: &str) -> std::io::Result<()> {
    let name = event.to_possible_value().expect("no skipped variants").get_name().to_owned();
    let mut stream = UnixStream::connect(socket_path())?;
    writeln!(stream, "{name} {session}")
}

pub fn parse_line(line: &str) -> Option<(Event, String)> {
    let (event, session) = line.trim().split_once(' ')?;
    Some((Event::from_str(event, true).ok()?, session.to_owned()))
}

pub fn read_lines(stream: UnixStream) -> impl Iterator<Item = String> {
    BufReader::new(stream).lines().map_while(Result::ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_event_and_session() {
        assert_eq!(parse_line("working abc-1\n"), Some((Event::Working, "abc-1".to_owned())));
    }

    #[test]
    fn rejects_unknown_event_or_missing_session() {
        assert_eq!(parse_line("dance abc"), None);
        assert_eq!(parse_line("done"), None);
    }
}
