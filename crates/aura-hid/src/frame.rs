use crate::colour::Rgb;
use crate::profile::Profile;

#[derive(Debug, Clone)]
pub struct Frame {
    colours: Vec<Rgb>,
}

impl Frame {
    pub fn new(profile: &Profile) -> Self {
        Self { colours: vec![Rgb::BLACK; profile.leds.len()] }
    }

    pub fn set(&mut self, index: usize, rgb: Rgb) {
        self.colours[index] = rgb;
    }

    pub fn set_group(&mut self, profile: &Profile, group: &str, rgb: Rgb) {
        for i in profile.group_indices(group) {
            self.colours[i] = rgb;
        }
    }

    pub fn packet(&self, profile: &Profile) -> Vec<u8> {
        let mut out = vec![0u8; profile.packet.len];
        out[..profile.packet.header.len()].copy_from_slice(&profile.packet.header);
        for (led, c) in profile.leds.iter().zip(&self.colours) {
            out[led.offset..led.offset + 3].copy_from_slice(&[c.r, c.g, c.b]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: Rgb = Rgb::new(255, 0, 0);

    #[test]
    fn step_a_frame_matches_the_bytes_that_worked_on_hardware() {
        let p = Profile::g614ju();
        let mut f = Frame::new(&p);
        for (name, c) in [("key1", RED), ("key2", Rgb::new(0, 255, 0)), ("key3", Rgb::new(0, 0, 255)), ("key4", Rgb::new(255, 255, 0))] {
            f.set(p.led_index(name).unwrap(), c);
        }
        f.set_group(&p, "bar", Rgb::new(255, 0, 255));

        let pkt = f.packet(&p);

        assert_eq!(pkt.len(), 64);
        assert_eq!(&pkt[..5], &[0x5d, 0xbc, 0x01, 0x01, 0x04]);
        assert_eq!(&pkt[9..21], &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0]);
        assert_eq!(&pkt[27..45], &[255, 0, 255].repeat(6)[..]);
    }

    #[test]
    fn single_bar_led_sets_only_its_offset() {
        let p = Profile::g614ju();
        let mut f = Frame::new(&p);
        f.set(p.led_index("bar3").unwrap(), RED);

        let pkt = f.packet(&p);

        assert_eq!(&pkt[33..36], &[255, 0, 0]);
        assert!(pkt[9..33].iter().chain(&pkt[36..]).all(|&b| b == 0));
    }
}
