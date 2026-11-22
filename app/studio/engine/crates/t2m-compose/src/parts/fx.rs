//! Effects: a riser through the last bar of a section that asks for one, and
//! an impact on the first beat after a section that asks for that.

use t2m_core::brief::Transition;
use t2m_core::{BendCurve, Note, Tick};

use crate::parts::drums::beat_ticks;
use crate::parts::{note, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};

const RISER: i64 = 84;
const IMPACT: i64 = 118;
/// A riser climbs this far over its bar, in cents.
const CLIMB: i32 = 1200;

/// One section of effects.
pub(crate) fn fx(plan: &Plan, track: &PlanTrack, section: &PlanSection) -> Vec<Note> {
    let mut notes = Vec::new();
    if section.transition == Transition::Riser {
        let start = section.end - plan.span;
        if start >= section.start
            && let Some(pitch) = track.place(0, track.center())
        {
            let mut rise = note(start, plan.span, pitch, RISER);
            rise.bend = Some(BendCurve(vec![(0, 0), (plan.span, CLIMB)]));
            notes.push(rise);
        }
    }
    if section.entered_by == Transition::Impact
        && let Some(pitch) = track.place(0, track.low)
    {
        notes.push(note(
            section.start,
            hit_length(plan, section),
            pitch,
            IMPACT,
        ));
    }
    tidy(notes, section)
}

/// An impact rings for two beats, or the whole section if it is shorter.
fn hit_length(plan: &Plan, section: &PlanSection) -> Tick {
    (2 * beat_ticks(plan)).min(section.end - section.start)
}

#[cfg(test)]
#[path = "fx_tests.rs"]
mod tests;
