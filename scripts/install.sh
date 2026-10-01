#!/usr/bin/env bash
# Build claude-glow, install it for the current user and enable the systemd user service.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release
install -Dm755 target/release/claude-glow "$HOME/.local/bin/claude-glow"
install -Dm644 packaging/claude-glow.service "$HOME/.config/systemd/user/claude-glow.service"
systemctl --user daemon-reload
systemctl --user enable --now claude-glow
echo "Installed. Now add the hooks: see docs/setup.md (packaging/hooks.json)."
