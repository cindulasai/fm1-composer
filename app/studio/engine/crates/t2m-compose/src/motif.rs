//! The motif: a short idea, kept in scale steps, and the ways to develop it.

use t2m_core::{Tick, TonalSystem};

use crate::parts::drums::beat_ticks;
use crate::plan::Plan;
use crate::seed::Rng;

/// Most notes in a motif.
const MAX_NOTES: usize = 8;
/// The generated motif's total length, in beats.
const SPAN_BEATS: Tick = 2;
/// How far from the tonic a generated motif may wander, in scale steps.
const WANDER: (i32, i32) = (-3, 8);

/// A few notes: scale steps above the tonic, each held for a number of ticks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub degrees: Vec<i32>,
    pub durs: Vec<Tick>,
    /// Ticks of rest before the first note.
    pub offset: Tick,
}

impl Cell {
    /// Ticks from the start of the cell to the end of its last note.
    #[cfg(test)]
    pub(crate) fn length(&self) -> Tick {
        self.offset + self.durs.iter().sum::<Tick>()
    }
}

/// How a cell is changed between phrases. The melody uses Repeat, Sequence, Invert and
/// AnswerTo; the others are tested developments held ready for richer phrasing.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DevOp {
    Repeat,
    /// Every note this many scale steps higher (or lower).
    Sequence(i32),
    /// Mirrored around the first note.
    Invert,
    /// Every note twice as long.
    Augment,
    /// Every note half as long.
    Diminish,
    /// Only the first notes, this many.
    Fragment(usize),
    /// Starts this many ticks later.
    Displace(Tick),
    /// The last note moved to this scale degree's pitch class, as a reply.
    AnswerTo(i32),
}

/// The piece's motif: the brief's, made playable, or one invented from the seed.
pub(crate) fn motif(plan: &Plan, track: &str) -> Cell {
    let beat = beat_ticks(plan);
    let given = plan.motif.and_then(|motif| {
        let count = motif
            .degrees
            .len()
            .min(motif.durations.len())
            .min(MAX_NOTES);
        let durs: Vec<Tick> = motif.durations[..count]
            .iter()
            .map(|beats| ticks(*beats, beat))
            .collect();
        (count > 0).then(|| Cell {
            degrees: motif.degrees[..count].to_vec(),
            durs,
            offset: 0,
        })
    });
    match given {
        Some(cell) => snap(&cell, plan.system),
        // A stream of its own, so every section hears the same invented motif.
        None => invent(plan, &mut plan.rng(track, "~motif"), beat),
    }
}

/// A length in beats as ticks, never less than a thirty-second of a whole note.
fn ticks(beats: f64, beat: Tick) -> Tick {
    let wanted = if beats.is_finite() {
        (beats * beat as f64).round().clamp(0.0, (beat * 64) as f64) as Tick
    } else {
        beat
    };
    wanted.max(beat / 8).max(1)
}

/// The cell with every step the system forbids replaced by the nearest one it allows.
pub(crate) fn snap(cell: &Cell, system: &TonalSystem) -> Cell {
    let mut degrees: Vec<i32> = Vec::with_capacity(cell.degrees.len());
    for degree in &cell.degrees {
        let Some(previous) = degrees.last().copied() else {
            degrees.push(*degree);
            continue;
        };
        let mut chosen = None;
        for offset in 0..=i32::from(system.degrees()) {
            for candidate in [degree - offset, degree + offset] {
                if chosen.is_none() && system.legal_move(previous, candidate) {
                    chosen = Some(candidate);
                }
            }
        }
        // Nothing near is allowed: the note repeats the one before it.
        degrees.push(chosen.unwrap_or(previous));
    }
    Cell {
        degrees,
        durs: cell.durs.clone(),
        offset: cell.offset,
    }
}

fn invent(plan: &Plan, rng: &mut Rng, beat: Tick) -> Cell {
    // Three or four notes, each at least an eighth, in two beats.
    let count = 3 + rng.below(2) as usize;
    let eighth = (beat / 2).max(1);
    let choices = [eighth, eighth, beat, beat, beat + eighth];
    let mut durs: Vec<Tick> = Vec::new();
    let mut used = 0;
    for left in (0..count).rev() {
        // Leave an eighth for each note still to come.
        let room = SPAN_BEATS * beat - used - Tick::try_from(left).unwrap_or(0) * eighth;
        let dur = rng.pick(&choices).copied().unwrap_or(beat).min(room).max(1);
        durs.push(dur);
        used += dur;
    }
    let starts: [i32; 3] = [0, 2, 4];
    let mut degree = rng.pick(&starts).copied().unwrap_or(0);
    let mut degrees = Vec::with_capacity(durs.len());
    for _ in &durs {
        degrees.push(degree);
        let step = i32::try_from(rng.between(-2, 2)).unwrap_or(0);
        degree = (degree + step).clamp(WANDER.0, WANDER.1);
    }
    snap(
        &Cell {
            degrees,
            durs,
            offset: 0,
        },
        plan.system,
    )
}

/// A changed copy of a cell.
pub(crate) fn develop(cell: &Cell, op: DevOp, system: &TonalSystem) -> Cell {
    let mut next = cell.clone();
    match op {
        DevOp::Repeat => {}
        DevOp::Sequence(steps) => {
            for degree in &mut next.degrees {
                *degree = degree.saturating_add(steps);
            }
        }
        DevOp::Invert => {
            let anchor = cell.degrees.first().copied().unwrap_or(0);
            for degree in &mut next.degrees {
                *degree = anchor.saturating_mul(2).saturating_sub(*degree);
            }
        }
        DevOp::Augment => {
            for dur in &mut next.durs {
                *dur = dur.saturating_mul(2);
            }
        }
        DevOp::Diminish => {
            for dur in &mut next.durs {
                *dur = (*dur / 2).max(1);
            }
        }
        DevOp::Fragment(count) => {
            let keep = count.clamp(1, cell.degrees.len().max(1));
            next.degrees.truncate(keep);
            next.durs.truncate(keep);
        }
        DevOp::Displace(by) => next.offset = (next.offset + by).max(0),
        DevOp::AnswerTo(target) => {
            let count = i32::from(system.degrees()).max(1);
            if let Some(last) = next.degrees.last_mut() {
                // The octave of the target nearest the note it replaces.
                let base = last.div_euclid(count) * count + target.rem_euclid(count);
                *last = [base - count, base, base + count]
                    .into_iter()
                    .min_by_key(|candidate| (candidate - *last).abs())
                    .unwrap_or(base);
            }
        }
    }
    snap(&next, system)
}

#[cfg(test)]
#[path = "motif_tests.rs"]
mod tests;
