# claude-glow

Turn your laptop's **lightbar** into a live status light for [Claude Code](https://claude.com/claude-code).

| Claude is… | Lightbar |
|---|---|
| not running | white |
| open, waiting for you | blue light sweeping slowly end to end and back |
| working | Claude-orange comet with a faint white tail, easing at each end |
| asking you a question / needs permission | red, pulsing |
| just finished | green for 8 s, then blue |

**Several sessions?** Each open session gets its own segment of the bar and shows its own state.
- **Two sessions** (the polished case): the bar splits into halves that animate as mirror images, with a dim seam in
  the middle so two sessions in the same state still read as two. Each session keeps its side until it closes.
- **3 to 6 sessions**: thirds, quarters and so on, down to one LED each.
- **More than 6**: the extras share the last LED at their highest priority, so a question is never hidden.

The keyboard backlight is left alone. Colours and effects are editable (see [Customising](#customising)).

> **Status:** tested on the **ASUS ROG Strix G16 (G614JU)** on Linux (CachyOS/Arch). Other ASUS Aura laptops
> may work by adding a [device profile](docs/profiles.md). This is an unofficial, reverse-engineered tool and is
> not affiliated with ASUS or Anthropic. Use at your own risk.

## Why this exists

`asusctl`/`asusd` expose a single colour zone for this laptop, so the lightbar can't be coloured on its own.
Reverse-engineering the HID protocol showed that a **streamed zoned frame** drives the 6 lightbar LEDs and the
4 keyboard zones independently. See [docs/protocol.md](docs/protocol.md) for the byte-level findings.

## How it works

```text
Claude Code hook ──► claude-glow hook <event> ──► Unix socket ──► claude-glow daemon
                                                                       │
                       session state + priority  ◄─────────────────────┤
                       scene (effect + colour)   ──► 20 fps frames ────┴──► /dev/hidraw0 ──► lightbar
```

- Hooks fire on Claude Code events and send one line to the daemon.
- The daemon tracks every open session and gives each one a segment of the bar, showing that session's state.
- It renders the scenes at 20 fps with smoothing between frames. The firmware only holds a custom colour while frames keep arriving.
- On stop (or crash) the daemon restores your saved lighting mode.

## Quick start

Requirements: Linux, a supported ASUS laptop, Rust (`cargo`), systemd, Claude Code.

```bash
git clone https://github.com/lebiraja/claude-glow
cd claude-glow
./scripts/install.sh
```

Then add the hooks from [`packaging/hooks.json`](packaging/hooks.json) to `~/.claude/settings.json`
(merge them into your existing `hooks` entries) and start a new Claude session. Full steps and
troubleshooting are in [docs/setup.md](docs/setup.md).

## Customising

Copy the bundled scenes and edit them:

```bash
mkdir -p ~/.config/claude-glow
cp crates/aura-hid/scenes_default.toml ~/.config/claude-glow/scenes.toml
systemctl --user restart claude-glow
```

```toml
[working]
effect = "comet"       # static | breathe | pulse | comet
colour = "#dd4c12"     # head colour
tail_colour = "#ffffff" # comet only: colour at the far end of the tail (optional)
period_ms = 1800       # one full trip there and back
tail = 0.5             # comet only: tail length as a fraction of the bar
floor = 0.02           # comet only: brightness far from the head (0..1)
```

The five scene names are `none`, `idle`, `working`, `ask` and `done`. The "done" hold time is the `DONE_HOLD`
constant in [`crates/claude-glow/src/state.rs`](crates/claude-glow/src/state.rs).

## Commands

```text
claude-glow daemon                 run the frame-streaming daemon (the systemd unit does this)
claude-glow hook <event>           called by Claude Code hooks; reads hook JSON on stdin
claude-glow send <event>           send an event by hand: start | working | ask | done | end
claude-glow probe <led|group>      light one LED (bar1..bar6, key1..key4) or group (bar, keys) for a few seconds
claude-glow restore                put the laptop back in its saved lighting mode
```

## Project layout

```text
crates/aura-hid/     library: device profiles, frame builder, effects, scenes, hidraw output
crates/claude-glow/  binary: daemon, session state machine, socket IPC, hook command
packaging/           systemd unit, udev rule, hook snippet
docs/                setup, architecture, protocol notes, adding a device
```

## Development

```bash
cargo test --workspace
cargo build --release
```

## License

[MIT](LICENSE)
