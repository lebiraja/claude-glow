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
installs the systemd user unit and starts it.

## 4. Smoke test

```bash
claude-glow probe bar1 ff0000 --secs 3     # bar LED 1 turns red for 3 s, keyboard white
claude-glow send working                   # bar breathes amber
claude-glow send end                       # back to white
```

## 5. Add the Claude Code hooks

Merge `packaging/hooks.json` into the `hooks` object of `~/.claude/settings.json`. Hook arrays are lists, so
append these entries to any arrays you already have rather than replacing them. Hooks are read at session
start, so open a **new** Claude session afterwards.

| Hook | Sends | Effect |
|---|---|---|
| `SessionStart` | `start` | blue |
| `UserPromptSubmit`, `PostToolUse` | `working` | amber breathing |
| `PreToolUse` (matcher `AskUserQuestion`), `Notification` | `ask` | red pulse |
| `Stop` | `done` | green for 8 s, then blue |
| `SessionEnd` | `end` | removes the session |

`Notification` events of type `idle_prompt` are ignored, so the bar doesn't turn red just because you walked away.

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
| Lights never change | `systemctl --user is-active claude-glow`; start a **new** Claude session; check `ls $XDG_RUNTIME_DIR/claude-glow.sock` |
| `no hidraw device 0b05:19b6` | Your laptop uses a different device; see [profiles.md](profiles.md) |
| Permission denied | Install the udev rule (step 2) |
| Bar flashes then reverts | Another program (e.g. asusd) is rewriting the mode; keep the daemon running, it re-sends every 50 ms |
| Lights stuck after a crash | `claude-glow restore` |
| Binary not found by hooks | Use an absolute path in the hook command, e.g. `/home/you/.local/bin/claude-glow hook start` |

## Uninstall

```bash
systemctl --user disable --now claude-glow
rm ~/.local/bin/claude-glow ~/.config/systemd/user/claude-glow.service
```

Then remove the claude-glow entries from `~/.claude/settings.json`.
