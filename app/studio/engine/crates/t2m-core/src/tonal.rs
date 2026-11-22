//! Tonal systems (modes, ragas, maqamat, gamelan scales) and their tunings.
//!
//! A [`Tuning`] is a table of pitches within one period (usually an octave).
//! A [`TonalSystem`] picks its degrees out of that table with `steps`, so one
//! twelve-tone table serves every Western mode, and a raga or maqam can sit on
//! a finer table.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Pitch;

const OCTAVE_CENTS: i32 = 1200;

/// Why a tuning or tonal system is not usable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TonalError {
    #[error("tuning {id} has no pitches")]
    TuningEmpty { id: String },
    #[error("tuning {id} must start at 0 cents")]
    TuningFirst { id: String },
    #[error("tuning {id} must rise with every pitch")]
    TuningOrder { id: String },
    #[error("tuning {id} needs a period greater than its last pitch")]
    TuningPeriod { id: String },
    #[error("system {system} needs at least one step, each higher than the one before")]
    SystemSteps { system: String },
    #[error("system {system} does not fit tuning {tuning}")]
    DegreeMismatch { system: String, tuning: String },
    #[error("system {system} names degree {degree} in {field}, which it does not have")]
    DegreeOutside {
        system: String,
        field: &'static str,
        degree: u8,
    },
}

/// A table of pitches in cents above the tonic, covering one period.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tuning {
    pub id: String,
    /// Ascending, first is 0.
    pub cents: Vec<i32>,
    /// The interval at which the table repeats; 1200 for an octave.
    pub period: i32,
    pub source: String,
    pub verified: bool,
}

impl Tuning {
    /// Checks that the table starts at 0, rises, and fits inside its period.
    pub fn check(&self) -> Result<(), TonalError> {
        let id = || self.id.clone();
        let first = *self
            .cents
            .first()
            .ok_or_else(|| TonalError::TuningEmpty { id: id() })?;
        if first != 0 {
            return Err(TonalError::TuningFirst { id: id() });
        }
        let rising = self.cents.windows(2).all(|pair| pair[0] < pair[1]);
        if !rising {
            return Err(TonalError::TuningOrder { id: id() });
        }
        let last = self.cents.last().copied().unwrap_or(0);
        if self.period <= last {
            return Err(TonalError::TuningPeriod { id: id() });
        }
        Ok(())
    }

    fn period(&self) -> i32 {
        if self.period > 0 {
            self.period
        } else {
            OCTAVE_CENTS
        }
    }
}

/// The broad kind of a tonal system; decides which rules the composer applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TonalFamily {
    Mode,
    Raga,
    Maqam,
    Gamelan,
    Pentatonic,
}

/// Which degrees may be arrived at when moving up and when moving down.
///
/// An empty list places no restriction on that direction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveGraph {
    #[serde(default)]
    pub ascending: Vec<u8>,
    #[serde(default)]
    pub descending: Vec<u8>,
}

/// A characteristic phrase: scale degrees and their relative durations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Phrase {
    pub degrees: Vec<i32>,
    pub durations: Vec<f32>,
}

/// An ornament a tradition applies on certain degrees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrnamentRule {
    pub kind: String,
    pub degrees: Vec<u8>,
}

/// A scale-like system: its degrees, how melody may move, where it rests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TonalSystem {
    pub id: String,
    pub family: TonalFamily,
    /// For each degree, its index in the tuning table. Strictly ascending.
    pub steps: Vec<u8>,
    pub default_tuning: String,
    /// Other tunings this system may be played in.
    #[serde(default)]
    pub tunings: Vec<String>,
    /// Other names people use for the system.
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub moves: MoveGraph,
    /// Degrees to aim for, strongest first.
    #[serde(default)]
    pub emphasis: Vec<u8>,
    #[serde(default)]
    pub rest_degrees: Vec<u8>,
    #[serde(default)]
    pub phrases: Vec<Phrase>,
    #[serde(default)]
    pub ornaments: Vec<OrnamentRule>,
    /// True when triads on its degrees lie within 15 cents of 12-tone equal temperament.
    pub chordable: bool,
    pub source: String,
    pub verified: bool,
}

