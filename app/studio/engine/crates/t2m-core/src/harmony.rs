//! Chords from roman numerals, for tonal systems that support harmony.
//!
//! Reading rules, chosen to match how pop, rock and jazz charts are written:
//!
//! - Case gives the quality: `IV` is major, `iv` is minor, whatever the scale.
//! - A plain numeral takes its root from the system's own scale, so `VII` in a
//!   minor mode is the lowered seventh.
//! - A numeral with an accidental is read against the major scale, so `bVII`
//!   is 1000 cents above the tonic in every mode.
//! - `7` is always a minor seventh (`V7`, `ii7`); `maj7` is a major seventh.
//! - `X/Y` with a numeral after the slash is an applied chord (`V/V`); with
//!   `3`, `5` or `7` after the slash it names the chord tone in the bass.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Pitch;
use crate::tonal::{TonalSystem, Tuning};

const OCTAVE: i32 = 1200;
const SEMITONE: i32 = 100;
const MAX_LABEL_CHARS: usize = 24;
const MAX_ACCIDENTALS: usize = 2;
const MAJOR_SCALE: [i32; 7] = [0, 200, 400, 500, 700, 900, 1100];
const NUMERALS: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

/// Why a roman numeral could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HarmonyError {
    #[error("the chord symbol is empty")]
    Empty,
    #[error("the chord symbol is longer than {MAX_LABEL_CHARS} characters")]
    TooLong,
    #[error("the tonal system {system} does not support chords")]
    NotChordable { system: String },
    #[error("no roman numeral I to VII at the start of {0:?}")]
    Numeral(String),
    #[error("unknown chord type {0:?}")]
    Suffix(String),
    #[error("unknown bass or target {0:?} after the slash")]
    Slash(String),
}

/// A chord: its tones in cents above the tonic, stacked upward from the root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chord {
    /// The scale degree of the root, from 0.
    pub root_degree: i32,
    /// Root first, then each further tone above it. The root lies in the first octave.
    pub tones: Vec<Pitch>,
    /// The chord tone meant to sound lowest; the root unless a slash bass was given.
    pub bass: Pitch,
    /// The symbol as written.
    pub label: String,
}

/// Reads a roman-numeral chord symbol in the given system and tuning.
///
/// Supports I–VII in either case, `b`/`#` accidentals, qualities (`m`, `dim`,
/// `°`, `ø`, `aug`, `+`), extensions (`6`, `7`, `maj7`, `9`, `maj9`, `11`,
/// `13`, `add9`), `sus2`/`sus4`, a slash bass (`V/3`) and applied chords (`V/V`).
pub fn parse_roman(s: &str, system: &TonalSystem, tuning: &Tuning) -> Result<Chord, HarmonyError> {
    if !system.chordable {
        return Err(HarmonyError::NotChordable {
            system: system.id.clone(),
        });
    }
    let label = s.trim();
    if label.is_empty() {
        return Err(HarmonyError::Empty);
    }
    if label.chars().count() > MAX_LABEL_CHARS {
        return Err(HarmonyError::TooLong);
    }
    let (head, after_slash) = match label.split_once('/') {
        Some((head, tail)) => (head, Some(tail)),
        None => (label, None),
    };
    let (numeral, suffix) = read_numeral(head)?;
    let shape = read_suffix(suffix, numeral.upper)?;

    let mut root = numeral.root(system, tuning);
    let mut root_degree = numeral.degree;
    let mut bass_interval = 0;
    if let Some(tail) = after_slash {
        match tail {
            "3" => bass_interval = shape.third,
            "5" => bass_interval = shape.fifth,
            "7" => {
                bass_interval = shape
                    .seventh
                    .ok_or_else(|| HarmonyError::Slash(tail.to_string()))?;
            }
            _ => {
                let target = read_target(tail)?;
                let target_root = target.root(system, tuning);
                root = (target_root + numeral.major_interval()).rem_euclid(OCTAVE);
                root_degree = system.nearest_degree(tuning, Pitch { cents: root });
            }
        }
    }

    let tones = shape
        .intervals()
        .into_iter()
        .map(|interval| Pitch {
            cents: root + interval,
        })
        .collect();
    Ok(Chord {
        root_degree,
        tones,
        bass: Pitch {
            cents: root + bass_interval,
        },
        label: label.to_string(),
    })
}

struct Numeral {
    degree: i32,
    accidental: i32,
    upper: bool,
}

impl Numeral {
    /// Cents above the tonic when read against the major scale.
    fn major_interval(&self) -> i32 {
        let index = usize::try_from(self.degree).unwrap_or(0);
        let natural = MAJOR_SCALE.get(index).copied().unwrap_or(0);
        (natural + self.accidental * SEMITONE).rem_euclid(OCTAVE)
    }

    /// The root in this system: the system's own degree for a plain numeral
    /// in a seven-degree system, otherwise the major-scale reading.
    fn root(&self, system: &TonalSystem, tuning: &Tuning) -> i32 {
        let plain = self.accidental == 0;
        if plain && system.degrees() == 7 {
            system.pitch(tuning, self.degree).cents.rem_euclid(OCTAVE)
        } else {
            self.major_interval()
        }
    }
}

