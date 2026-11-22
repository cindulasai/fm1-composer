//! Files: a score as a Standard MIDI File, with three ways to carry microtones.
//!
//! `to_events` turns a score into timed MIDI events (shared with a later audio
//! engine); `export` wraps those into files, and `read_notes` reads them back.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod bundle;
mod events;
mod export;
mod scala;
mod smf;
#[cfg(test)]
mod testkit;

pub use bundle::write_bundle;
pub use events::{
    EventKind, EventStream, MidiWarning, TimedEvent, TuningProfile, to_events, to_events_with,
};
pub use export::{
    ExportBundle, ExportOptions, MidiError, SectionSelection, TrackSelection, export,
    export_with_tuning,
};
pub use scala::{write_ascl, write_kbm, write_scl};
pub use smf::{ReadNote, ReadTrack, read_notes};
