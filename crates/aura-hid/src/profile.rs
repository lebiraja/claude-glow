use serde::Deserialize;

use crate::colour::Rgb;

const G614JU: &str = include_str!("../profiles/g614ju.toml");

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("invalid profile TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("LED {name:?} offset {offset} does not fit in a {len}-byte packet")]
    OffsetOutOfRange { name: String, offset: usize, len: usize },
    #[error("header is {header} bytes but packet length is {len}")]
    HeaderTooLong { header: usize, len: usize },
    #[error("duplicate LED name {0:?}")]
    DuplicateLed(String),
}

#[derive(Debug, Deserialize)]
pub struct PacketSpec {
    pub len: usize,
    pub init: Vec<u8>,
    pub header: Vec<u8>,
}

#[derive(Debug, Deserialize)]
pub struct Led {
    pub name: String,
    pub group: String,
    pub offset: usize,
}

#[derive(Debug, Deserialize)]
pub struct Profile {
    pub name: String,
    pub vid: u16,
    pub pid: u16,
    pub keyboard_default: Rgb,
    pub packet: PacketSpec,
    pub restore: Vec<Vec<u8>>,
    pub leds: Vec<Led>,
}

impl Profile {
    pub fn from_toml(src: &str) -> Result<Self, ProfileError> {
        let profile: Profile = toml::from_str(src)?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn g614ju() -> Self {
        Self::from_toml(G614JU).expect("bundled g614ju profile is valid")
    }

    fn validate(&self) -> Result<(), ProfileError> {
        let len = self.packet.len;
        if self.packet.header.len() > len {
            return Err(ProfileError::HeaderTooLong { header: self.packet.header.len(), len });
        }
        for (i, led) in self.leds.iter().enumerate() {
            if led.offset + 3 > len {
                return Err(ProfileError::OffsetOutOfRange { name: led.name.clone(), offset: led.offset, len });
            }
            if self.leds[..i].iter().any(|other| other.name == led.name) {
                return Err(ProfileError::DuplicateLed(led.name.clone()));
            }
        }
        Ok(())
    }

    pub fn led_index(&self, name: &str) -> Option<usize> {
        self.leds.iter().position(|l| l.name == name)
    }

    pub fn group_indices<'a>(&'a self, group: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.leds.iter().enumerate().filter(move |(_, l)| l.group == group).map(|(i, _)| i)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_profile_loads_with_four_keys_and_six_bar_leds() {
        let p = Profile::g614ju();

        assert_eq!((p.vid, p.pid), (0x0b05, 0x19b6));
        assert_eq!(p.group_indices("keys").count(), 4);
        assert_eq!(p.group_indices("bar").count(), 6);
        assert_eq!(p.led_index("bar1"), Some(4));
    }

    #[test]
    fn rejects_offset_past_packet_end() {
        let src = G614JU.replace("offset = 42", "offset = 62");

        assert!(matches!(Profile::from_toml(&src), Err(ProfileError::OffsetOutOfRange { .. })));
    }

    #[test]
    fn rejects_duplicate_led_names() {
        let src = G614JU.replace("name = \"bar6\"", "name = \"bar5\"");

        assert!(matches!(Profile::from_toml(&src), Err(ProfileError::DuplicateLed(_))));
    }
}
