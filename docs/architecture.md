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
- `aura-hid::effects`: `Static`, `Breathe`, `Pulse`, evaluated as `colour_at(time)`
- `aura-hid::scene`: scene name to effect, loaded from TOML
- `aura-hid::device`: `HidSink` trait and the `HidrawDevice` that finds `/dev/hidrawN` by VID:PID
- `claude-glow::state`: per-session state, priority resolution, "done" hold, session expiry
- `claude-glow::ipc`: Unix socket path and the `<event> <session_id>` line protocol
- `claude-glow::daemon`: socket listener thread and the 20 fps render loop

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
State priority is `Ask > Working > Done > Idle`; no sessions shows the `none` scene. `Done` decays to `Idle`
after `DONE_HOLD` using timestamps (no timers). Sessions expire after 12 h without an event. Events:
`start`, `working`, `ask`, `done`, `end`.

## Failure behaviour
- The hook command always exits 0, even if the daemon is down, so Claude is never blocked.
- If the HID write fails (resume from suspend, device re-enumeration), the daemon reopens the device on the next frame.
- On SIGTERM/SIGINT the daemon writes the saved built-in mode back so lighting returns to normal.

## Configuration
- `~/.config/claude-glow/scenes.toml` (optional): overrides the bundled scenes.
- `XDG_RUNTIME_DIR`: location of the socket (`claude-glow.sock`).
- No other environment variables or secrets.
