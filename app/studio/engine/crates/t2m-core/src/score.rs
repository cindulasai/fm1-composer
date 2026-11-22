//! The composed piece: notes on tracks, laid over the sections of the form.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Tick;
use crate::brief::Role;
use crate::pitch::{Pitch, Tonic};
use crate::time::TimeFrame;

/// A composed piece.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Score {
    pub meta: ScoreMeta,
    pub tracks: Vec<Track>,
    /// The sections of the form, in order, each ending where the next begins or earlier.
    pub sections: Vec<SectionSpan>,
}

/// What a score needs besides its notes to be played or exported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoreMeta {
    pub tempo_bpm: u16,
    pub frame: TimeFrame,
    pub tonic: Tonic,
    pub tuning_id: String,
    /// The knowledge-base fingerprint the score was composed with.
    pub data_version: String,
    pub seed: u64,
}

/// One part.
///
/// On a pitched track a note's pitch is cents above the tonic. On a drum or
/// percussion track it names a kit piece: its MIDI note number times 100.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub role: Role,
    pub instrument: String,
    /// In order of start time.
    pub notes: Vec<Note>,
}

/// One note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub start: Tick,
    pub dur: Tick,
    pub pitch: Pitch,
    /// MIDI velocity, 1 to 127.
    pub vel: u8,
    /// Pitch movement within the note, for slides and ornaments.
    #[serde(default)]
    pub bend: Option<BendCurve>,
}

impl Note {
    /// The tick at which the note stops sounding.
    pub fn end(&self) -> Tick {
        self.start.saturating_add(self.dur)
    }
}

/// Pitch offsets over the life of a note: (ticks after the note starts, cents).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BendCurve(pub Vec<(Tick, i32)>);

/// Where one section of the form lies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionSpan {
    pub id: String,
    pub start: Tick,
    pub end: Tick,
}

/// One thing wrong with a score. `note` is the note's position in its track.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScoreIssue {
    #[error("track {track}, note {note}: starts before the note in front of it")]
    Unsorted { track: String, note: usize },
    #[error("track {track}, note {note}: starts before the piece does")]
    NegativeStart { track: String, note: usize },
    #[error("track {track}, note {note}: has no length")]
    Duration { track: String, note: usize },
    #[error("track {track}, note {note}: velocity is outside 1 to 127")]
    Velocity { track: String, note: usize },
    #[error("track {track}, note {note}: sounds over an earlier note of the same pitch")]
    Overlap { track: String, note: usize },
    #[error("track {track}, note {note}: runs past the end of the form")]
    OutsideForm { track: String, note: usize },
    #[error("section {section}: is empty, or begins before the one in front of it ends")]
    Sections { section: usize },
}

impl Score {
    /// The tick at which the form ends; 0 when there are no sections.
    pub fn end(&self) -> Tick {
        self.sections.last().map_or(0, |section| section.end)
    }

    /// Checks that the score is playable as written: sections in order, notes
    /// sorted, audible, inside the form, with no note retriggered while it sounds.
    pub fn check(&self) -> Result<(), Vec<ScoreIssue>> {
        let mut issues = Vec::new();
        let mut previous_end: Tick = 0;
        for (index, section) in self.sections.iter().enumerate() {
            let empty = section.end <= section.start;
            if empty || section.start < previous_end {
                issues.push(ScoreIssue::Sections { section: index });
            }
            previous_end = previous_end.max(section.end);
        }
        let form_end = self.end();
        for track in &self.tracks {
            check_track(track, form_end, &mut issues);
        }
        if issues.is_empty() {
            Ok(())
        } else {
            Err(issues)
        }
    }
}

fn check_track(track: &Track, form_end: Tick, issues: &mut Vec<ScoreIssue>) {
    let mut previous_start = Tick::MIN;
    // For each pitch, when its most recent note stops sounding.
    let mut sounding: BTreeMap<i32, Tick> = BTreeMap::new();
    for (index, note) in track.notes.iter().enumerate() {
        let at = |issue: fn(String, usize) -> ScoreIssue| issue(track.id.clone(), index);
        if note.start < previous_start {
            issues.push(at(|track, note| ScoreIssue::Unsorted { track, note }));
        }
        previous_start = note.start;
        if note.start < 0 {
            issues.push(at(|track, note| ScoreIssue::NegativeStart { track, note }));
        }
        if note.dur <= 0 {
            issues.push(at(|track, note| ScoreIssue::Duration { track, note }));
        }
        if note.vel == 0 || note.vel > 127 {
            issues.push(at(|track, note| ScoreIssue::Velocity { track, note }));
        }
        if note.end() > form_end {
            issues.push(at(|track, note| ScoreIssue::OutsideForm { track, note }));
        }
        let held_until = sounding.get(&note.pitch.cents).copied();
        if held_until.is_some_and(|until| note.start < until) {
            issues.push(at(|track, note| ScoreIssue::Overlap { track, note }));
        }
        let until = held_until.map_or(note.end(), |until| until.max(note.end()));
        sounding.insert(note.pitch.cents, until);
    }
}
