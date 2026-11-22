//! What the window draws: plain data, serialisable, no musical logic needed to read it.

use serde::Serialize;
use t2m_core::Score;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionView {
    pub id: String,
    pub start: i64,
    pub end: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackView {
    pub id: String,
    pub role: String,
    pub instrument: String,
    pub locked: bool,
    /// (start tick, length in ticks, pitch as a MIDI note number with the fraction for cents).
    pub notes: Vec<(i64, i64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct View {
    pub gallery_id: String,
    pub title: String,
    pub style: String,
    pub tempo_bpm: u16,
    pub seed: u64,
    /// Ticks per quarter note, so the window can place beats.
    pub ticks_per_quarter: i64,
    pub end: i64,
    pub sections: Vec<SectionView>,
    pub tracks: Vec<TrackView>,
    /// Ids of the quality gates the piece fails, empty when it passes all.
    pub failed_gates: Vec<String>,
    pub can_undo: bool,
    pub can_redo: bool,
}

pub(crate) fn tracks_of(
    score: &Score,
    locked: &std::collections::BTreeSet<String>,
) -> Vec<TrackView> {
    let (tonic, offset) = score.meta.tonic.midi_and_offset(0);
    let base = f64::from(tonic) + f64::from(offset) / 100.0;
    score
        .tracks
        .iter()
        .map(|track| {
            let kit = matches!(
                track.role,
                t2m_core::brief::Role::Drums | t2m_core::brief::Role::Percussion
            );
            TrackView {
                id: track.id.clone(),
                role: format!("{:?}", track.role).to_lowercase(),
                instrument: track.instrument.clone(),
                locked: locked.contains(&track.id),
                notes: track
                    .notes
                    .iter()
                    .map(|n| {
                        let pitch = f64::from(n.pitch.cents) / 100.0;
                        (n.start, n.dur, if kit { pitch } else { base + pitch })
                    })
                    .collect(),
            }
        })
        .collect()
}
