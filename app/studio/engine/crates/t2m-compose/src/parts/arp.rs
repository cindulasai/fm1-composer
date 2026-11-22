//! The arpeggio: the notes of each chord, one at a time, in a repeating shape.

use t2m_core::Note;

use crate::harmony::ChordSpan;
use crate::parts::drums::beat_ticks;
use crate::parts::{note, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};
use crate::seed::Rng;

const STRONG: i64 = 90;
const PLAIN: i64 = 76;
/// The most notes in the pool a shape moves through.
const POOL: usize = 8;
/// An arpeggio at or above this busyness plays sixteenths, below it eighths.
const FAST: f64 = 0.5;

/// The shape of an arpeggio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    Up,
    Down,
    UpDown,
    Random,
}

impl Shape {
    /// A shape by name, with or without the `arp.` prefix.
    pub(crate) fn parse(name: &str) -> Option<Shape> {
        let name = name.trim();
        Some(match name.strip_prefix("arp.").unwrap_or(name) {
            "up" => Shape::Up,
            "down" => Shape::Down,
            "updown" => Shape::UpDown,
            "random_seeded" | "random" => Shape::Random,
            _ => return None,
        })
    }
}

/// One section of arpeggio.
pub(crate) fn arp(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    spans: &[ChordSpan],
) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let shape = Shape::parse(track.pattern).unwrap_or(Shape::Up);
    let beat = beat_ticks(plan);
    let step = if track.busy >= FAST {
        beat / 4
    } else {
        beat / 2
    }
    .max(1);
    let mut notes = Vec::new();
    for span in spans {
        let pool = pool(track, span);
        if pool.is_empty() {
            continue;
        }
        let mut at = span.start;
        let mut index = 0usize;
        while at < span.end {
            let pitch = pick(shape, &pool, index, &mut rng);
            let until = (at + step).min(span.end);
            let strong = (at - span.start) % beat == 0;
            notes.push(note(
                at,
                until - at,
                pitch,
                if strong { STRONG } else { PLAIN },
            ));
            at += step;
            index += 1;
        }
    }
    tidy(notes, section)
}

/// The chord's pitch classes at every octave the part can play, lowest first.
fn pool(track: &PlanTrack, span: &ChordSpan) -> Vec<i32> {
    let mut classes: Vec<i32> = span
        .voicing
        .iter()
        .map(|p| p.cents.rem_euclid(1200))
        .collect();
    classes.sort_unstable();
    classes.dedup();
    let mut pool: Vec<i32> = Vec::new();
    let mut octave = track.low.div_euclid(1200) * 1200;
    while octave <= track.high {
        pool.extend(
            classes
                .iter()
                .map(|class| octave + class)
                .filter(|pitch| *pitch >= track.low && *pitch <= track.high),
        );
        octave += 1200;
    }
    pool.truncate(POOL);
    pool
}

fn pick(shape: Shape, pool: &[i32], index: usize, rng: &mut Rng) -> i32 {
    let len = pool.len();
    let at = match shape {
        Shape::Up => index % len,
        Shape::Down => len - 1 - index % len,
        Shape::UpDown => {
            // Up to the top and back, without playing the turning notes twice.
            let cycle = (2 * len).saturating_sub(2).max(1);
            let place = index % cycle;
            if place < len { place } else { cycle - place }
        }
        Shape::Random => rng.below(u32::try_from(len).unwrap_or(1)) as usize,
    };
    pool[at.min(len - 1)]
}

#[cfg(test)]
#[path = "arp_tests.rs"]
mod tests;
