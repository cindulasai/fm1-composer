//! The composer: a deterministic, seeded function from a brief to a score.
//!
//! Every part of every section is a pure function of the plan and its own
//! random stream, so recomposing leaves untouched parts exactly as they were.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]
// Parts are added one story at a time; until all are wired some items are unused.

mod critic;
mod expression;
mod harmony;
mod motif;
mod parts;
mod plan;
mod seed;
#[cfg(test)]
mod testkit;

use t2m_core::brief::{Brief, Role};
use t2m_core::{
    Knowledge, Meter, Note, Score, ScoreMeta, SectionSpan, Tick, TimeFrame, Tonic, Track, repair,
};

use crate::harmony::{ChordSpan, chord_timeline};
use crate::plan::{Plan, resolve};

pub use critic::{Gate, Metric, Scorecard, critique};
pub use seed::Rerolls;

/// Composes a piece. Total: a brief that is not valid is repaired first.
pub fn compose(brief: &Brief, kb: &Knowledge, seed: u64) -> Score {
    recompose(brief, kb, seed, &Rerolls::default())
}

/// Composes a piece with some parts rerolled.
///
/// A part reads only its own inputs and its own random stream, so a part
/// whose inputs did not change comes out identical to the last composition.
pub fn recompose(brief: &Brief, kb: &Knowledge, seed: u64, rerolls: &Rerolls) -> Score {
    let (brief, _) = repair(brief.clone(), kb);
    let Some(plan) = resolve(&brief, kb, seed, rerolls) else {
        return empty(kb, seed);
    };
    let timeline = chord_timeline(&plan);
    let cells = play(&plan, &timeline);
    let tracks = plan
        .tracks
        .iter()
        .zip(cells)
        .map(|(track, notes)| Track {
            id: track.id.to_string(),
            role: track.role,
            instrument: track.instrument.id.clone(),
            notes: notes.into_iter().flatten().collect(),
        })
        .collect();
    let mut score = Score {
        meta: ScoreMeta {
            tempo_bpm: plan.tempo,
            frame: plan.frame.clone(),
            tonic: plan.tonic,
            tuning_id: plan.tuning.id.clone(),
            data_version: kb.data_version().to_string(),
            seed,
        },
        tracks,
        sections: plan
            .sections
            .iter()
            .map(|section| SectionSpan {
                id: section.id.clone(),
                start: section.start,
                end: section.end,
            })
            .collect(),
    };
    critic::polish(&mut score, &brief, &plan, &timeline);
    score
}

/// The order parts are written in: later parts may read earlier ones.
const ORDER: [Role; 10] = [
    Role::Drums,
    Role::Percussion,
    Role::Bass,
    Role::Chords,
    Role::Pad,
    Role::Drone,
    Role::Lead,
    Role::Counter,
    Role::Arp,
    Role::Fx,
];

/// Every track's notes, one list per section: `cells[track][section]`.
fn play(plan: &Plan, timeline: &[Vec<ChordSpan>]) -> Vec<Vec<Vec<Note>>> {
    let mut cells: Vec<Vec<Vec<Note>>> =
        vec![vec![Vec::new(); plan.sections.len()]; plan.tracks.len()];
    for role in ORDER {
        for (ti, track) in plan
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.role == role)
        {
            // One motif for the whole piece, so every section can recall it.
            let idea = motif::motif(plan, track.id);
            for (si, section) in plan.sections.iter().enumerate() {
                if !section.active.contains(track.id) {
                    continue;
                }
                let chords = timeline.get(si).map_or(&[][..], Vec::as_slice);
                let notes = match role {
                    Role::Drums | Role::Percussion => parts::drums::drums(plan, track, section),
                    Role::Bass => {
                        let kicks = kicks_in(plan, &cells, si);
                        parts::bass::bass(plan, track, section, chords, &kicks)
                    }
                    Role::Chords | Role::Pad | Role::Drone => {
                        parts::chords::chords(plan, track, section, chords)
                    }
                    Role::Counter => {
                        let melody = lead_notes(plan, &cells, si);
                        parts::counter::counter(plan, track, section, chords, &melody)
                    }
                    Role::Arp => parts::arp::arp(plan, track, section, chords),
                    Role::Fx => parts::fx::fx(plan, track, section),
                    Role::Lead => parts::melody::lead(plan, track, section, chords, &idea),
                };
                cells[ti][si] = notes;
            }
        }
    }
    // Parts above read each other as written; only now are they played.
    for (ti, track) in plan.tracks.iter().enumerate() {
        for (si, section) in plan.sections.iter().enumerate() {
            let written = std::mem::take(&mut cells[ti][si]);
            cells[ti][si] = expression::express(plan, track, section, written);
        }
    }
    cells
}

/// The notes the first lead track plays in a section.
fn lead_notes(plan: &Plan, cells: &[Vec<Vec<Note>>], section: usize) -> Vec<Note> {
    plan.tracks
        .iter()
        .zip(cells)
        .find(|(track, _)| track.role == Role::Lead)
        .and_then(|(_, cells)| cells.get(section).cloned())
        .unwrap_or_default()
}

/// When the kick drum plays in a section: the first drum track's kick.
fn kicks_in(plan: &Plan, cells: &[Vec<Vec<Note>>], section: usize) -> Vec<Tick> {
    plan.tracks
        .iter()
        .zip(cells)
        .find(|(track, _)| track.role == Role::Drums)
        .and_then(|(track, cells)| {
            cells
                .get(section)
                .map(|notes| parts::drums::kick_onsets(notes, track))
        })
        .unwrap_or_default()
}

/// A score with nothing in it, for the brief that cannot be resolved even after repair.
fn empty(kb: &Knowledge, seed: u64) -> Score {
    Score {
        meta: ScoreMeta {
            tempo_bpm: 120,
            frame: TimeFrame::Meter(Meter {
                numerator: 4,
                denominator: 4,
                grouping: Meter::default_grouping(4, 4),
            }),
            tonic: Tonic::Class(t2m_core::PitchClass::C),
            tuning_id: "12tet".to_string(),
            data_version: kb.data_version().to_string(),
            seed,
        },
        tracks: Vec::new(),
        sections: Vec::new(),
    }
}

#[cfg(test)]
#[path = "compose_tests.rs"]
mod tests;
