use std::collections::HashMap;

use serde::Deserialize;

use crate::effects::Effect;
#[cfg(test)]
use crate::colour::Rgb;

const DEFAULT: &str = include_str!("../scenes_default.toml");

#[derive(Debug, Deserialize)]
pub struct Scenes(HashMap<String, Effect>);

impl Scenes {
    pub fn from_toml(src: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(src)
    }

    pub fn bundled() -> Self {
        Self::from_toml(DEFAULT).expect("bundled scenes are valid")
    }

    /// Scenes from `other` replace same-named scenes in `self`; scenes `other` doesn't define are kept.
    pub fn merged(mut self, other: Scenes) -> Self {
        self.0.extend(other.0);
        self
    }

    pub fn get(&self, name: &str) -> Option<&Effect> {
        self.0.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_scenes_cover_every_claude_state() {
        let s = Scenes::bundled();

        for name in ["none", "idle", "working", "ask", "done"] {
            assert!(s.get(name).is_some(), "missing scene {name}");
        }
    }

    #[test]
    fn unknown_effect_is_rejected() {
        assert!(Scenes::from_toml("[x]\neffect = \"disco\"\n").is_err());
    }

    #[test]
    fn merging_overrides_by_name_and_keeps_the_rest() {
        let user = Scenes::from_toml("[idle]\neffect = \"static\"\ncolour = \"#123456\"\n").unwrap();

        let merged = Scenes::bundled().merged(user);

        assert!(matches!(merged.get("idle"), Some(Effect::Static { colour }) if *colour == Rgb::new(0x12, 0x34, 0x56)));
        assert!(merged.get("working").is_some());
    }
}
