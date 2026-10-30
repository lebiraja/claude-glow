use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::ipc::Message;
use crate::registry::Live;

pub const DONE_HOLD: Duration = Duration::from_secs(8);
const SESSION_TTL: Duration = Duration::from_secs(12 * 3600);
/// Working/Ask with no event for this long means we missed the end of the turn (e.g. an interrupt).
const ACTIVE_STALE: Duration = Duration::from_secs(30 * 60);
/// How long Claude Code may still report a session as idle after its working hook, before we believe it.
const IDLE_GRACE: Duration = Duration::from_secs(5);
/// Consecutive reconciles a known session may be missing from Claude Code's session list before it is dropped
/// (more than one, so a file caught mid-write doesn't remove a live session).
const MISSING_LIMIT: u8 = 2;

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
    /// Seen in Claude Code's session list, so its disappearance from that list means the session closed.
    listed: bool,
    /// Consecutive reconciles in which a listed session was missing.
    misses: u8,
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
pub struct Sessions {
    entries: HashMap<String, Entry>,
    /// Sessions that sent `end`. Claude Code keeps listing a session (and its process lives) for a moment after
    /// `SessionEnd`, so these are not re-added from the list or by late hooks. A new `start` (resume) clears it.
    ended: HashSet<String>,
}

impl Sessions {
    pub fn apply(&mut self, msg: &Message, now: Instant, at: SystemTime) {
        let state = match msg.event {
            Event::End => {
                self.entries.remove(&msg.session);
                self.ended.insert(msg.session.clone());
                return;
            }
            Event::Start => State::Idle,
            Event::Working => State::Working,
            Event::Ask => State::Ask,
            Event::Done => State::Done,
        };
        if msg.event == Event::Start {
            self.ended.remove(&msg.session);
        } else if self.ended.contains(&msg.session) {
            return;
        }
        let previous = self.entries.remove(&msg.session);
        let slot = previous.as_ref().map_or_else(|| self.free_slot(), |p| p.slot);
        // A repeated SessionStart (resume, compact) must not knock a busy session back to idle.
        let (state, since, at) = match &previous {
            Some(p) if msg.event == Event::Start => (p.state, p.since, p.at),
            _ => (state, now, at),
        };
        self.entries.insert(
            msg.session.clone(),
            Entry {
                state,
                since,
                at,
                slot,
                pid: msg.pid.or(previous.as_ref().and_then(|p| p.pid)),
                transcript: msg.transcript.clone().or(previous.as_ref().and_then(|p| p.transcript.clone())),
                listed: previous.is_some_and(|p| p.listed),
                misses: 0,
            },
        );
    }

    fn free_slot(&self) -> usize {
        (0..).find(|slot| self.entries.values().all(|e| e.slot != *slot)).expect("unbounded range")
    }

    /// Drop sessions that expired or whose process died, and turn interrupted sessions back to idle.
    pub fn sweep(
        &mut self,
        now: Instant,
        at: SystemTime,
        alive: impl Fn(u32) -> bool,
        interrupted: impl Fn(&Path, SystemTime) -> bool,
    ) {
        self.entries.retain(|_, e| now.duration_since(e.since) < SESSION_TTL && e.pid.is_none_or(&alive));
        for e in self.entries.values_mut() {
            let busy = matches!(e.effective(now), State::Working | State::Ask);
            if busy && e.transcript.as_deref().is_some_and(|t| interrupted(t, e.at)) {
                (e.state, e.since, e.at) = (State::Idle, now, at);
            }
        }
    }

    /// Sync with the sessions Claude Code reports as running: add sessions we never heard from (daemon
    /// restarted, hooks missed), drop sessions that left the list (closed without `SessionEnd`), and return
    /// Working sessions to idle when Claude Code says the turn ended without a Stop hook (API error, crash mid-turn).
    pub fn reconcile(&mut self, live: &[Live], now: Instant, at: SystemTime) {
        self.ended.retain(|id| live.iter().any(|l| &l.session_id == id));
        for e in self.entries.values_mut() {
            e.misses = e.misses.saturating_add(1);
        }
        for l in live {
            if self.ended.contains(&l.session_id) {
                continue;
            }
            if let Some(e) = self.entries.get_mut(&l.session_id) {
                (e.listed, e.misses) = (true, 0);
                e.pid = e.pid.or(Some(l.pid));
                if e.state == State::Working && l.idle() && now.duration_since(e.since) >= IDLE_GRACE {
                    (e.state, e.since, e.at) = (State::Idle, now, at);
                }
                continue;
            }
            let state = if l.busy() { State::Working } else { State::Idle };
            let slot = self.free_slot();
            let entry = Entry { state, since: now, at, slot, pid: Some(l.pid), transcript: None, listed: true, misses: 0 };
            self.entries.insert(l.session_id.clone(), entry);
        }
        self.entries.retain(|_, e| !e.listed || e.misses < MISSING_LIMIT);
    }

