use std::io;
use std::os::unix::net::UnixListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, sleep};
use std::time::{Duration, Instant};

use aura_hid::{Frame, HidSink, HidrawDevice, Profile, Rgb, Scenes};

use crate::ipc;
use crate::state::{Sessions, State};

const FRAME_INTERVAL: Duration = Duration::from_millis(50);
/// Fraction of the remaining distance to the target covered per frame (smooth fades, soft comet trail).
const SMOOTHING: f32 = 0.4;

pub fn restore(profile: &Profile, dev: &mut impl HidSink) -> io::Result<()> {
    profile.restore.iter().try_for_each(|p| dev.write(p))
}

/// Target colour for every bar LED. Each live session owns a contiguous segment of the bar;
/// with no sessions the whole bar shows the `none` scene.
fn bar_targets(scenes: &Scenes, slots: &[State], leds: usize, t: Duration) -> Vec<Rgb> {
    let names: Vec<&str> = if slots.is_empty() { vec!["none"] } else { slots.iter().map(|s| s.scene()).collect() };
    let (base, extra) = (leds / names.len(), leds % names.len());
    let mut out = Vec::with_capacity(leds);
    for (k, name) in names.iter().enumerate() {
        let len = base + usize::from(k < extra);
        for j in 0..len {
            let pos = if len > 1 { j as f32 / (len - 1) as f32 } else { 0.5 };
            out.push(scenes.get(name).map_or(Rgb::BLACK, |e| e.colour_at(t, pos)));
        }
    }
    out
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

    let bar: Vec<usize> = profile.group_indices("bar").collect();
    let mut shown = vec![[0.0_f32; 3]; bar.len()];
    let start = Instant::now();
    let mut dev: Option<HidrawDevice> = None;
    while running.load(Ordering::SeqCst) {
        let now = Instant::now();
        let slots = sessions.lock().expect("sessions lock").slots(now, bar.len());
        let targets = bar_targets(scenes, &slots, bar.len(), now - start);

        let mut frame = Frame::new(profile);
        frame.set_group(profile, "keys", profile.keyboard_default);
        for ((led, cur), target) in bar.iter().zip(&mut shown).zip(&targets) {
            for (c, t) in cur.iter_mut().zip([target.r, target.g, target.b]) {
                *c += (f32::from(t) - *c) * SMOOTHING;
            }
            frame.set(*led, Rgb::new(cur[0].round() as u8, cur[1].round() as u8, cur[2].round() as u8));
        }

        if dev.is_none() {
            dev = HidrawDevice::open(profile).ok().and_then(|mut d| d.write(&profile.packet.init).ok().map(|()| d));
        }
        if let Some(d) = dev.as_mut() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn static_scenes() -> Scenes {
        Scenes::from_toml(
            "[none]\neffect = \"static\"\ncolour = \"#ffffff\"\n\
             [idle]\neffect = \"static\"\ncolour = \"#0000ff\"\n\
             [done]\neffect = \"static\"\ncolour = \"#00ff00\"\n\
             [ask]\neffect = \"static\"\ncolour = \"#ff0000\"\n",
        )
        .expect("test scenes are valid")
    }

    fn amber(t: &[Rgb]) -> usize {
        t.iter().filter(|c| c.r > 200).count()
    }

    #[test]
    fn no_sessions_fills_bar_with_none_scene() {
        let targets = bar_targets(&Scenes::bundled(), &[], 6, Duration::ZERO);

        assert_eq!(targets, vec![Rgb::new(255, 255, 255); 6]);
    }

    #[test]
    fn two_sessions_split_the_bar_in_halves() {
        let targets = bar_targets(&static_scenes(), &[State::Idle, State::Done], 6, Duration::ZERO);

        assert_eq!(&targets[..3], &[Rgb::new(0, 0, 255); 3]);
        assert_eq!(&targets[3..], &[Rgb::new(0, 255, 0); 3]);
    }

    #[test]
    fn four_sessions_share_six_leds_two_two_one_one() {
        let slots = [State::Idle, State::Done, State::Ask, State::Idle];

        let targets = bar_targets(&static_scenes(), &slots, 6, Duration::ZERO);

        assert_eq!(targets[0], targets[1]);
        assert_eq!(targets[2], targets[3]);
        assert_ne!(targets[1], targets[2]);
        assert_eq!(targets.len(), 6);
    }

    #[test]
    fn six_sessions_get_one_led_each() {
        let slots = [State::Idle, State::Done, State::Idle, State::Done, State::Idle, State::Done];

        let targets = bar_targets(&static_scenes(), &slots, 6, Duration::ZERO);

        assert_eq!(targets[0], Rgb::new(0, 0, 255));
        assert_eq!(targets[1], Rgb::new(0, 255, 0));
        assert_eq!(targets[5], Rgb::new(0, 255, 0));
    }

    #[test]
    fn working_comet_lights_the_head_end_first_and_moves() {
        let scenes = Scenes::bundled();

        let start = bar_targets(&scenes, &[State::Working], 6, Duration::ZERO);
        let middle = bar_targets(&scenes, &[State::Working], 6, Duration::from_millis(900));

        assert_eq!(amber(&start), 1);
        assert!(start[0].r > start[5].r);
        assert!(middle[5].r > middle[0].r);
    }
}
