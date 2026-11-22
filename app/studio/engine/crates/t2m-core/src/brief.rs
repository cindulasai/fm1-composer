//! The Composition Brief: the only input to the composer.
//!
//! A brief is a plan for a piece, written by a language model or by hand. Its
//! JSON form is the contract with the model, so field names here are part of
//! the public interface and the doc comments become the schema descriptions
//! the model reads. A parsed brief is not yet trustworthy: pass it through
//! `repair` before composing.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The largest brief text accepted, in bytes.
pub const MAX_BRIEF_BYTES: usize = 1_000_000;

/// The brief format version this build reads and writes.
pub const BRIEF_VERSION: u32 = 1;

/// A plan for one piece of music.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Brief {
    /// Format version; always 1.
    pub version: u32,
    pub identity: Identity,
    pub time: TimeSpec,
    pub tonality: Tonality,
    /// The sections of the piece in playing order (1 to 24).
    pub form: Vec<Section>,
    /// The parts of the arrangement (1 to 16).
    pub tracks: Vec<TrackSpec>,
    /// An optional short melodic idea for the piece to develop.
    #[serde(default)]
    pub motif: Option<Motif>,
    /// What was guessed rather than stated, and why.
    #[serde(default)]
    pub assumptions: Vec<Assumption>,
}

impl Brief {
    /// Reads a brief from JSON text.
    ///
    /// Unknown fields are ignored. Text over [`MAX_BRIEF_BYTES`] is refused
    /// without being parsed.
    pub fn from_json(text: &str) -> Result<Brief, BriefParseError> {
        if text.len() > MAX_BRIEF_BYTES {
            return Err(BriefParseError::TooLarge { bytes: text.len() });
        }
        serde_json::from_str(text).map_err(|error| BriefParseError::Syntax {
            line: error.line(),
            column: error.column(),
            message: error.to_string(),
        })
    }

    /// Writes the brief as indented JSON.
    pub fn to_json(&self) -> String {
        // Serializing these types cannot fail: every map key is a string.
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Why brief text could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BriefParseError {
    #[error("the brief is {bytes} bytes; the limit is {MAX_BRIEF_BYTES}")]
    TooLarge { bytes: usize },
    #[error("line {line}, column {column}: {message}")]
    Syntax {
        line: usize,
        column: usize,
        message: String,
    },
}

/// What the piece is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Identity {
    #[serde(default)]
    pub title: String,
    /// One line describing the piece.
    #[serde(default)]
    pub summary: String,
    /// A style id from the knowledge base, for example `hiphop.lofi`.
    pub style: String,
    /// An optional second style id to blend in.
    #[serde(default)]
    pub fusion: Option<String>,
    /// Artists or works the user named, and what was taken from each.
    #[serde(default)]
    pub references: Vec<Reference>,
}

/// A reference the user made and how it was understood.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Reference {
    pub said: String,
    #[serde(default)]
    pub interpreted_as: Vec<String>,
}

/// Tempo, meter, feel and length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TimeSpec {
    /// Beats per minute, 30 to 300.
    pub tempo_bpm: u16,
    pub meter: MeterSpec,
    /// A groove id from the knowledge base, for example `groove.lofi_swing`.
    pub feel: String,
    pub length: Length,
}

/// A bar-based meter, or a named cycle from a tradition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MeterSpec {
    Meter {
        numerator: u8,
        /// 1, 2, 4, 8, 16 or 32.
        denominator: u8,
        /// For additive meters, how the bar divides, for example `[2, 2, 3]` for 7/8.
        #[serde(default)]
        grouping: Option<Vec<u8>>,
    },
    Cycle {
        /// A cycle id from the knowledge base, for example `tala.rupak`.
        id: String,
    },
}

/// How long the piece is. Total length is 1 to 512 bars or cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Length {
    Bars(u16),
    Cycles(u16),
    Seconds(u16),
}

