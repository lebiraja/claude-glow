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
    born: Instant,
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
        let born = self.0.get(session).map_or(now, |e| e.born);
        self.0.insert(session.to_owned(), Entry { state, since: now, born });
    }

    /// One state per live session, in the order sessions started, at most `max` entries.
    /// Sessions past the limit are folded into the last slot at their highest priority.
    pub fn slots(&self, now: Instant, max: usize) -> Vec<State> {
        let mut live: Vec<_> = self.0.iter().filter(|(_, e)| now.duration_since(e.since) < SESSION_TTL).collect();
        live.sort_by_key(|(id, e)| (e.born, id.as_str()));
        let mut slots: Vec<State> = live.iter().map(|(_, e)| e.effective(now)).collect();
        if slots.len() > max {
            let folded = slots.split_off(max - 1).into_iter().max().expect("overflow is non-empty");
            slots.push(folded);
        }
        slots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_sessions_gives_no_slots() {
        assert!(Sessions::default().slots(Instant::now(), 6).is_empty());
    }

    #[test]
    fn each_session_gets_a_slot_in_start_order() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("b", Event::Working, t0);
        s.apply("a", Event::Ask, t0 + Duration::from_secs(1));

        assert_eq!(s.slots(t0 + Duration::from_secs(2), 6), vec![State::Working, State::Ask]);
    }

    #[test]
    fn slot_order_is_stable_when_a_session_changes_state() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Start, t0);
        s.apply("b", Event::Start, t0 + Duration::from_secs(1));

        s.apply("a", Event::Done, t0 + Duration::from_secs(2));

        assert_eq!(s.slots(t0 + Duration::from_secs(3), 6), vec![State::Done, State::Idle]);
    }

    #[test]
    fn overflow_sessions_fold_into_last_slot_by_priority() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        for (i, (id, ev)) in [("a", Event::Start), ("b", Event::Start), ("c", Event::Working), ("d", Event::Ask)].into_iter().enumerate() {
            s.apply(id, ev, t0 + Duration::from_secs(i as u64));
        }

        assert_eq!(s.slots(t0 + Duration::from_secs(9), 3), vec![State::Idle, State::Idle, State::Ask]);
    }

    #[test]
    fn done_holds_for_8s_then_falls_back_to_idle() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Done, t0);

        assert_eq!(s.slots(t0 + Duration::from_secs(7), 6), vec![State::Done]);
        assert_eq!(s.slots(t0 + Duration::from_secs(8), 6), vec![State::Idle]);
    }

    #[test]
    fn ending_last_session_clears_slots() {
        let now = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Working, now);

        s.apply("a", Event::End, now);

        assert!(s.slots(now, 6).is_empty());
    }

    #[test]
    fn stale_sessions_expire() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply("a", Event::Working, t0);

        assert!(s.slots(t0 + SESSION_TTL, 6).is_empty());
    }
}
