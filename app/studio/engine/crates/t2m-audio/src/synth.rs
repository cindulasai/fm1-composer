//! The sounds: what one voice sounds like at a given moment.

/// A voice's character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timbre {
    /// A pure tone; used to measure pitch.
    Sine,
    Soft,
    Bright,
    Sub,
    Pad,
    Pluck,
    Kick,
    Snare,
    Hat,
    OpenHat,
    Crash,
    Tom,
    Click,
}

const TAU: f32 = std::f32::consts::TAU;

/// How a timbre's loudness behaves.
pub(crate) struct Shape {
    /// Seconds to reach full level.
    pub attack: f32,
    /// Seconds for the level to fall by 60 dB after the note ends.
    pub release: f32,
    /// Seconds to fall by 60 dB while held; None = holds level.
    pub decay: Option<f32>,
    /// Ignores the note's length and rings out on its own (drums).
    pub one_shot: bool,
    /// Voice level relative to the others.
    pub gain: f32,
}

impl Timbre {
    pub(crate) fn shape(self) -> Shape {
        let s = |attack, release, decay, one_shot, gain| Shape {
            attack,
            release,
            decay,
            one_shot,
            gain,
        };
        match self {
            Timbre::Sine => s(0.003, 0.08, None, false, 0.5),
            Timbre::Soft => s(0.006, 0.12, None, false, 0.45),
            Timbre::Bright => s(0.004, 0.10, None, false, 0.30),
            Timbre::Sub => s(0.006, 0.10, None, false, 0.55),
            Timbre::Pad => s(0.09, 0.35, None, false, 0.30),
            Timbre::Pluck => s(0.002, 0.08, Some(0.7), false, 0.35),
            Timbre::Kick => s(0.001, 0.05, Some(0.35), true, 0.9),
            Timbre::Snare => s(0.001, 0.05, Some(0.25), true, 0.6),
            Timbre::Hat => s(0.001, 0.03, Some(0.07), true, 0.3),
            Timbre::OpenHat => s(0.001, 0.05, Some(0.3), true, 0.3),
            Timbre::Crash => s(0.001, 0.1, Some(1.0), true, 0.3),
            Timbre::Tom => s(0.001, 0.05, Some(0.4), true, 0.7),
            Timbre::Click => s(0.0005, 0.02, Some(0.03), true, 0.4),
        }
    }

    /// The timbre of a drum-kit key (General MIDI numbering).
    pub fn of_kit_key(key: i32) -> Timbre {
        match key {
            35 | 36 => Timbre::Kick,
            37..=40 => Timbre::Snare,
            42 | 44 => Timbre::Hat,
            46 => Timbre::OpenHat,
            49 | 51..=53 | 55 | 57 | 59 => Timbre::Crash,
            41 | 43 | 45 | 47 | 48 | 50 => Timbre::Tom,
            _ => Timbre::Click,
        }
    }

    /// One sample, before the envelope. `phase` is 0..1 and already advanced; `age` is
    /// seconds since the note began; `noise` is a fresh value in -1..1.
    pub(crate) fn sample(self, phase: f32, age: f32, noise: f32) -> f32 {
        match self {
            Timbre::Sine => (phase * TAU).sin(),
            Timbre::Soft | Timbre::Pad => {
                let p = phase * TAU;
                p.sin() + 0.25 * (2.0 * p).sin() + 0.08 * (3.0 * p).sin()
            }
            Timbre::Bright | Timbre::Pluck => {
                let p = phase * TAU;
                let mut sum = 0.0;
                for k in 1..=6 {
                    sum += (k as f32 * p).sin() / k as f32;
                }
                sum * 0.8
            }
            Timbre::Sub => {
                let p = phase * TAU;
                p.sin() + 0.4 * (2.0 * p).sin()
            }
            Timbre::Kick | Timbre::Tom => (phase * TAU).sin(),
            Timbre::Snare => 0.6 * noise + 0.4 * (phase * TAU).sin(),
            Timbre::Hat | Timbre::OpenHat | Timbre::Crash => noise,
            Timbre::Click => {
                if age < 0.004 {
                    noise
                } else {
                    0.0
                }
            }
        }
    }

    /// The pitch of a drum voice a moment after it begins: kicks and toms start high and fall.
    pub(crate) fn drum_hz(self, base: f32, age: f32) -> f32 {
        match self {
            Timbre::Kick => 48.0 + 130.0 * (-age / 0.035).exp(),
            Timbre::Tom => base * (1.0 + 0.6 * (-age / 0.05).exp()),
            Timbre::Snare => 190.0,
            _ => base,
        }
    }
}