/// Tonic, scale system, tuning and how harmony behaves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Tonality {
    pub tonic: TonicSpec,
    /// A tonal system id, for example `mode.dorian`, `raga.yaman`, `maqam.bayati`.
    pub system: String,
    /// A tuning id valid for the system, for example `12tet`.
    pub tuning: String,
    pub harmonic_language: HarmonicLanguage,
}

/// The tonic as a note name (`"D"`, `"Eb"`, `"F#"`) or an exact frequency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum TonicSpec {
    Name(String),
    Hz { hz: f64 },
}

/// How chords relate to the melody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HarmonicLanguage {
    /// Chord progressions with tension and resolution.
    Functional,
    /// Chords drawn from one mode, without strong resolution.
    Modal,
    /// A sustained drone under the melody; no chord changes.
    Drone,
    /// No harmony at all.
    None,
}

/// One section of the form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Section {
    /// Unique within the brief.
    pub id: String,
    /// What the section is, for example `intro`, `verse`, `chorus`, `bridge`, `outro`.
    #[serde(default)]
    pub kind: String,
    /// Length in bars, or in cycles when the meter is a cycle.
    pub bars: u16,
    /// 0 (calm) to 1 (full intensity).
    pub energy: f64,
    /// 0 (sparse) to 1 (dense).
    pub density: f64,
    pub harmony: SectionHarmony,
    /// Ids of the tracks that play in this section.
    pub tracks: Vec<String>,
    /// What happens at the end of the section, leading into the next.
    #[serde(default)]
    pub transition: Transition,
}

/// The harmony of a section.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SectionHarmony {
    /// Roman numerals that repeat through the section, for example `["i7", "IV7"]`.
    Progression {
        roman: Vec<String>,
        /// How many bars each chord lasts, at least 1.
        bars_per_chord: u8,
    },
    /// Sustained scale degrees, counted from 1 for the tonic, for example `[1, 5]`.
    Drone {
        degrees: Vec<u8>,
    },
    /// A named progression from the knowledge base, for example `prog.andalusian`.
    Named {
        id: String,
    },
    None,
}

/// How a section hands over to the next one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    None,
    FillLight,
    FillHeavy,
    /// A crash cymbal on the first beat of the next section.
    Crash,
    /// A rising effect through the last bar.
    Riser,
    /// A hit on the first beat of the next section.
    Impact,
    /// Everything stops for the last beat.
    Break,
}

/// One part of the arrangement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrackSpec {
    /// Unique within the brief.
    pub id: String,
    pub role: Role,
    /// An instrument id from the knowledge base, for example `keys.rhodes`.
    pub instrument: String,
    #[serde(default)]
    pub register: Register,
    /// A pattern family for the role, for example `melody.lyrical`. Empty means the style's default.
    #[serde(default)]
    pub pattern: String,
    /// 0 (few notes) to 1 (many notes).
    pub busy: f64,
    #[serde(default)]
    pub articulation: Articulation,
}

/// What a part does in the arrangement.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Lead,
    Counter,
    Chords,
    Pad,
    Bass,
    Drums,
    Percussion,
    Arp,
    Drone,
    Fx,
}

/// Where a part sits in pitch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Register {
    Low,
    MidLow,
    #[default]
    Mid,
    MidHigh,
    High,
}

/// How long notes are held relative to their written length.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Articulation {
    Legato,
    #[default]
    Normal,
    Staccato,
}

/// A short melodic idea: at most a bar or two, never a full melody.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Motif {
    /// Scale steps above the tonic, 0 for the tonic itself; may be negative.
    pub degrees: Vec<i32>,
    /// The length of each note in beats; same count as `degrees`.
    pub durations: Vec<f64>,
    /// How the idea is developed, for example `sequence_then_answer`.
    #[serde(default)]
    pub development: String,
}

/// One thing that was guessed rather than stated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Assumption {
    /// JSON pointer to the guessed field, for example `/time/tempo_bpm`.
    pub field: String,
    pub because: String,
    /// 0 to 1.
    #[serde(default)]
    pub confidence: f64,
}

/// The JSON Schema of a brief, for use as a language model's output schema.
pub fn brief_schema() -> serde_json::Value {
    schemars::schema_for!(Brief).to_value()
}
