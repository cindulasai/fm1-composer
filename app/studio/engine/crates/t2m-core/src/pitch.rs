//! Pitch as cents above the piece's tonic, and the tonic itself.
//!
//! Nothing inside the engine stores MIDI note numbers. A pitch becomes a MIDI
//! note plus a cents offset only at the edges (export, playback).

use serde::{Deserialize, Serialize};

const CENTS_PER_SEMITONE: i64 = 100;
const CENTS_PER_OCTAVE: i32 = 1200;
const MIDI_MAX: i64 = 127;
const HALF_SEMITONE: i64 = 50;
const A4_MIDI: f64 = 69.0;
const A4_HZ: f64 = 440.0;

/// A pitch measured in cents above the piece's tonic at octave 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Pitch {
    pub cents: i32,
}

impl Pitch {
    /// The same pitch `n` octaves higher (lower when `n` is negative). Saturates.
    pub fn octave_up(self, n: i32) -> Pitch {
        Pitch {
            cents: self
                .cents
                .saturating_add(n.saturating_mul(CENTS_PER_OCTAVE)),
        }
    }

    /// The nearest MIDI note and the remaining offset in cents.
    ///
    /// `tonic_midi` is the MIDI note of the tonic at octave 0 of this piece.
    /// The offset lies in `-50.0..=50.0`; an exact quarter-tone rounds up to the
    /// higher note. The note is clamped to `0..=127`.
    pub fn to_midi(self, tonic_midi: u8) -> (u8, f32) {
        let total = i64::from(tonic_midi) * CENTS_PER_SEMITONE + i64::from(self.cents);
        let nearest = (total + HALF_SEMITONE).div_euclid(CENTS_PER_SEMITONE);
        let note = nearest.clamp(0, MIDI_MAX);
        let offset = (total - note * CENTS_PER_SEMITONE).clamp(-HALF_SEMITONE, HALF_SEMITONE);
        (clamp_to_u8(note), offset as f32)
    }
}

/// The twelve pitch classes, named by their sharp spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PitchClass {
    C,
    Cs,
    D,
    Ds,
    E,
    F,
    Fs,
    G,
    Gs,
    A,
    As,
    B,
}

const PITCH_CLASSES: [PitchClass; 12] = [
    PitchClass::C,
    PitchClass::Cs,
    PitchClass::D,
    PitchClass::Ds,
    PitchClass::E,
    PitchClass::F,
    PitchClass::Fs,
    PitchClass::G,
    PitchClass::Gs,
    PitchClass::A,
    PitchClass::As,
    PitchClass::B,
];

impl PitchClass {
    /// Semitones above C, `0..=11`.
    pub fn semitone(self) -> u8 {
        self as u8
    }

    /// The pitch class `semitones` above C; any integer is accepted.
    pub fn from_semitone(semitones: i64) -> PitchClass {
        let index = semitones.rem_euclid(12) as usize;
        PITCH_CLASSES.get(index).copied().unwrap_or(PitchClass::C)
    }

    /// The sharp spelling, for example `"C#"`.
    pub fn name(self) -> &'static str {
        const NAMES: [&str; 12] = [
            "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
        ];
        NAMES.get(self as usize).copied().unwrap_or("C")
    }
}

/// Reads a note name such as `"Db"`, `"C#"`, `"E♭"` or `"f#"`.
///
/// One letter A–G in either case, then at most two accidentals of one kind.
/// Surrounding whitespace is ignored. Anything else is `None`.
pub fn parse_pitch_class(s: &str) -> Option<PitchClass> {
    let mut chars = s.trim().chars();
    let letter = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut shift = 0i64;
    for (count, c) in chars.enumerate() {
        let step = match c {
            '#' | '♯' => 1,
            'b' | '♭' => -1,
            _ => return None,
        };
        let mixed = shift * step < 0;
        if count >= 2 || mixed {
            return None;
        }
        shift += step;
    }
    Some(PitchClass::from_semitone(letter + shift))
}

/// The tonic of a piece: a named pitch class or an exact frequency.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Tonic {
    Class(PitchClass),
    Hz(f64),
}

impl Tonic {
    /// The MIDI note of this tonic in `octave` (middle C is octave 4) and its
    /// offset in cents from that note.
    ///
    /// A frequency keeps its pitch class and fine tuning and is placed in the
    /// requested octave. A frequency that is not a positive finite number is
    /// read as C.
    pub fn midi_and_offset(&self, octave: i32) -> (u8, f32) {
        let (class, offset) = match *self {
            Tonic::Class(class) => (class, 0.0),
            Tonic::Hz(hz) => class_of_hz(hz),
        };
        let octave = i64::from(octave.clamp(-1, 9));
        let mut note = (octave + 1) * 12 + i64::from(class.semitone());
        if note > MIDI_MAX {
            note -= 12;
        }
        (clamp_to_u8(note), offset)
    }
}

fn class_of_hz(hz: f64) -> (PitchClass, f32) {
    if !hz.is_finite() || hz <= 0.0 {
        return (PitchClass::C, 0.0);
    }
    let semitones = A4_MIDI + 12.0 * (hz / A4_HZ).log2();
    let nearest = (semitones + 0.5).floor();
    let offset = (semitones - nearest) * 100.0;
    // `nearest` is finite and small for any positive finite frequency.
    (PitchClass::from_semitone(nearest as i64), offset as f32)
}

fn clamp_to_u8(note: i64) -> u8 {
    u8::try_from(note.clamp(0, MIDI_MAX)).unwrap_or(0)
}
