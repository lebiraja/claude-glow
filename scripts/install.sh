#!/usr/bin/env bash
# Build claude-glow, install it for the current user, enable the systemd user service and add the Claude Code hooks.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
install -Dm755 target/release/claude-glow "$HOME/.local/bin/claude-glow"
install -Dm644 packaging/claude-glow.service "$HOME/.config/systemd/user/claude-glow.service"
systemctl --user daemon-reload
systemctl --user enable claude-glow
systemctl --user restart claude-glow
python3 scripts/install-hooks.py
echo "Installed. Open a new Claude session; check with: claude-glow status"
