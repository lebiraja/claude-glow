use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::ipc::Message;

pub const DONE_HOLD: Duration = Duration::from_secs(8);
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
/// Working/Ask with no event for this long means we missed the end of the turn (e.g. an interrupt).
const ACTIVE_STALE: Duration = Duration::from_secs(30 * 60);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
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
    /// Wall-clock twin of `since`, to compare against transcript modification times.
    at: SystemTime,
    /// Position in the bar, kept for the session's whole life so segments don't swap sides.
    slot: usize,
    pid: Option<u32>,
    transcript: Option<PathBuf>,
}

impl Entry {
    fn effective(&self, now: Instant) -> State {
        let age = now.duration_since(self.since);
        match self.state {
            State::Done if age >= DONE_HOLD => State::Idle,
            State::Working | State::Ask if age >= ACTIVE_STALE => State::Idle,
            state => state,
        }
    }
}

#[derive(Default)]
pub struct Sessions(HashMap<String, Entry>);

impl Sessions {
    pub fn apply(&mut self, msg: &Message, now: Instant, at: SystemTime) {
        let state = match msg.event {
            Event::End => {
                self.0.remove(&msg.session);
                return;
            }
            Event::Start => State::Idle,
            Event::Working => State::Working,
            Event::Ask => State::Ask,
            Event::Done => State::Done,
        };
        let previous = self.0.remove(&msg.session);
        let slot = previous.as_ref().map_or_else(|| self.free_slot(), |p| p.slot);
        // A repeated SessionStart (resume, compact) must not knock a busy session back to idle.
        let (state, since, at) = match &previous {
            Some(p) if msg.event == Event::Start => (p.state, p.since, p.at),
            _ => (state, now, at),
        };
        self.0.insert(
            msg.session.clone(),
            Entry {
                state,
                since,
                at,
                slot,
                pid: msg.pid.or(previous.as_ref().and_then(|p| p.pid)),
                transcript: msg.transcript.clone().or(previous.and_then(|p| p.transcript)),
            },
        );
    }

    fn free_slot(&self) -> usize {
        (0..).find(|slot| self.0.values().all(|e| e.slot != *slot)).expect("unbounded range")
    }

    /// Drop sessions that expired or whose process died, and turn interrupted sessions back to idle.
    pub fn sweep(
        &mut self,
        now: Instant,
        at: SystemTime,
        alive: impl Fn(u32) -> bool,
        interrupted: impl Fn(&Path, SystemTime) -> bool,
    ) {
        self.0.retain(|_, e| now.duration_since(e.since) < SESSION_TTL && e.pid.is_none_or(&alive));
        for e in self.0.values_mut() {
            let busy = matches!(e.effective(now), State::Working | State::Ask);
            if busy && e.transcript.as_deref().is_some_and(|t| interrupted(t, e.at)) {
                (e.state, e.since, e.at) = (State::Idle, now, at);
            }
        }
    }

    /// One state per live session in slot order (each session keeps its slot until it ends), at most `max` entries.
    /// Sessions past the limit are folded into the last slot at their highest priority.
    pub fn slots(&self, now: Instant, max: usize) -> Vec<State> {
        if max == 0 {
            return Vec::new();
        }
        let mut live: Vec<_> = self.0.iter().filter(|(_, e)| now.duration_since(e.since) < SESSION_TTL).collect();
        live.sort_by_key(|(id, e)| (e.slot, id.as_str()));
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

    fn msg(event: Event, session: &str) -> Message {
        Message { event, session: session.into(), pid: None, transcript: None }
    }

    fn apply(s: &mut Sessions, event: Event, session: &str, now: Instant) {
        s.apply(&msg(event, session), now, SystemTime::now());
    }

    fn sweep_with(s: &mut Sessions, now: Instant, alive: bool, interrupted: bool) {
        s.sweep(now, SystemTime::now(), |_| alive, |_, _| interrupted);
    }

    #[test]
    fn no_sessions_gives_no_slots() {
        assert!(Sessions::default().slots(Instant::now(), 6).is_empty());
    }

    #[test]
    fn each_session_gets_a_slot_in_start_order() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "b", t0);
        apply(&mut s, Event::Ask, "a", t0 + Duration::from_secs(1));

