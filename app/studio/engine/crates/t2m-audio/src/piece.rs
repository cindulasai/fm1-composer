//! A score as notes with exact frequencies, ready to be played.

use crate::synth::Timbre;
use t2m_core::brief::Role;
use t2m_core::{Score, Tick};

/// One note to play.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayNote {
    pub start: Tick,
    pub end: Tick,
    pub hz: f32,
    pub vel: u8,
    pub track: usize,
    pub timbre: Timbre,
    /// (ticks after the start, cents away from `hz`), for slides and risers.
    pub bend: Vec<(Tick, f32)>,
}

/// Everything the engine needs to play a score.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// Sorted by start tick.
    pub notes: Vec<PlayNote>,
    pub tempo_bpm: u16,
    pub tracks: usize,
    pub end: Tick,
}

fn timbre_of(role: Role) -> Timbre {
    match role {
        Role::Lead => Timbre::Bright,
        Role::Counter => Timbre::Soft,
        Role::Chords => Timbre::Soft,
        Role::Pad | Role::Drone => Timbre::Pad,
        Role::Bass => Timbre::Sub,
        Role::Arp => Timbre::Pluck,
        Role::Fx => Timbre::Sine,
        Role::Drums | Role::Percussion => Timbre::Click,
    }
}

/// The score as a playable piece. A pitch is exact to the cent: the tonic (in octave 0,
/// as the MIDI writer places it) plus the note's cents, as a frequency.
pub fn piece_of(score: &Score) -> Piece {
    let (tonic, offset) = score.meta.tonic.midi_and_offset(0);
    let tonic_semitones = f64::from(tonic) + f64::from(offset) / 100.0;
    let mut notes = Vec::new();
    for (track_index, track) in score.tracks.iter().enumerate() {
        let kit = matches!(track.role, Role::Drums | Role::Percussion);
        for note in &track.notes {
            let semitones = f64::from(note.pitch.cents) / 100.0;
            let (timbre, hz) = if kit {
                let key = semitones.round() as i32;
                (Timbre::of_kit_key(key), midi_hz(f64::from(key)) as f32)
            } else {
                (
                    timbre_of(track.role),
                    midi_hz(tonic_semitones + semitones) as f32,
                )
            };
            let bend = note
                .bend
                .as_ref()
                .map(|curve| {
                    curve
                        .0
                        .iter()
                        .map(|&(after, cents)| (after, cents as f32))
                        .collect()
                })
                .unwrap_or_default();
            notes.push(PlayNote {
                start: note.start,
                end: note.start.saturating_add(note.dur.max(1)),
                hz,
                vel: note.vel.clamp(1, 127),
                track: track_index,
                timbre,
                bend,
            });
        }
    }
    notes.sort_by_key(|n| n.start);
    Piece {
        notes,
        tempo_bpm: score.meta.tempo_bpm.max(1),
        tracks: score.tracks.len(),
        end: score.end(),
    }
}

/// Frequency of a (fractional) MIDI note number, A4 = 69 = 440 Hz.
pub(crate) fn midi_hz(note: f64) -> f64 {
    440.0 * 2f64.powf((note - 69.0) / 12.0)
}