    /// One line per session in slot order, for `claude-glow status`.
    pub fn describe(&self, now: Instant) -> Vec<String> {
        let mut all: Vec<_> = self.entries.iter().collect();
        all.sort_by_key(|(id, e)| (e.slot, id.as_str()));
        all.iter()
            .map(|(id, e)| {
                let pid = e.pid.map_or_else(|| "-".to_owned(), |p| p.to_string());
                format!("slot {} {:<8} {}s pid {pid} {id}", e.slot, e.effective(now).scene(), now.duration_since(e.since).as_secs())
            })
            .collect()
    }

    /// One state per live session in slot order (each session keeps its slot until it ends), at most `max` entries.
    /// Sessions past the limit are folded into the last slot at their highest priority.
    pub fn slots(&self, now: Instant, max: usize) -> Vec<State> {
        if max == 0 {
            return Vec::new();
        }
        let mut live: Vec<_> = self.entries.iter().filter(|(_, e)| now.duration_since(e.since) < SESSION_TTL).collect();
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

    fn live(id: &str, status: &str) -> Live {
        Live { pid: 7, session_id: id.into(), status: Some(status.into()) }
    }

    #[test]
    fn reconcile_adds_sessions_we_never_heard_from() {
        let now = Instant::now();
        let mut s = Sessions::default();

        s.reconcile(&[live("a", "idle"), live("b", "busy")], now, SystemTime::now());

        let mut slots = s.slots(now, 6);
        slots.sort();
        assert_eq!(slots, vec![State::Idle, State::Working]);
    }

    #[test]
    fn reconcile_ends_a_working_turn_claude_reports_idle_after_the_grace() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "a", t0);

        s.reconcile(&[live("a", "idle")], t0 + Duration::from_secs(1), SystemTime::now());
        assert_eq!(s.slots(t0 + Duration::from_secs(1), 6), vec![State::Working]);

        s.reconcile(&[live("a", "idle")], t0 + IDLE_GRACE, SystemTime::now());
        assert_eq!(s.slots(t0 + IDLE_GRACE, 6), vec![State::Idle]);
    }

    #[test]
    fn reconcile_leaves_questions_and_done_alone() {
        let t0 = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Ask, "a", t0);
        apply(&mut s, Event::Done, "b", t0);
        let later = t0 + Duration::from_secs(6);

        s.reconcile(&[live("a", "idle"), live("b", "idle")], later, SystemTime::now());

        assert_eq!(s.slots(later, 6), vec![State::Ask, State::Done]);
    }

    #[test]
    fn reconcile_keeps_existing_slots() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Start, "a", now);

        s.reconcile(&[live("b", "busy"), live("a", "idle")], now, SystemTime::now());

        assert_eq!(s.slots(now, 6), vec![State::Idle, State::Working]);
    }

    #[test]
    fn an_ended_session_is_not_re_added_while_claude_still_lists_it() {
        let now = Instant::now();
        let mut s = Sessions::default();
        s.reconcile(&[live("a", "idle")], now, SystemTime::now());

        apply(&mut s, Event::End, "a", now);
        s.reconcile(&[live("a", "idle")], now, SystemTime::now());
        apply(&mut s, Event::Working, "a", now);

        assert!(s.slots(now, 6).is_empty());
    }

    #[test]
    fn resuming_an_ended_session_brings_it_back() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::End, "a", now);

        apply(&mut s, Event::Start, "a", now);

        assert_eq!(s.slots(now, 6), vec![State::Idle]);
    }

    #[test]
    fn a_listed_session_that_leaves_the_list_is_dropped_after_two_misses() {
        let now = Instant::now();
        let mut s = Sessions::default();
        s.reconcile(&[live("a", "idle"), live("b", "idle")], now, SystemTime::now());

        s.reconcile(&[live("a", "idle")], now, SystemTime::now());
        assert_eq!(s.slots(now, 6).len(), 2, "one miss may be a file caught mid-write");

        s.reconcile(&[live("a", "idle")], now, SystemTime::now());
        assert_eq!(s.slots(now, 6), vec![State::Idle]);
    }

    #[test]
    fn hook_only_sessions_are_not_dropped_by_the_list() {
        let now = Instant::now();
        let mut s = Sessions::default();
        apply(&mut s, Event::Working, "manual", now);

        for _ in 0..3 {
            s.reconcile(&[], now, SystemTime::now());
        }

        assert_eq!(s.slots(now, 6), vec![State::Working]);
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
