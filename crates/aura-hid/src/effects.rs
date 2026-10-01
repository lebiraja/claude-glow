use std::f32::consts::TAU;
use std::time::Duration;

use serde::Deserialize;

use crate::colour::Rgb;

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case")]
pub enum Effect {
    Static { colour: Rgb },
    Breathe { colour: Rgb, period_ms: u64 },
    Pulse { colour: Rgb, period_ms: u64 },
}

impl Effect {
    pub fn colour_at(&self, t: Duration) -> Rgb {
        match self {
            Effect::Static { colour } => *colour,
            Effect::Breathe { colour, period_ms } => colour.scale(0.15 + 0.85 * wave(t, *period_ms)),
            Effect::Pulse { colour, period_ms } => colour.scale(wave(t, *period_ms).powi(2)),
        }
    }
}

/// Smooth 0..1 wave that starts at 0.
fn wave(t: Duration, period_ms: u64) -> f32 {
    let phase = (t.as_millis() as u64 % period_ms.max(1)) as f32 / period_ms.max(1) as f32;
    0.5 - 0.5 * (phase * TAU).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CYAN: Rgb = Rgb::new(0, 255, 255);

    #[test]
    fn static_ignores_time() {
        let e = Effect::Static { colour: CYAN };

        assert_eq!(e.colour_at(Duration::from_millis(123)), CYAN);
    }

    #[test]
    fn breathe_is_dim_at_start_and_full_at_half_period() {
        let e = Effect::Breathe { colour: CYAN, period_ms: 1000 };

        assert_eq!(e.colour_at(Duration::ZERO), CYAN.scale(0.15));
        assert_eq!(e.colour_at(Duration::from_millis(500)), CYAN);
    }

    #[test]
    fn pulse_goes_fully_dark() {
        let e = Effect::Pulse { colour: CYAN, period_ms: 1000 };

        assert_eq!(e.colour_at(Duration::ZERO), Rgb::BLACK);
    }
}
