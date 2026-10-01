use std::collections::HashMap;

use serde::Deserialize;

use crate::effects::Effect;

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
}
