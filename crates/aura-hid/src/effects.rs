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
    /// A bright head bouncing along the strip with a fading tail behind it. The head eases in and out at
    /// each end. `tail` is the trailing length as a fraction of the strip.
    Comet {
        colour: Rgb,
        period_ms: u64,
        #[serde(default = "default_tail")]
        tail: f32,
        /// Brightness (0..1) of LEDs far from the head.
        #[serde(default = "default_floor")]
        floor: f32,
        /// Colour of the far end of the tail; defaults to `colour`.
        tail_colour: Option<Rgb>,
    },
}

/// Glow ahead of the head, as a fraction of the trailing length.
const LEAD: f32 = 0.25;

fn default_tail() -> f32 {
    0.45
}

fn default_floor() -> f32 {
    0.05
}

impl Effect {
    /// Colour of the LED at `pos` (0.0..=1.0 along the strip) at time `t`.
    pub fn colour_at(&self, t: Duration, pos: f32) -> Rgb {
        match self {
            Effect::Static { colour } => *colour,
            Effect::Breathe { colour, period_ms } => colour.scale(0.15 + 0.85 * wave(t, *period_ms)),
            Effect::Pulse { colour, period_ms } => colour.scale(wave(t, *period_ms).powi(2)),
            Effect::Comet { colour, period_ms, tail, floor, tail_colour } => {
                let p = phase(t, *period_ms);
                let head = 0.5 - 0.5 * (p * TAU).cos();
                let behind = if p < 0.5 { head - pos } else { pos - head };
                let reach = if behind >= 0.0 { *tail } else { tail * LEAD };
                let near = (1.0 - behind.abs() / reach.max(0.01)).clamp(0.0, 1.0).powf(1.5);
                tail_colour.unwrap_or(*colour).lerp(*colour, near.powf(0.4)).scale(floor + (1.0 - floor) * near)
            }
        }
    }
}

fn phase(t: Duration, period_ms: u64) -> f32 {
    (t.as_millis() as u64 % period_ms.max(1)) as f32 / period_ms.max(1) as f32
}

/// Smooth 0..1 wave that starts at 0.
fn wave(t: Duration, period_ms: u64) -> f32 {
    0.5 - 0.5 * (phase(t, period_ms) * TAU).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CYAN: Rgb = Rgb::new(0, 255, 255);

    #[test]
    fn static_ignores_time() {
        let e = Effect::Static { colour: CYAN };

        assert_eq!(e.colour_at(Duration::from_millis(123), 0.0), CYAN);
    }

    #[test]
    fn breathe_is_dim_at_start_and_full_at_half_period() {
        let e = Effect::Breathe { colour: CYAN, period_ms: 1000 };

        assert_eq!(e.colour_at(Duration::ZERO, 0.0), CYAN.scale(0.15));
        assert_eq!(e.colour_at(Duration::from_millis(500), 0.0), CYAN);
    }

    #[test]
    fn pulse_goes_fully_dark() {
        let e = Effect::Pulse { colour: CYAN, period_ms: 1000 };

        assert_eq!(e.colour_at(Duration::ZERO, 0.0), Rgb::BLACK);
    }

    #[test]
    fn comet_head_sits_at_start_then_far_end_at_half_period() {
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.3, floor: 0.05, tail_colour: None };

        assert_eq!(e.colour_at(Duration::ZERO, 0.0), CYAN);
        assert_eq!(e.colour_at(Duration::ZERO, 1.0), CYAN.scale(0.05));
        assert_eq!(e.colour_at(Duration::from_millis(500), 1.0), CYAN);
    }

    #[test]
    fn comet_tail_fades_with_distance_from_head() {
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.5, floor: 0.05, tail_colour: None };

        let near = e.colour_at(Duration::ZERO, 0.1);
        let far = e.colour_at(Duration::ZERO, 0.4);

        assert!(near.g > far.g);
    }

    #[test]
    fn comet_blends_from_tail_colour_to_head_colour() {
        let violet = Rgb::new(138, 43, 255);
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.3, floor: 1.0, tail_colour: Some(violet) };

        assert_eq!(e.colour_at(Duration::ZERO, 0.0), CYAN);
        assert_eq!(e.colour_at(Duration::ZERO, 1.0), violet);
    }

    #[test]
    fn comet_tail_trails_behind_the_head() {
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.5, floor: 0.0, tail_colour: None };
        let quarter = Duration::from_millis(250);

        let behind = e.colour_at(quarter, 0.3);
        let ahead = e.colour_at(quarter, 0.7);

        assert!(behind.g > ahead.g);
    }

    #[test]
    fn comet_tail_swaps_sides_when_it_turns_around() {
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.5, floor: 0.0, tail_colour: None };
        let three_quarters = Duration::from_millis(750);

        let behind = e.colour_at(three_quarters, 0.7);
        let ahead = e.colour_at(three_quarters, 0.3);

        assert!(behind.g > ahead.g);
    }

    #[test]
    fn comet_eases_through_the_middle_at_quarter_period() {
        let e = Effect::Comet { colour: CYAN, period_ms: 1000, tail: 0.5, floor: 0.0, tail_colour: None };

        assert_eq!(e.colour_at(Duration::from_millis(250), 0.5), CYAN);
    }
}
