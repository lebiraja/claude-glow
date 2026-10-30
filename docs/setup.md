# Setup guide

## 1. Check your hardware

```bash
cat /sys/class/dmi/id/product_name        # e.g. "ROG Strix G614JU_G614JU"
lsusb | grep -i "0b05:19b6"               # ASUS N-KEY device
ls -l /dev/hidraw*
```

The bundled profile targets USB ID `0b05:19b6` (G614JU). For another laptop see [profiles.md](profiles.md).

## 2. Permissions

claude-glow writes straight to the `hidraw` node of the ASUS N-KEY device, as your user. If
`claude-glow probe bar1` fails with "permission denied":

```bash
sudo cp packaging/99-claude-glow.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

## 3. Build and install

```bash
./scripts/install.sh
```

This builds the release binary, installs it to `~/.local/bin/claude-glow` (make sure that is on your `PATH`),
installs the systemd user unit, (re)starts it, and merges the hooks into `~/.claude/settings.json` (step 5).
Re-run it after pulling changes: it restarts the daemon so the new binary is used.

## 4. Smoke test

```bash
claude-glow probe bar1 ff0000 --secs 3     # bar LED 1 turns red for 3 s, keyboard white
claude-glow send working                   # orange comet sweeps the bar
claude-glow status                         # slot 0 working ... manual
claude-glow send end                       # back to white
```

## 5. Add the Claude Code hooks

`install.sh` does this for you via `scripts/install-hooks.py`: it removes any old claude-glow entries, appends
the ones from `packaging/hooks.json` with the absolute binary path, keeps all other hooks, and backs the file up
to `settings.json.bak-claude-glow`. Run it on its own with `python3 scripts/install-hooks.py`. Hooks are read at
session start, so open a **new** Claude session afterwards. The hooks run synchronously with a 2 s timeout so events always arrive in order.

| Hook | Sends | Effect |
|---|---|---|
| `SessionStart` | `start` | blue sweep |
| `UserPromptSubmit`, `PostToolUse` | `working` | orange comet |
| `PreToolUse` (matcher `AskUserQuestion`), `Notification` | `ask` | red pulse |
| `Stop` | `done` | green for 8 s, then blue |
| `SessionEnd` | `end` | removes the session |

Only `permission_prompt` and `elicitation_dialog` notifications turn the bar red. Idle reminders are ignored, so it doesn't go red just because you walked away.

Pressing **Esc** to interrupt, or killing Claude, is detected automatically (see the edge-case table in [architecture.md](architecture.md)).

## Day to day

```bash
systemctl --user status claude-glow
systemctl --user restart claude-glow       # after editing scenes.toml
systemctl --user stop claude-glow          # restores your normal lighting
journalctl --user -u claude-glow -f
```

## Troubleshooting

| Symptom | Fix |
|---|---|
| Wrong colour / a session that isn't open | `claude-glow status` shows what the daemon tracks; `claude-glow send end --session <id>` removes one |
| Lights flicker, keyboard stays white | Another program streams to the same device. Find it: `for p in /proc/[0-9]*; do ls -l $p/fd 2>/dev/null \| grep -q hidraw0 && cat $p/comm; done`; stop it (e.g. `systemctl --user disable --now rogd`) |
| Sessions never show their state | Another tool rewrote the hooks; re-run `python3 scripts/install-hooks.py` |
| Changes not visible after rebuild | Re-run `./scripts/install.sh` (it restarts the service) |
| Lights never change | `systemctl --user is-active claude-glow`; start a **new** Claude session; check `ls $XDG_RUNTIME_DIR/claude-glow.sock` |
| `no hidraw device 0b05:19b6` | Your laptop uses a different device; see [profiles.md](profiles.md) |
| Permission denied | Install the udev rule (step 2) |
| Bar flashes then reverts | Another program (e.g. asusd) is rewriting the mode; keep the daemon running, it re-sends every 50 ms |
| Lights stuck after a crash | `claude-glow restore` |
| Binary not found by hooks | Use an absolute path in the hook command, e.g. `/home/you/.local/bin/claude-glow hook start` |

## Uninstall

```bash
claude-glow restore
systemctl --user disable --now claude-glow
rm ~/.local/bin/claude-glow ~/.config/systemd/user/claude-glow.service
```

Then remove the claude-glow entries from `~/.claude/settings.json`.
