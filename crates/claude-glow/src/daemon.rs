use std::io;
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use aura_hid::{Frame, HidSink, HidrawDevice, Profile, Rgb, Scenes};

use crate::ipc;
use crate::state::Sessions;

const FRAME_INTERVAL: Duration = Duration::from_millis(50);
const FADE: Duration = Duration::from_millis(300);

struct Fader {
    scene: &'static str,
    since: Instant,
    from: Rgb,
    last: Rgb,
}

pub fn restore(profile: &Profile, dev: &mut impl HidSink) -> io::Result<()> {
    profile.restore.iter().try_for_each(|p| dev.write(p))
}

pub fn run(profile: &Profile, scenes: &Scenes) -> io::Result<()> {
    let sessions = Arc::new(Mutex::new(Sessions::default()));
    let running = Arc::new(AtomicBool::new(true));
    {
        let running = running.clone();
        ctrlc::set_handler(move || running.store(false, Ordering::SeqCst)).map_err(io::Error::other)?;
    }

    let path = ipc::socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    {
        let sessions = sessions.clone();
        thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                for line in ipc::read_lines(stream) {
                    if let Some((event, id)) = ipc::parse_line(&line) {
                        sessions.lock().expect("sessions lock").apply(&id, event, Instant::now());
                    }
                }
            }
        });
    }

    let start = Instant::now();
    let mut dev: Option<HidrawDevice> = None;
    let mut fader = Fader { scene: "none", since: start, from: Rgb::BLACK, last: Rgb::BLACK };
    while running.load(Ordering::SeqCst) {
        let now = Instant::now();
        let scene = sessions.lock().expect("sessions lock").displayed(now).map_or("none", |s| s.scene());
        if scene != fader.scene {
            fader = Fader { scene, since: now, from: fader.last, last: fader.last };
        }
        if let Some(effect) = scenes.get(scene) {
            let target = effect.colour_at(now - start);
            fader.last = fader.from.lerp(target, (now - fader.since).as_secs_f32() / FADE.as_secs_f32());
        }

        if dev.is_none() {
            dev = HidrawDevice::open(profile).ok().and_then(|mut d| d.write(&profile.packet.init).ok().map(|()| d));
        }
        if let Some(d) = dev.as_mut() {
            let mut frame = Frame::new(profile);
            frame.set_group(profile, "keys", profile.keyboard_default);
            frame.set_group(profile, "bar", fader.last);
            if d.write(&frame.packet(profile)).is_err() {
                dev = None;
            }
        }
        sleep(FRAME_INTERVAL);
    }

    if let Some(mut d) = dev.or_else(|| HidrawDevice::open(profile).ok()) {
        restore(profile, &mut d)?;
    }
    let _ = std::fs::remove_file(&path);
    Ok(())
}
