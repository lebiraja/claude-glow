use std::collections::HashMap;
use std::time::{Duration, Instant};

use clap::ValueEnum;

pub const DONE_HOLD: Duration = Duration::from_secs(8);
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);

/// Declaration order is display priority, lowest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Idle,
    Done,
    Working,
    Ask,
}

impl State {
    pub fn scene(self) -> &'static str {
        match self {
            State::Idle => "idle",
            State::Done => "done",
            State::Working => "working",
            State::Ask => "ask",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Event {
    Start,
    Working,
    Ask,
    Done,
    End,
}

struct Entry {
    state: State,
    since: Instant,
}

impl Entry {
    fn effective(&self, now: Instant) -> State {
        if self.state == State::Done && now.duration_since(self.since) >= DONE_HOLD {
            State::Idle
        } else {
            self.state
        }
    }
}

#[derive(Default)]
pub struct Sessions(HashMap<String, Entry>);

impl Sessions {
    pub fn apply(&mut self, session: &str, event: Event, now: Instant) {
        let state = match event {
            Event::End => {
                self.0.remove(session);
                return;
            }
            Event::Start => State::Idle,
            Event::Working => State::Working,
            Event::Ask => State::Ask,
            Event::Done => State::Done,
        };
        self.0.insert(session.to_owned(), Entry { state, since: now });
    }

    /// Highest-priority state across live sessions, or None when no session is open.
    pub fn displayed(&self, now: Instant) -> Option<State> {
        self.0
            .values()
            .filter(|e| now.duration_since(e.since) < SESSION_TTL)
            .map(|e| e.effective(now))
            .max()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sessions_shows_nothing() {
        assert_eq!(Sessions::default().displayed(Instant::now()), None);
    }

    #[test]
    fn highest_priority_wins_across_sessions() {
        let now = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Start, now);
        s.apply("b", Event::Working, now);
        assert_eq!(s.displayed(now), Some(State::Working));

        s.apply("a", Event::Ask, now);

        assert_eq!(s.displayed(now), Some(State::Ask));
    }

    #[test]
    fn done_holds_for_8s_then_falls_back_to_idle() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Done, t0);

        assert_eq!(s.displayed(t0 + Duration::from_secs(7)), Some(State::Done));
        assert_eq!(s.displayed(t0 + Duration::from_secs(8)), Some(State::Idle));
    }

    #[test]
    fn ending_last_session_clears_display() {
        let now = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Working, now);

        s.apply("a", Event::End, now);

        assert_eq!(s.displayed(now), None);
    }

    #[test]
    fn stale_sessions_expire() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Working, t0);

        assert_eq!(s.displayed(t0 + SESSION_TTL), None);
    }
}
