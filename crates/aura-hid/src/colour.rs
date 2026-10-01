use std::str::FromStr;

use serde::{de, Deserialize, Deserializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const BLACK: Rgb = Rgb { r: 0, g: 0, b: 0 };

    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn scale(self, k: f32) -> Self {
        let f = |c: u8| (c as f32 * k.clamp(0.0, 1.0)).round() as u8;
        Self::new(f(self.r), f(self.g), f(self.b))
    }

    pub fn lerp(self, to: Rgb, k: f32) -> Self {
        let k = k.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * k).round() as u8;
        Self::new(f(self.r, to.r), f(self.g, to.g), f(self.b, to.b))
    }
}

impl FromStr for Rgb {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 {
            return Err(format!("colour must be 6 hex digits, got {s:?}"));
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|e| format!("{s:?}: {e}"));
        Ok(Self::new(byte(0)?, byte(2)?, byte(4)?))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_hash() {
        assert_eq!("#00ffff".parse(), Ok(Rgb::new(0, 255, 255)));
        assert_eq!("ff00ff".parse(), Ok(Rgb::new(255, 0, 255)));
    }

    #[test]
    fn rejects_bad_length_and_digits() {
        assert!("#fff".parse::<Rgb>().is_err());
        assert!("#zzzzzz".parse::<Rgb>().is_err());
    }
}