impl TonalSystem {
    /// How many degrees the system has in one period.
    pub fn degrees(&self) -> u8 {
        u8::try_from(self.steps.len()).unwrap_or(u8::MAX)
    }

    /// Checks the system against one of its tunings.
    pub fn check(&self, tuning: &Tuning) -> Result<(), TonalError> {
        let rising = self.steps.windows(2).all(|pair| pair[0] < pair[1]);
        if self.steps.is_empty() || !rising {
            return Err(TonalError::SystemSteps {
                system: self.id.clone(),
            });
        }
        let fits = self
            .steps
            .iter()
            .all(|step| usize::from(*step) < tuning.cents.len());
        if !fits {
            return Err(TonalError::DegreeMismatch {
                system: self.id.clone(),
                tuning: tuning.id.clone(),
            });
        }
        let fields: [(&'static str, &[u8]); 4] = [
            ("moves.ascending", &self.moves.ascending),
            ("moves.descending", &self.moves.descending),
            ("emphasis", &self.emphasis),
            ("rest_degrees", &self.rest_degrees),
        ];
        for (field, listed) in fields {
            if let Some(degree) = listed.iter().find(|degree| **degree >= self.degrees()) {
                return Err(TonalError::DegreeOutside {
                    system: self.id.clone(),
                    field,
                    degree: *degree,
                });
            }
        }
        Ok(())
    }

    /// The pitch of a degree. Degree 0 is the tonic; degrees continue above
    /// and below through every octave.
    pub fn pitch(&self, tuning: &Tuning, degree: i32) -> Pitch {
        let count = self.count();
        if count == 0 {
            return Pitch { cents: 0 };
        }
        let octave = i64::from(degree.div_euclid(count));
        let index = usize::try_from(degree.rem_euclid(count)).unwrap_or(0);
        let step = self.steps.get(index).copied().unwrap_or(0);
        let within = tuning.cents.get(usize::from(step)).copied().unwrap_or(0);
        let cents = octave * i64::from(tuning.period()) + i64::from(within);
        Pitch {
            cents: i32::try_from(cents.clamp(i64::from(i32::MIN), i64::from(i32::MAX)))
                .unwrap_or(0),
        }
    }

    /// Whether a melody may move from one degree to another.
    ///
    /// Staying put is always allowed. Moving up must arrive on a degree of the
    /// ascending form, moving down on a degree of the descending form.
    pub fn legal_move(&self, from: i32, to: i32) -> bool {
        let count = self.count();
        if count == 0 || from == to {
            return true;
        }
        let allowed = if to > from {
            &self.moves.ascending
        } else {
            &self.moves.descending
        };
        let class = to.rem_euclid(count);
        allowed.is_empty() || allowed.iter().any(|degree| i32::from(*degree) == class)
    }

    /// The degree whose pitch is closest to `p`. A tie goes to the lower degree.
    pub fn nearest_degree(&self, tuning: &Tuning, p: Pitch) -> i32 {
        let count = self.count();
        if count == 0 {
            return 0;
        }
        let octave = p.cents.div_euclid(tuning.period());
        // From the degree just below this octave to the tonic of the next one.
        let lowest = octave.saturating_mul(count).saturating_sub(1);
        let mut best = lowest;
        let mut best_distance = u32::MAX;
        for offset in 0..=count + 1 {
            let degree = lowest.saturating_add(offset);
            let distance = self.pitch(tuning, degree).cents.abs_diff(p.cents);
            if distance < best_distance {
                best = degree;
                best_distance = distance;
            }
        }
        best
    }

    fn count(&self) -> i32 {
        i32::from(self.degrees())
    }
}
