#!/usr/bin/env python3
"""Merge packaging/hooks.json into ~/.claude/settings.json (idempotent, backs up the original).

Existing claude-glow entries are replaced, other hooks are kept, and the binary is referenced by absolute path.
"""
import json
import shutil
import sys
from pathlib import Path

repo = Path(__file__).resolve().parent.parent
settings_path = Path.home() / ".claude/settings.json"
binary = str(Path.home() / ".local/bin/claude-glow")

ours = json.loads((repo / "packaging/hooks.json").read_text())["hooks"]
settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
if settings_path.exists():
    shutil.copy2(settings_path, settings_path.with_name("settings.json.bak-claude-glow"))

hooks = settings.setdefault("hooks", {})
for event, groups in hooks.items():
    for group in groups:
        group["hooks"] = [h for h in group.get("hooks", []) if "claude-glow hook" not in h.get("command", "")]
    hooks[event] = [g for g in groups if g["hooks"]]
for event, groups in ours.items():
    for group in groups:
        for h in group["hooks"]:
            h["command"] = h["command"].replace("claude-glow", binary, 1)
    hooks.setdefault(event, []).extend(groups)
hooks = {k: v for k, v in hooks.items() if v}
settings["hooks"] = hooks

settings_path.write_text(json.dumps(settings, indent=2) + "\n")
print(f"claude-glow hooks installed in {settings_path} (backup: settings.json.bak-claude-glow)", file=sys.stderr)
