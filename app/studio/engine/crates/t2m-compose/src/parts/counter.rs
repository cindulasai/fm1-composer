//! The counter-melody: a second voice that speaks where the lead is resting
//! or holding a long note, always underneath it.

use t2m_core::{Note, Tick};

use crate::harmony::{ChordSpan, chord_at};
use crate::parts::drums::beat_ticks;
use crate::parts::{note, single_line, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};

const VEL: i64 = 80;
/// Out of a thousand, how likely an open slot is to be used with the part at busy 0 and 1.
const USE: (f64, f64) = (150.0, 650.0);

/// One section of counter-melody against the lead's notes in the same section.
pub(crate) fn counter(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    spans: &[ChordSpan],
    lead: &[Note],
) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let beat = beat_ticks(plan);
    let eighth = (beat / 2).max(1);
    let chance = USE.0 + (USE.1 - USE.0) * track.busy.clamp(0.0, 1.0);
    let mut notes: Vec<Note> = Vec::new();
    let mut last = track.center();
    let mut at = section.start;
    while at < section.end {
        let Some(above) = open_slot(lead, at, beat) else {
            at += eighth;
            continue;
        };
        if f64::from(rng.below(1000)) >= chance {
            at += eighth;
            continue;
        }
        // Chord tones in range, below the lead where it sounds, never its pitch class.
        let classes: Vec<i32> = chord_at(spans, at)
            .map(|span| {
                span.chord
                    .tones
                    .iter()
                    .map(|t| t.cents.rem_euclid(1200))
                    .collect()
            })
            .unwrap_or_else(|| vec![0, 400, 700]);
        let mut options: Vec<i32> = classes
            .iter()
            .filter_map(|class| track.place(*class, last))
            .collect();
        options.extend(
            classes
                .iter()
                .filter_map(|class| track.place(*class, last - 1200)),
        );
        options.retain(|pitch| {
            above.is_none_or(|lead_pitch| {
                *pitch < lead_pitch && pitch.rem_euclid(1200) != lead_pitch.rem_euclid(1200)
            })
        });
        options.sort_by_key(|pitch| ((pitch - last).abs(), *pitch));
        options.dedup();
        let choices = &options[..options.len().min(2)];
        if let Some(pitch) = rng.pick(choices).copied() {
            // Until the lead speaks again, at most two beats.
            let until = lead
                .iter()
                .map(|n| n.start)
                .find(|start| *start > at)
                .unwrap_or(section.end)
                .min(at + 2 * beat);
            if until > at {
                notes.push(note(at, until - at, pitch, VEL));
                last = pitch;
                at = until;
                continue;
            }
        }
        at += eighth;
    }
    tidy(single_line(notes), section)
}

/// Whether the counter may start at `at`: the lead is resting or holding a note of
/// a beat or more. `Some(None)` is a rest; `Some(Some(pitch))` is a held lead note.
fn open_slot(lead: &[Note], at: Tick, beat: Tick) -> Option<Option<i32>> {
    if lead.iter().any(|n| n.start == at) {
        return None;
    }
    match lead.iter().find(|n| n.start < at && at < n.end()) {
        None => Some(None),
        Some(held) if held.dur >= beat => Some(Some(held.pitch.cents)),
        Some(_) => None,
    }
}

#[cfg(test)]
#[path = "counter_tests.rs"]
mod tests;
