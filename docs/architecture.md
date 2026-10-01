# Architecture

## Overview
claude-glow maps Claude Code lifecycle events to lightbar effects on an ASUS laptop. Claude Code hooks call a
tiny CLI, which forwards events to a long-running daemon that streams HID frames to the Aura controller.

## Tech stack
- Rust 2021 workspace, std threads (no async runtime)
- `clap` (CLI), `serde` + `toml` + `serde_json` (profiles, scenes, hook input), `ctrlc` (graceful shutdown)
- Linux `hidraw` + systemd user service

## Modules
- `aura-hid::profile`: device profile (USB IDs, packet header, LED names and byte offsets, restore packets)
- `aura-hid::frame`: LED colours to a 64-byte packet via the profile
- `aura-hid::effects`: `Static`, `Breathe`, `Pulse`, `Comet`, evaluated as `colour_at(time, position)` per LED
- `aura-hid::scene`: scene name to effect, loaded from TOML
- `aura-hid::device`: `HidSink` trait and the `HidrawDevice` that finds `/dev/hidrawN` by VID:PID
- `claude-glow::state`: per-session state, ordered slots (with priority folding past 6), "done" hold, session expiry
- `claude-glow::ipc`: Unix socket path and the `<event> <session_id>` line protocol
- `claude-glow::daemon`: socket listener thread, per-segment bar rendering and the 20 fps loop

## Flow
```text
 Claude Code            claude-glow hook          daemon
┌────────────┐  stdin   ┌──────────────┐  line  ┌─────────────────────────┐
│ hook event │ ───────► │ parse JSON   │ ─────► │ Sessions::apply         │
└────────────┘  JSON    │ session_id   │ socket │ displayed() = max state │
                        └──────────────┘        │ scene → Effect          │
                                                │ crossfade (300 ms)      │
                                                │ Frame → 64-byte packet  │
                                                └───────────┬─────────────┘
                                                            │ every 50 ms
                                                            ↓
                                              /dev/hidrawN (ASUS N-KEY)
                                                            ↓
                                                  lightbar + keyboard zones
```

## State model
Each live session owns a contiguous segment of the bar, ordered by when it started. LEDs are divided as evenly as
possible (extras go to the earliest sessions). Beyond one session per LED, overflow sessions fold into the last
slot by priority `Ask > Working > Done > Idle`. No sessions shows the `none` scene. `Done` decays to `Idle` after
`DONE_HOLD` using timestamps (no timers). Sessions expire after 12 h without an event. Events:
`start`, `working`, `ask`, `done`, `end`.

Effects are evaluated per LED with a position along the segment (0..1), which is how the comet moves. The comet head
eases in and out at each end and its tail trails behind it.

## Failure behaviour
- The hook command always exits 0, even if the daemon is down, so Claude is never blocked.
- If the HID write fails (resume from suspend, device re-enumeration), the daemon reopens the device on the next frame.
- On SIGTERM/SIGINT the daemon writes the saved built-in mode back so lighting returns to normal.

## Configuration
- `~/.config/claude-glow/scenes.toml` (optional): overrides the bundled scenes.
- `XDG_RUNTIME_DIR`: location of the socket (`claude-glow.sock`).
- No other environment variables or secrets.
