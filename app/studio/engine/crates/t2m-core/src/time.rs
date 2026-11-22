//! Musical time: bar-based meters and beat cycles, measured in ticks.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{TICKS_PER_QUARTER, Tick};

const WHOLE_NOTE: Tick = TICKS_PER_QUARTER * 4;

/// Why a meter or cycle is not usable.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TimeError {
    #[error("the numerator must be at least 1")]
    Numerator,
    #[error("the denominator {0} is not one of 1, 2, 4, 8, 16, 32")]
    Denominator(u8),
    #[error("the grouping adds up to {got}, not {expected}")]
    GroupingSum { expected: u8, got: u32 },
    #[error("a grouping needs at least one group, each of at least 1")]
    GroupingEmpty,
    #[error("a cycle needs at least 1 beat")]
    Beats,
    #[error("{got} strokes were given for a cycle of {expected} beats")]
    StrokeLength { expected: u8, got: usize },
    #[error("accent on beat {beat} lies outside a cycle of {beats} beats")]
    AccentOutside { beat: u8, beats: u8 },
}

/// A bar-based meter such as 4/4, or 7/8 grouped 2+2+3.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meter {
    pub numerator: u8,
    pub denominator: u8,
    /// How the numerator's units group into felt beats. Sums to the numerator.
    pub grouping: Vec<u8>,
}

impl Meter {
    /// Builds a meter. Without a grouping the conventional one is used:
    /// threes for compound meters (6/8, 9/8, 12/8), twos ending in a three for
    /// odd meters over 8 or shorter (5/8, 7/8), otherwise one group per unit.
    pub fn new(
        numerator: u8,
        denominator: u8,
        grouping: Option<Vec<u8>>,
    ) -> Result<Meter, TimeError> {
        if numerator == 0 {
            return Err(TimeError::Numerator);
        }
        if unit_ticks(denominator).is_none() {
            return Err(TimeError::Denominator(denominator));
        }
        let grouping = match grouping {
            Some(given) => {
                check_grouping(&given, numerator)?;
                given
            }
            None => default_grouping(numerator, denominator),
        };
        Ok(Meter {
            numerator,
            denominator,
            grouping,
        })
    }

    /// The conventional grouping for this numerator and denominator.
    pub fn default_grouping(numerator: u8, denominator: u8) -> Vec<u8> {
        default_grouping(numerator, denominator)
    }

    /// Length of one bar.
    pub fn bar_ticks(&self) -> Tick {
        Tick::from(self.numerator.max(1)) * self.unit()
    }

    /// Where each group starts, from the bar line.
    pub fn accents(&self) -> Vec<Tick> {
        group_starts(&self.grouping, self.unit())
    }

    fn unit(&self) -> Tick {
        unit_ticks(self.denominator).unwrap_or(TICKS_PER_QUARTER)
    }

    fn strong_positions(&self) -> Vec<Tick> {
        let ungrouped = self.grouping.iter().all(|group| *group == 1);
        if !ungrouped {
            return self.accents();
        }
        let mut strong = vec![0];
        let even = self.numerator.is_multiple_of(2);
        if even && self.numerator >= 4 {
            strong.push(self.bar_ticks() / 2);
        }
        strong
    }
}

/// An accent inside a cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accent {
    /// Beat index from 0.
    pub beat: u8,
    /// Relative weight; larger is stronger.
    pub weight: u8,
}

/// One named stroke of a cycle's basic pattern, one per beat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stroke {
    pub name: String,
}

/// A beat cycle from a tradition (a tala, an iqa', a timeline).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub id: String,
    pub beats: u8,
    /// The note value of one beat: 4 for a quarter, 8 for an eighth.
    pub beat_unit: u8,
    pub grouping: Vec<u8>,
    #[serde(default)]
    pub accents: Vec<Accent>,
    #[serde(default)]
    pub strokes: Vec<Stroke>,
    pub source: String,
    pub verified: bool,
}

impl Cycle {
    /// Checks that the cycle is internally consistent.
    pub fn check(&self) -> Result<(), TimeError> {
        if self.beats == 0 {
            return Err(TimeError::Beats);
        }
        if unit_ticks(self.beat_unit).is_none() {
            return Err(TimeError::Denominator(self.beat_unit));
        }
        check_grouping(&self.grouping, self.beats)?;
        if !self.strokes.is_empty() && self.strokes.len() != usize::from(self.beats) {
            return Err(TimeError::StrokeLength {
                expected: self.beats,
                got: self.strokes.len(),
            });
        }
        match self.accents.iter().find(|accent| accent.beat >= self.beats) {
            Some(accent) => Err(TimeError::AccentOutside {
                beat: accent.beat,
                beats: self.beats,
            }),
            None => Ok(()),
        }
    }

    fn unit(&self) -> Tick {
        unit_ticks(self.beat_unit).unwrap_or(TICKS_PER_QUARTER)
    }
}

/// The repeating span music is laid out on: a bar of a meter, or one cycle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeFrame {
    Meter(Meter),
    Cycle(Cycle),
}

impl TimeFrame {
    /// Length of one bar or one cycle. Always positive.
    pub fn span_ticks(&self) -> Tick {
        match self {
            TimeFrame::Meter(meter) => meter.bar_ticks(),
            TimeFrame::Cycle(cycle) => Tick::from(cycle.beats.max(1)) * cycle.unit(),
        }
    }

    /// Metrically strong positions within one span, from its start.
    ///
    /// Ungrouped meters: the downbeat, plus the half-bar when the bar has an
    /// even number of at least four units. Grouped meters and cycles: the start
    /// of each group.
    pub fn strong_positions(&self) -> Vec<Tick> {
        match self {
            TimeFrame::Meter(meter) => meter.strong_positions(),
            TimeFrame::Cycle(cycle) => group_starts(&cycle.grouping, cycle.unit()),
        }
    }
}

fn unit_ticks(denominator: u8) -> Option<Tick> {
    matches!(denominator, 1 | 2 | 4 | 8 | 16 | 32).then(|| WHOLE_NOTE / Tick::from(denominator))
}

fn check_grouping(grouping: &[u8], expected: u8) -> Result<(), TimeError> {
    if grouping.is_empty() || grouping.contains(&0) {
        return Err(TimeError::GroupingEmpty);
    }
    let got: u32 = grouping.iter().map(|group| u32::from(*group)).sum();
    if got != u32::from(expected) {
        return Err(TimeError::GroupingSum { expected, got });
    }
    Ok(())
}

fn default_grouping(numerator: u8, denominator: u8) -> Vec<u8> {
    let short_units = denominator >= 8;
    let compound = short_units && numerator > 3 && numerator.is_multiple_of(3);
    if compound {
        return vec![3; usize::from(numerator / 3)];
    }
    let odd = !numerator.is_multiple_of(2);
    if short_units && numerator > 3 && odd {
        let mut groups = vec![2; usize::from((numerator - 3) / 2)];
        groups.push(3);
        return groups;
    }
    vec![1; usize::from(numerator)]
}

fn group_starts(grouping: &[u8], unit: Tick) -> Vec<Tick> {
    let mut starts = Vec::with_capacity(grouping.len());
    let mut at: Tick = 0;
    for group in grouping {
        starts.push(at);
        at += Tick::from(*group) * unit;
    }
    starts
}
