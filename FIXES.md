## 2026-10-30 — Keyboard and lightbar segments on opposite sides
**Symptom:** A session's segment was on the left of the keyboard but on the right of the lightbar (e.g. "done" green).
**Root cause:** `g614ju.toml` listed the keyboard zones left to right but the bar LEDs in offset order, which is right to left (offset 27 is the rightmost LED). Segments are laid out in profile order, so the bar came out mirrored.
**Fix:** The bar LEDs are now listed left to right (`bar6`/42 … `bar1`/27) in `crates/aura-hid/profiles/g614ju.toml`. A test asserts the order (`crates/aura-hid/src/profile.rs`, `bundled_profile_lists_groups_left_to_right`), and the rule is documented in `docs/profiles.md`.
**Verified:** 62 tests pass; on hardware Lebi confirmed orange (working) on the left of both the keyboard and the bar, blue on the right.

## 2026-10-30 — Orange bar while both sessions were idle
**Symptom:** Two idle sessions, but the bar and keyboard showed the orange "working" scene.
**Root cause:** (1) The rog-on-top setup had rewritten the hooks in `~/.claude/settings.json` to call `rogctl`, so claude-glow got no session events. (2) A manual `claude-glow hook working` with empty stdin created a session called `unknown`, tied to a live claude pid, that stayed Working for 30 min. (3) `scripts/install.sh` used `enable --now`, which doesn't restart a running daemon.
**Fix:** Hooks pointed back to `claude-glow` (backup in `settings.json.bak-pre-glow-restore`). Hooks without a `session_id` are now dropped (`crates/claude-glow/src/main.rs`, `hook`). The daemon reconciles with `~/.claude/sessions/*.json` every second (`registry.rs`, `Sessions::reconcile` in `state.rs`). `install.sh` now restarts the service.
**Verified:** 62 tests pass; `claude-glow status` lists exactly the 2 running sessions with the correct states.

## 2026-10-30 — Lights flicker and keyboard stays white
**Symptom:** Bar and keyboard flicker constantly; keyboard stays white even while Claude is working.
**Root cause:** The rog-on-top daemon `rogd` (user service, enabled) was streaming frames to `/dev/hidraw0` at the same time as claude-glow; the two daemons' frames alternated and rogd's frames had a white keyboard. Confirmed by a fixed raw test frame flickering with claude-glow stopped, and `/proc/*/fd` showing `rogd` holding `hidraw0`.
**Fix:** `systemctl --user disable --now rogd`; claude-glow is the only writer. No code change.
**Verified:** only `claude-glow` holds `/dev/hidraw0`; service active.

