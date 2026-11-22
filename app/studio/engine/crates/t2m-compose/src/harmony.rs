//! The chord timeline every pitched part reads, with voice-led voicings.
//!
//! Each section is worked out by itself, so changing one section's harmony
//! never alters another section's notes.

use t2m_core::brief::SectionHarmony;
use t2m_core::{Chord, Pitch, Tick, parse_roman};

use crate::plan::{Plan, PlanSection};

const OCTAVE: i32 = 1200;
/// Voicings are built in this range (MIDI notes); parts move them to their own.
const VOICING_RANGE: (i32, i32) = (48, 76);
/// Most notes in one voicing.
const MAX_VOICES: usize = 4;

/// One chord, from `start` up to `end`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChordSpan {
    pub start: Tick,
    pub end: Tick,
    pub chord: Chord,
    /// The chord as sounded: ascending, moving as little as possible from the chord before.
    pub voicing: Vec<Pitch>,
}

/// The chords of every section, in section order. Empty for sections without chords.
pub(crate) fn chord_timeline(plan: &Plan) -> Vec<Vec<ChordSpan>> {
    plan.sections
        .iter()
        .map(|section| section_chords(plan, section))
        .collect()
}

/// The chord sounding at `tick`, if any.
pub(crate) fn chord_at(spans: &[ChordSpan], tick: Tick) -> Option<&ChordSpan> {
    spans
        .iter()
        .find(|span| span.start <= tick && tick < span.end)
}

fn section_chords(plan: &Plan, section: &PlanSection) -> Vec<ChordSpan> {
    let SectionHarmony::Progression {
        roman,
        bars_per_chord,
    } = &section.harmony
    else {
        return Vec::new();
    };
    let chords: Vec<Chord> = roman
        .iter()
        .filter_map(|symbol| parse_roman(symbol, plan.system, plan.tuning).ok())
        .collect();
    if chords.is_empty() {
        return Vec::new();
    }
    let low = plan.cents_of_midi(VOICING_RANGE.0);
    let high = plan.cents_of_midi(VOICING_RANGE.1);
    let length = Tick::from((*bars_per_chord).max(1)) * plan.span;
    let mut spans: Vec<ChordSpan> = Vec::new();
    let mut start = section.start;
    for chord in chords.iter().cycle() {
        if start >= section.end {
            break;
        }
        let previous = spans.last().map(|span| span.voicing.as_slice());
        let voicing = voice(chord, previous, low, high);
        let end = (start + length).min(section.end);
        spans.push(ChordSpan {
            start,
            end,
            chord: chord.clone(),
            voicing,
        });
        start = end;
    }
    spans
}

/// The pitch classes to sound: at most four, keeping the tones that define the chord.
fn voiced_classes(chord: &Chord) -> Vec<i32> {
    let mut classes: Vec<i32> = Vec::new();
    for tone in &chord.tones {
        let class = tone.cents.rem_euclid(OCTAVE);
        if !classes.contains(&class) {
            classes.push(class);
        }
    }
    // Too many notes: the fifth goes first, then the root (the bass has it).
    for dropped in [2, 0] {
        if classes.len() > MAX_VOICES {
            let target = chord
                .tones
                .get(dropped)
                .map(|tone| tone.cents.rem_euclid(OCTAVE));
            classes.retain(|class| Some(*class) != target);
        }
    }
    classes.truncate(MAX_VOICES);
    classes.sort_unstable();
    classes
}

/// Every close-position arrangement of the classes that fits the range.
fn candidates(classes: &[i32], low: i32, high: i32) -> Vec<Vec<i32>> {
    let mut found = Vec::new();
    for rotation in 0..classes.len() {
        // Stack the classes upward starting from this one.
        let mut stack: Vec<i32> = Vec::with_capacity(classes.len());
        for offset in 0..classes.len() {
            let class = classes[(rotation + offset) % classes.len()];
            let pitch = match stack.last() {
                Some(below) => below + (class - below).rem_euclid(OCTAVE),
                None => class,
            };
            stack.push(pitch);
        }
        let (Some(bottom), Some(top)) = (stack.first().copied(), stack.last().copied()) else {
            continue;
        };
        // Lift the stack to the lowest octave inside the range, then try each octave above.
        let mut shift = (low - bottom).div_euclid(OCTAVE) * OCTAVE;
        if bottom + shift < low {
            shift += OCTAVE;
        }
        while top + shift <= high {
            found.push(stack.iter().map(|pitch| pitch + shift).collect());
            shift += OCTAVE;
        }
    }
    found
}

/// How far the voices move between two voicings: each note to its nearest neighbour, both ways.
fn motion(from: &[i32], to: &[i32]) -> i32 {
    let nearest = |pitch: i32, others: &[i32]| -> i32 {
        others
            .iter()
            .map(|other| (other - pitch).abs())
            .min()
            .unwrap_or(0)
    };
    let forward: i32 = to.iter().map(|pitch| nearest(*pitch, from)).sum();
    let backward: i32 = from.iter().map(|pitch| nearest(*pitch, to)).sum();
    forward + backward
}

fn voice(chord: &Chord, previous: Option<&[Pitch]>, low: i32, high: i32) -> Vec<Pitch> {
    let classes = voiced_classes(chord);
    let options = candidates(&classes, low, high);
    let center = low + (high - low) / 2;
    let previous: Option<Vec<i32>> =
        previous.map(|pitches| pitches.iter().map(|p| p.cents).collect());
    let cost = |option: &Vec<i32>| -> i32 {
        match &previous {
            Some(before) => motion(before, option),
            // The first chord of a section sits around the middle of the range.
            None => option.iter().map(|pitch| (pitch - center).abs()).sum(),
        }
    };
    let best = options.iter().min_by_key(|option| cost(option));
    let chosen = match best {
        Some(option) => option.clone(),
        // The range cannot hold the chord in close position: stack it from the bottom.
        None => classes
            .iter()
            .map(|class| low + (class - low).rem_euclid(OCTAVE))
            .collect(),
    };
    let mut voicing: Vec<Pitch> = chosen.into_iter().map(|cents| Pitch { cents }).collect();
    voicing.sort();
    voicing
}

#[cfg(test)]
#[path = "harmony_tests.rs"]
mod tests;
