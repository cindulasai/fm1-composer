//! Expression: what turns a correct part into a played one. Section dynamics,
//! a swell across each phrase, note lengths by articulation, and a little
//! human unevenness in timing. Each section of each part is treated alone, so
//! changing one never changes another.

use t2m_core::Note;
use t2m_core::brief::{Articulation, Role};

use crate::plan::{Plan, PlanSection, PlanTrack};

/// Loudest and softest section, as a share of the written velocity.
const DYNAMICS: (f64, f64) = (0.55, 1.0);
/// A phrase swells from this much below to this much above the written velocity.
const ARC: f64 = 12.0;
/// Bars in the phrase the swell runs across.
const PHRASE_BARS: i64 = 4;

/// The same notes, played.
pub(crate) fn express(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    mut notes: Vec<Note>,
) -> Vec<Note> {
    let drums = matches!(track.role, Role::Drums | Role::Percussion);
    let scale = DYNAMICS.0 + (DYNAMICS.1 - DYNAMICS.0) * section.energy.clamp(0.0, 1.0);
    let phrase = (PHRASE_BARS * plan.span).max(1);
    let share = length_share(track.articulation);
    let reach = i64::from(plan.groove.human_ticks);
    let mut rng = plan.rng_labelled(track.id, &section.id, "~human");
    for note in &mut notes {
        let mut loud = f64::from(note.vel) * scale;
        if !drums {
            let place = ((note.start - section.start) % phrase) as f64 / phrase as f64;
            // Soft at the edges of the phrase, loudest in the middle.
            loud += ((1.0 - (2.0 * place - 1.0).abs()) - 0.5) * 2.0 * ARC;
            let end = note.end();
            note.dur = (note.dur * share.0 / share.1).max(1);
            // Late or early, but the note still ends where it did, inside its section.
            let moved = (note.start + rng.between(-reach, reach)).clamp(section.start, end - 1);
            note.dur = (note.dur - (moved - note.start)).max(1);
            note.start = moved;
        }
        note.vel = u8::try_from((loud.round() as i64).clamp(1, 127)).unwrap_or(1);
    }
    notes.sort_by_key(|note| (note.start, note.pitch.cents));
    // Moving starts can bring two notes of one pitch together: the earlier gives way.
    for index in 1..notes.len() {
        let (before, after) = notes.split_at_mut(index);
        let current = &after[0];
        if let Some(earlier) = before
            .iter_mut()
            .rev()
            .find(|n| n.pitch == current.pitch && n.end() > current.start)
        {
            earlier.dur = (current.start - earlier.start).max(1);
        }
    }
    notes.retain(|note| note.dur > 0 && note.end() <= section.end);
    notes
}

/// How much of a note's written length is sounded, as a fraction.
fn length_share(articulation: Articulation) -> (i64, i64) {
    match articulation {
        Articulation::Legato => (1, 1),
        Articulation::Normal => (9, 10),
        Articulation::Staccato => (1, 2),
    }
}

#[cfg(test)]
#[path = "expression_tests.rs"]
mod tests;
