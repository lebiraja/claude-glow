mod daemon;
mod ipc;
mod procfs;
mod state;

use std::io::{self, IsTerminal, Read};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use aura_hid::{Frame, HidrawDevice, Profile, Rgb, Scenes};
use clap::{Parser, Subcommand};
use serde::Deserialize;

use ipc::Message;
use state::Event;

#[derive(Parser)]
#[command(name = "claude-glow", about = "Drive the laptop lightbar from Claude Code state")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run the frame-streaming daemon (use the systemd user unit).
    Daemon,
    /// Called by Claude Code hooks: reads the hook JSON on stdin, forwards the event to the daemon.
    Hook { event: Event },
    /// Send an event by hand, e.g. `claude-glow send working`.
    Send {
        event: Event,
        #[arg(long, default_value = "manual")]
        session: String,
    },
    /// Light one LED (e.g. bar3) or group (keys, bar) for a few seconds, keyboard white.
    Probe {
        target: String,
        #[arg(default_value = "ff0000")]
        colour: Rgb,
        #[arg(long, default_value_t = 4)]
        secs: u64,
    },
    /// Return the laptop to its saved built-in lighting mode.
    Restore,
}

#[derive(Deserialize, Default)]
struct HookInput {
    session_id: Option<String>,
    transcript_path: Option<PathBuf>,
    notification_type: Option<String>,
}

/// Bundled scenes, overridden per scene by `~/.config/claude-glow/scenes.toml` when it exists and parses.
fn scenes() -> Scenes {
    let user = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("claude-glow/scenes.toml"));
    let Some(src) = user.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) else {
        return Scenes::bundled();
    };
    match Scenes::from_toml(&src) {
        Ok(custom) => Scenes::bundled().merged(custom),
        Err(e) => {
            eprintln!("ignoring invalid scenes.toml, using bundled scenes: {e}");
            Scenes::bundled()
        }
    }
}

/// Notifications that mean Claude is waiting on the user; others (idle reminders, auth) are not questions.
fn is_question(notification_type: Option<&str>) -> bool {
    notification_type.is_none_or(|t| matches!(t, "permission_prompt" | "elicitation_dialog"))
}

fn hook(event: Event) {
    let mut raw = String::new();
    if !io::stdin().is_terminal() {
        let _ = io::stdin().read_to_string(&mut raw);
    }
    let input: HookInput = serde_json::from_str(&raw).unwrap_or_default();
    if event == Event::Ask && !is_question(input.notification_type.as_deref()) {
        return;
    }
    let msg = Message {
        event,
        session: input.session_id.unwrap_or_else(|| "unknown".to_owned()),
        pid: procfs::claude_ancestor(),
        transcript: input.transcript_path,
    };
    let _ = ipc::send(&msg);
}

fn probe(profile: &Profile, target: &str, colour: Rgb, secs: u64) -> io::Result<()> {
    use aura_hid::HidSink;
    let mut dev = HidrawDevice::open(profile)?;
    let mut frame = Frame::new(profile);
    frame.set_group(profile, "keys", profile.keyboard_default);
    match profile.led_index(target) {
        Some(i) => frame.set(i, colour),
        None => frame.set_group(profile, target, colour),
    }
    let packet = frame.packet(profile);

    dev.write(&profile.packet.init)?;
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        dev.write(&packet)?;
        sleep(Duration::from_millis(50));
    }
    daemon::restore(profile, &mut dev)
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    let profile = Profile::g614ju();
    match cli.command {
        Command::Daemon => daemon::run(&profile, &scenes()),
        Command::Hook { event } => {
            hook(event);
            Ok(())
        }
        Command::Send { event, session } => ipc::send(&Message { event, session, pid: None, transcript: None }),
        Command::Probe { target, colour, secs } => probe(&profile, &target, colour, secs),
        Command::Restore => daemon::restore(&profile, &mut HidrawDevice::open(&profile)?),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_permission_and_elicitation_notifications_are_questions() {
        assert!(is_question(None));
        assert!(is_question(Some("permission_prompt")));
        assert!(is_question(Some("elicitation_dialog")));
        assert!(!is_question(Some("idle_prompt")));
        assert!(!is_question(Some("auth_success")));
    }
}
