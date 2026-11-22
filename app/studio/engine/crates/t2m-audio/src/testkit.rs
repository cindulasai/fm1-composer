//! Pieces and measurements for the tests of this crate.

use crate::piece::{Piece, PlayNote};
use crate::synth::Timbre;

pub const RATE: u32 = 48_000;

pub fn note(start: i64, end: i64, hz: f32, track: usize, timbre: Timbre) -> PlayNote {
    PlayNote {
        start,
        end,
        hz,
        vel: 100,
        track,
        timbre,
        bend: Vec::new(),
    }
}

pub fn piece(notes: Vec<PlayNote>, tempo: u16, tracks: usize) -> Piece {
    let end = notes.iter().map(|n| n.end).max().unwrap_or(0);
    Piece {
        notes,
        tempo_bpm: tempo,
        tracks,
        end,
    }
}

/// Renders `frames` frames and returns the left channel.
pub fn left(engine: &mut crate::Engine, frames: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; frames * 2];
    engine.render(&mut out);
    out.iter().step_by(2).copied().collect()
}

pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Frequency from rising zero crossings (pure tones only), with linear interpolation.
pub fn hz_of(samples: &[f32]) -> f64 {
    let mut crossings: Vec<f64> = Vec::new();
    for i in 1..samples.len() {
        if samples[i - 1] < 0.0 && samples[i] >= 0.0 {
            let frac = f64::from(-samples[i - 1]) / f64::from(samples[i] - samples[i - 1]);
            crossings.push(i as f64 - 1.0 + frac);
        }
    }
    if crossings.len() < 3 {
        return 0.0;
    }
    let periods = (crossings.len() - 1) as f64;
    f64::from(RATE) * periods / (crossings[crossings.len() - 1] - crossings[0])
}

pub fn cents_between(a: f64, b: f64) -> f64 {
    1200.0 * (a / b).log2()
}
