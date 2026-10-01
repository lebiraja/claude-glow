mod daemon;
mod ipc;
mod state;

use std::io::{self, Read};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::{Duration, Instant};

use aura_hid::{Frame, HidrawDevice, Profile, Rgb, Scenes};
use clap::{Parser, Subcommand};
use serde::Deserialize;

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
    notification_type: Option<String>,
}

fn scenes() -> Scenes {
    let user = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .map(|d| d.join("claude-glow/scenes.toml"));
    match user.and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(src) => Scenes::from_toml(&src).expect("invalid ~/.config/claude-glow/scenes.toml"),
        None => Scenes::bundled(),
    }
}

fn hook(event: Event) {
    let mut raw = String::new();
    let _ = io::stdin().read_to_string(&mut raw);
    let input: HookInput = serde_json::from_str(&raw).unwrap_or_default();
    if event == Event::Ask && input.notification_type.as_deref() == Some("idle_prompt") {
        return;
    }
    let _ = ipc::send(event, input.session_id.as_deref().unwrap_or("unknown"));
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
        Command::Send { event, session } => ipc::send(event, &session),
        Command::Probe { target, colour, secs } => probe(&profile, &target, colour, secs),
        Command::Restore => daemon::restore(&profile, &mut HidrawDevice::open(&profile)?),
    }
}
