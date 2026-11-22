//! Grooves: a style's drum vocabulary and feel, as data.
//!
//! A groove is a set of one-bar patterns on a step grid, a pool of fills, and
//! the timing and loudness shaping that make the grid feel played: swing that
//! depends on tempo, a per-step push or drag, and a per-step accent.

use serde::{Deserialize, Serialize};

/// A rhythmic feel and its drum patterns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Groove {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// Cycles this groove can be played in; empty for bar-based grooves.
    #[serde(default)]
    pub cycles: Vec<String>,
    /// The meter the patterns are written in, as (numerator, denominator).
    #[serde(default = "common_time")]
    pub meter: (u8, u8),
    /// Steps in one bar of a pattern: 16 for sixteenths in 4/4, 8 for eighths.
    #[serde(default = "sixteen")]
    pub steps: u8,
    #[serde(default)]
    pub patterns: Vec<DrumPattern>,
    #[serde(default)]
    pub fills: Vec<Fill>,
    /// Swing against tempo; empty for a straight feel.
    #[serde(default)]
    pub swing: Vec<SwingPoint>,
    /// How many steps make one swung pair: 2 swings every other step, 4 every other pair.
    #[serde(default = "two")]
    pub swing_steps: u8,
    /// Ticks to move each step by, one entry per step; empty for none.
    #[serde(default)]
    pub timing: Vec<i16>,
    /// Loudness to add at each step, one entry per step; empty for none.
    #[serde(default)]
    pub accent: Vec<i8>,
    /// The most a pitched part's note may be moved, in ticks, when humanized.
    #[serde(default = "eight")]
    pub human_ticks: u8,
    pub source: String,
    pub verified: bool,
}

fn common_time() -> (u8, u8) {
    (4, 4)
}

fn sixteen() -> u8 {
    16
}

fn two() -> u8 {
    2
}

fn eight() -> u8 {
    8
}

/// How intense a section is, in thirds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Low,
    Mid,
    High,
}

impl Band {
    /// The band an energy from 0 to 1 falls in.
    pub fn of(energy: f64) -> Band {
        if energy < 1.0 / 3.0 {
            Band::Low
        } else if energy < 2.0 / 3.0 {
            Band::Mid
        } else {
            Band::High
        }
    }
}

/// One bar of drums for sections of one energy band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrumPattern {
    pub energy: Band,
    pub lanes: Vec<Lane>,
}

/// How big a fill is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FillSize {
    Light,
    Heavy,
}

/// A fill for the last bar of a section. It takes over from its first hit to the bar's end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fill {
    pub size: FillSize,
    pub lanes: Vec<Lane>,
}

/// One kit piece through one bar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lane {
    /// The kit piece's name, for example `kick` or `closed_hat`.
    pub piece: String,
    /// One character per step: `X` accented, `x` normal, `g` ghost, `.` silent.
    pub hits: String,
    /// Ticks to play this piece late (or early, if negative).
    #[serde(default)]
    pub late: i16,
}

/// The swing ratio (long note to short note, in thousandths) at one tempo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwingPoint {
    pub bpm: u16,
    pub ratio: u16,
}

/// How hard a hit is played.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Accent,
    Normal,
    Ghost,
}

impl Lane {
    /// The hit at each step; `None` where the lane is silent.
    pub fn steps(&self) -> impl Iterator<Item = Option<Hit>> + '_ {
        self.hits.chars().map(|symbol| match symbol {
            'X' => Some(Hit::Accent),
            'x' => Some(Hit::Normal),
            'g' => Some(Hit::Ghost),
            _ => None,
        })
    }
}

impl Fill {
    /// The first step at which any lane of the fill plays.
    pub fn first_step(&self) -> Option<usize> {
        self.lanes
            .iter()
            .filter_map(|lane| lane.hits.chars().position(|symbol| symbol != '.'))
            .min()
    }
}

impl Groove {
    /// The swing ratio at a tempo, in thousandths: 1000 is straight, 2000 is a triplet feel.
    ///
    /// Between two points the ratio changes evenly; beyond the ends it stays at the end value.
    pub fn swing_ratio(&self, bpm: u16) -> u32 {
        let (Some(first), Some(last)) = (self.swing.first(), self.swing.last()) else {
            return 1000;
        };
        if bpm <= first.bpm {
            return u32::from(first.ratio);
        }
        if bpm >= last.bpm {
            return u32::from(last.ratio);
        }
        for pair in self.swing.windows(2) {
            let (slow, fast) = (pair[0], pair[1]);
            if bpm >= slow.bpm && bpm <= fast.bpm && fast.bpm > slow.bpm {
                let along = i64::from(bpm - slow.bpm);
                let width = i64::from(fast.bpm - slow.bpm);
                let change = i64::from(fast.ratio) - i64::from(slow.ratio);
                let ratio = i64::from(slow.ratio) + change * along / width;
                return u32::try_from(ratio).unwrap_or(1000);
            }
        }
        u32::from(last.ratio)
    }

    /// Checks the groove's own consistency. Returns what is wrong, if anything.
    pub fn check(&self) -> Result<(), String> {
        let steps = usize::from(self.steps);
        if steps == 0 {
            return Err("a groove needs at least one step per bar".to_string());
        }
        let lanes = self
            .patterns
            .iter()
            .flat_map(|pattern| &pattern.lanes)
            .chain(self.fills.iter().flat_map(|fill| &fill.lanes));
        for lane in lanes {
            if lane.hits.chars().count() != steps {
                return Err(format!(
                    "lane {} has {} steps, not {steps}",
                    lane.piece,
                    lane.hits.chars().count()
                ));
            }
            if let Some(symbol) = lane.hits.chars().find(|symbol| !"Xxg.".contains(*symbol)) {
                return Err(format!(
                    "lane {} uses the unknown symbol {symbol:?}",
                    lane.piece
                ));
            }
        }
        if self.fills.iter().any(|fill| fill.first_step().is_none()) {
            return Err("a fill has no hits".to_string());
        }
        for (name, length) in [("timing", self.timing.len()), ("accent", self.accent.len())] {
            if length != 0 && length != steps {
                return Err(format!("{name} has {length} entries, not {steps}"));
            }
        }
        let rising = self.swing.windows(2).all(|pair| pair[0].bpm < pair[1].bpm);
        if !rising || self.swing.iter().any(|point| point.ratio < 1000) {
            return Err(
                "swing points must rise in tempo, with ratios of at least 1000".to_string(),
            );
        }
        if self.swing_steps == 0 || !self.swing_steps.is_multiple_of(2) {
            return Err("swing_steps must be an even number of steps".to_string());
        }
        Ok(())
    }
}