        assert_eq!(s.slots(t0 + Duration::from_secs(2), 6), vec![State::Working, State::Ask]);
    }

    #[test]
    fn slot_order_is_stable_when_a_session_changes_state() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Start, "a", t0);
        apply(&mut s, Event::Start, "b", t0 + Duration::from_secs(1));

        apply(&mut s, Event::Done, "a", t0 + Duration::from_secs(2));

        assert_eq!(s.slots(t0 + Duration::from_secs(3), 6), vec![State::Done, State::Idle]);
    }

    #[test]
    fn zero_leds_means_no_slots_instead_of_a_panic() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", now);

        assert!(s.slots(now, 0).is_empty());
    }

    #[test]
    fn more_sessions_than_leds_still_fold_into_the_available_slots() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Start, "a", now);
        apply(&mut s, Event::Ask, "b", now);

        assert_eq!(s.slots(now, 1), vec![State::Ask]);
    }

    #[test]
    fn a_surviving_session_keeps_its_side_when_a_new_one_joins() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Start, "a", t0);
        apply(&mut s, Event::Working, "b", t0);
        apply(&mut s, Event::End, "a", t0);

        apply(&mut s, Event::Done, "c", t0);

        assert_eq!(s.slots(t0, 6), vec![State::Done, State::Working]);
    }

    #[test]
    fn overflow_sessions_fold_into_last_slot_by_priority() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        for (i, (id, ev)) in [("a", Event::Start), ("b", Event::Start), ("c", Event::Working), ("d", Event::Ask)].into_iter().enumerate() {
            apply(&mut s, ev, id, t0 + Duration::from_secs(i as u64));
        }

        assert_eq!(s.slots(t0 + Duration::from_secs(9), 3), vec![State::Idle, State::Idle, State::Ask]);
    }

    #[test]
    fn done_holds_for_8s_then_falls_back_to_idle() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Done, "a", t0);

        assert_eq!(s.slots(t0 + Duration::from_secs(7), 6), vec![State::Done]);
        assert_eq!(s.slots(t0 + Duration::from_secs(8), 6), vec![State::Idle]);
    }

    #[test]
    fn ending_last_session_clears_slots() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", now);

        apply(&mut s, Event::End, "a", now);

        assert!(s.slots(now, 6).is_empty());
    }

    #[test]
    fn stale_sessions_expire() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", t0);

        assert!(s.slots(t0 + SESSION_TTL, 6).is_empty());
    }

    #[test]
    fn working_without_events_for_30_minutes_falls_back_to_idle() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", t0);

        assert_eq!(s.slots(t0 + ACTIVE_STALE - Duration::from_secs(1), 6), vec![State::Working]);
        assert_eq!(s.slots(t0 + ACTIVE_STALE, 6), vec![State::Idle]);
    }

    #[test]
    fn repeated_start_does_not_reset_a_busy_session() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", t0);

        apply(&mut s, Event::Start, "a", t0 + Duration::from_secs(1));

        assert_eq!(s.slots(t0 + Duration::from_secs(2), 6), vec![State::Working]);
    }

    #[test]
    fn sweep_drops_sessions_whose_process_died() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply(&Message { pid: Some(7), ..msg(Event::Working, "a") }, t0, SystemTime::now());

        sweep_with(&mut s, t0, false, false);

        assert!(s.slots(t0, 6).is_empty());
    }

    #[test]
    fn sweep_keeps_sessions_without_a_pid() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", t0);

        sweep_with(&mut s, t0, false, false);

        assert_eq!(s.slots(t0, 6), vec![State::Working]);
    }

    #[test]
    fn sweep_turns_an_interrupted_working_session_idle() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply(&Message { transcript: Some("/t.jsonl".into()), ..msg(Event::Working, "a") }, t0, SystemTime::now());

        sweep_with(&mut s, t0, true, true);

        assert_eq!(s.slots(t0, 6), vec![State::Idle]);
    }

    #[test]
    fn sweep_does_not_touch_done_or_idle_sessions_on_interrupt() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        s.apply(&Message { transcript: Some("/t.jsonl".into()), ..msg(Event::Done, "a") }, t0, SystemTime::now());

        sweep_with(&mut s, t0, true, true);

        assert_eq!(s.slots(t0, 6), vec![State::Done]);
    }
}