fn read_numeral(text: &str) -> Result<(Numeral, &str), HarmonyError> {
    let bad = || HarmonyError::Numeral(text.to_string());
    let mut accidental = 0;
    let mut rest = text;
    for _ in 0..MAX_ACCIDENTALS {
        let Some(first) = rest.chars().next() else {
            break;
        };
        let shift = match first {
            'b' | '♭' => -1,
            '#' | '♯' => 1,
            _ => break,
        };
        accidental += shift;
        rest = rest.get(first.len_utf8()..).unwrap_or("");
    }
    let letters = rest
        .find(|c: char| !matches!(c, 'I' | 'V' | 'i' | 'v'))
        .unwrap_or(rest.len());
    let (numeral, suffix) = rest.split_at(letters);
    let upper = numeral.chars().all(|c| c.is_ascii_uppercase());
    let lower = numeral.chars().all(|c| c.is_ascii_lowercase());
    if !upper && !lower {
        return Err(bad());
    }
    let wanted = numeral.to_ascii_uppercase();
    let degree = NUMERALS
        .iter()
        .position(|name| *name == wanted)
        .ok_or_else(bad)?;
    let numeral = Numeral {
        degree: i32::try_from(degree).unwrap_or(0),
        accidental,
        upper,
    };
    Ok((numeral, suffix))
}

fn read_target(text: &str) -> Result<Numeral, HarmonyError> {
    let slash = || HarmonyError::Slash(text.to_string());
    let (numeral, rest) = read_numeral(text).map_err(|_| slash())?;
    if rest.is_empty() {
        Ok(numeral)
    } else {
        Err(slash())
    }
}

/// The intervals of a chord above its root.
struct Shape {
    third: i32,
    fifth: i32,
    seventh: Option<i32>,
    /// Sixths, ninths and above.
    added: Vec<i32>,
}

impl Shape {
    fn intervals(&self) -> Vec<i32> {
        let mut all = vec![0, self.third, self.fifth];
        all.extend(self.seventh);
        all.extend(self.added.iter().copied());
        all.sort_unstable();
        all
    }
}

/// The unread part of a chord suffix.
struct Cursor<'a>(&'a str);

impl Cursor<'_> {
    /// Consumes `token` when the unread part starts with it.
    fn take(&mut self, token: &str) -> bool {
        match self.0.strip_prefix(token) {
            Some(after) => {
                self.0 = after;
                true
            }
            None => false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Quality {
    Major,
    Minor,
    Diminished,
    HalfDiminished,
    Augmented,
}

fn read_suffix(suffix: &str, upper: bool) -> Result<Shape, HarmonyError> {
    let bad = || HarmonyError::Suffix(suffix.to_string());
    let mut rest = Cursor(suffix);

    let mut quality = if upper {
        Quality::Major
    } else {
        Quality::Minor
    };
    if rest.take("dim") || rest.take("°") {
        quality = Quality::Diminished;
    } else if rest.take("ø") {
        quality = Quality::HalfDiminished;
    } else if rest.take("aug") || rest.take("+") {
        quality = Quality::Augmented;
    } else if !rest.0.starts_with("maj") && rest.take("m") {
        quality = Quality::Minor;
    }

    let (third, fifth) = match quality {
        Quality::Major => (400, 700),
        Quality::Minor => (300, 700),
        Quality::Diminished | Quality::HalfDiminished => (300, 600),
        Quality::Augmented => (400, 800),
    };
    let mut shape = Shape {
        third,
        fifth,
        seventh: None,
        added: Vec::new(),
    };
    if quality == Quality::HalfDiminished {
        shape.seventh = Some(1000);
    }

    let minor_seventh = if quality == Quality::Diminished {
        900
    } else {
        1000
    };
    let simple = matches!(quality, Quality::Major | Quality::Minor);
    if rest.take("maj7") {
        shape.seventh = Some(1100);
    } else if rest.take("maj9") {
        shape.seventh = Some(1100);
        shape.added.push(1400);
    } else if rest.take("add9") {
        shape.added.push(1400);
    } else if rest.take("13") {
        shape.seventh = Some(minor_seventh);
        shape.added.extend([1400, 2100]);
    } else if rest.take("11") {
        shape.seventh = Some(minor_seventh);
        shape.added.extend([1400, 1700]);
    } else if rest.take("9") {
        shape.seventh = Some(minor_seventh);
        shape.added.push(1400);
    } else if rest.take("7") {
        shape.seventh = Some(minor_seventh);
    } else if rest.take("6") {
        shape.added.push(900);
    }
    // Diminished chords stop at the seventh; anything richer is not supported.
    if !simple && quality != Quality::Augmented && !shape.added.is_empty() {
        return Err(bad());
    }

    if rest.take("sus2") {
        shape.third = 200;
    } else if rest.take("sus4") {
        shape.third = 500;
    } else if rest.0.is_empty() {
        return Ok(shape);
    }
    if rest.0.is_empty() && simple {
        Ok(shape)
    } else {
        Err(bad())
    }
}
