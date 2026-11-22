//! The shared vocabulary of text2midi v2.
//!
//! Everything here is plain data and pure functions: no I/O beyond reading the
//! embedded knowledge base, no network, no audio, no randomness.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

pub mod brief;
pub mod brief_ops;
pub mod groove;
pub mod harmony;
pub mod knowledge;
pub mod patch;
pub mod pitch;
pub mod score;
pub mod time;
pub mod tonal;

pub use brief::{Brief, BriefParseError, brief_schema};
pub use brief_ops::{BriefIssue, IssueKind, repair, validate};
pub use groove::Groove;
pub use harmony::{Chord, HarmonyError, parse_roman};
pub use knowledge::{
    FormTemplate, GalleryEntry, Instrument, Kind, KitPiece, Knowledge, KnowledgeError,
    PaletteEntry, Progression, Style, TempoRange,
};
pub use patch::{BriefPatch, ChangeSet, PatchError, PatchOp, apply, changed_inputs};
pub use pitch::{Pitch, PitchClass, Tonic, parse_pitch_class};
pub use score::{BendCurve, Note, Score, ScoreIssue, ScoreMeta, SectionSpan, Track};
pub use time::{Accent, Cycle, Meter, Stroke, TimeError, TimeFrame};
pub use tonal::{MoveGraph, OrnamentRule, Phrase, TonalError, TonalFamily, TonalSystem, Tuning};

/// Tick resolution used everywhere: 960 ticks per quarter note.
pub const TICKS_PER_QUARTER: i64 = 960;

/// A position or length in ticks. Signed so that offsets can be negative.
pub type Tick = i64;
