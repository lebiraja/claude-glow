## 2026-10-30 — Lights flicker and keyboard stays white
**Symptom:** Bar and keyboard flicker constantly; keyboard stays white even while Claude is working.
**Root cause:** The rog-on-top daemon `rogd` (user service, enabled) was streaming frames to `/dev/hidraw0` at the same time as claude-glow; the two daemons' frames alternated and rogd's frames had a white keyboard. Confirmed by a fixed raw test frame flickering with claude-glow stopped, and `/proc/*/fd` showing `rogd` holding `hidraw0`.
**Fix:** `systemctl --user disable --now rogd`; claude-glow is the only writer. No code change.
**Verified:** only `claude-glow` holds `/dev/hidraw0`; service active.

