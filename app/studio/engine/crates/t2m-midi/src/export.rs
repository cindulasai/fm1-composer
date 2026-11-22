//! Export: a score, a way to carry microtones, and which parts to include, as files.

use crate::events::{MidiWarning, TuningProfile, to_events_with};
use crate::scala::{write_ascl, write_kbm, write_scl};
use crate::smf::write_smf;
use t2m_core::{Knowledge, Score, SectionSpan, Tick, Tuning};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MidiError {
    #[error("cannot read MIDI: {reason}")]
    Parse { reason: String },
    #[error("the score's tuning {id:?} is not in the knowledge base")]
    Tuning { id: String },
    #[error("no such track: {id}")]
    NoTrack { id: String },
    #[error("no such section: {id}")]
    NoSection { id: String },
    #[error("{message}")]
    Io { message: String },
}

/// Which parts go into the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TrackSelection {
    #[default]
    All,
    /// Only these track ids, in score order.
    Only(Vec<String>),
}

/// Which stretch of the form goes into the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SectionSelection {
    #[default]
    All,
    /// One section; its start becomes tick 0.
    Only(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportOptions {
    pub tracks: TrackSelection,
    pub sections: SectionSelection,
    pub profile: TuningProfile,
    /// One file per selected track instead of one file.
    pub one_file_per_track: bool,
    /// File name stem.
    pub name: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            tracks: TrackSelection::All,
            sections: SectionSelection::All,
            profile: TuningProfile::Mpe,
            one_file_per_track: false,
            name: "piece".into(),
        }
    }
}

/// The files to write and what they could not say exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExportBundle {
    /// (file name, bytes).
    pub files: Vec<(String, Vec<u8>)>,
    pub warnings: Vec<MidiWarning>,
}

/// Exports `score`, finding its tuning in `kb`.
pub fn export(
    score: &Score,
    kb: &Knowledge,
    options: &ExportOptions,
) -> Result<ExportBundle, MidiError> {
    let tuning = kb
        .tuning(&score.meta.tuning_id)
        .ok_or_else(|| MidiError::Tuning {
            id: score.meta.tuning_id.clone(),
        })?;
    export_with_tuning(score, kb, tuning, options)
}

/// Exports `score` using `tuning` for the tuning-file profile.
pub fn export_with_tuning(
    score: &Score,
    kb: &Knowledge,
    tuning: &Tuning,
    options: &ExportOptions,
) -> Result<ExportBundle, MidiError> {
    let selected = select(score, options)?;
    let parts: Vec<(String, Score)> = if options.one_file_per_track {
        selected
            .tracks
            .iter()
            .map(|track| {
                let mut one = selected.clone();
                one.tracks = vec![track.clone()];
                (format!("{}-{}", options.name, track.id), one)
            })
            .collect()
    } else {
        vec![(options.name.clone(), selected)]
    };
    let mut bundle = ExportBundle::default();
    for (stem, part) in &parts {
        let stream = to_events_with(part, kb, options.profile, Some(tuning));
        bundle
            .files
            .push((format!("{stem}.mid"), write_smf(part, &stream.events)));
        for warning in stream.warnings {
            if !bundle.warnings.contains(&warning) {
                bundle.warnings.push(warning);
            }
        }
    }
    let twelve_tone = tuning.period == 1200
        && tuning.cents.len() == 12
        && tuning
            .cents
            .iter()
            .enumerate()
            .all(|(i, &c)| c == i as i32 * 100);
    if options.profile == TuningProfile::TuningFile && !twelve_tone {
        let (midi, offset) = score.meta.tonic.midi_and_offset(0);
        let hz = 440.0 * 2f64.powf((f64::from(midi) + f64::from(offset) / 100.0 - 69.0) / 12.0);
        let name = &options.name;
        let text = |s: String| s.into_bytes();
        bundle
            .files
            .push((format!("{name}.scl"), text(write_scl(tuning, name))));
        bundle.files.push((
            format!("{name}.kbm"),
            text(write_kbm(midi, hz, tuning.cents.len())),
        ));
        bundle.files.push((
            format!("{name}.ascl"),
            text(write_ascl(tuning, name, midi, hz)),
        ));
    }
    Ok(bundle)
}

/// The score restricted to the chosen tracks and section, times rebased to the section start.
fn select(score: &Score, options: &ExportOptions) -> Result<Score, MidiError> {
    let mut out = score.clone();
    if let TrackSelection::Only(ids) = &options.tracks {
        if let Some(id) = ids
            .iter()
            .find(|id| !score.tracks.iter().any(|t| &t.id == *id))
        {
            return Err(MidiError::NoTrack { id: id.clone() });
        }
        out.tracks.retain(|t| ids.contains(&t.id));
    }
    if let SectionSelection::Only(id) = &options.sections {
        let span: SectionSpan = score
            .sections
            .iter()
            .find(|s| &s.id == id)
            .cloned()
            .ok_or_else(|| MidiError::NoSection { id: id.clone() })?;
        let shift: Tick = span.start;
        for track in &mut out.tracks {
            track
                .notes
                .retain(|n| n.start >= span.start && n.start < span.end);
            for note in &mut track.notes {
                note.start -= shift;
                note.dur = note.dur.min(span.end - span.start - note.start).max(1);
            }
        }
        out.sections = vec![SectionSpan {
            id: span.id,
            start: 0,
            end: span.end - span.start,
        }];
    }
    Ok(out)
}

#[cfg(test)]
#[path = "export_tests.rs"]
mod tests;
