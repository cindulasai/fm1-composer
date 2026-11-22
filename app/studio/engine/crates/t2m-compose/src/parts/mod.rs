//! Part generators: one pure function per role, each writing one section.

use t2m_core::{Note, Pitch, Tick};

use crate::plan::PlanSection;

pub(crate) mod arp;
pub(crate) mod bass;
pub(crate) mod chords;
pub(crate) mod counter;
pub(crate) mod drums;
pub(crate) mod fx;
pub(crate) mod melody;

/// A plain note.
pub(crate) fn note(start: Tick, dur: Tick, cents: i32, vel: i64) -> Note {
    Note {
        start,
        dur,
        pitch: Pitch { cents },
        vel: velocity(vel),
        bend: None,
    }
}

/// A loudness brought into MIDI's 1 to 127.
pub(crate) fn velocity(value: i64) -> u8 {
    u8::try_from(value.clamp(1, 127)).unwrap_or(1)
}

/// Makes a part's notes for one section playable: inside the section, in
/// order, and never sounding a pitch that is already sounding.
pub(crate) fn tidy(mut notes: Vec<Note>, section: &PlanSection) -> Vec<Note> {
    for note in &mut notes {
        note.start = note.start.clamp(section.start, section.end - 1);
        note.dur = note.dur.min(section.end - note.start);
    }
    notes.sort_by_key(|note| (note.start, note.pitch.cents));
    let mut kept: Vec<Note> = Vec::with_capacity(notes.len());
    for note in notes {
        // The most recent note of this pitch gives way to the new one.
        if let Some(earlier) = kept.iter_mut().rev().find(|k| k.pitch == note.pitch)
            && earlier.end() > note.start
        {
            earlier.dur = note.start - earlier.start;
        }
        kept.push(note);
    }
    kept.retain(|note| note.dur > 0);
    kept
}

/// Makes a line monophonic: each note stops when the next one starts.
pub(crate) fn single_line(mut notes: Vec<Note>) -> Vec<Note> {
    notes.sort_by_key(|note| note.start);
    notes.dedup_by_key(|note| note.start);
    for index in 1..notes.len() {
        let next_start = notes[index].start;
        let earlier = &mut notes[index - 1];
        earlier.dur = earlier.dur.min(next_start - earlier.start);
    }
    notes
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
