//! The lead melody: phrases that open with the motif, move by step, land on
//! chord tones where the beat is strong and end each phrase with a held note and a rest.

use t2m_core::{Note, Pitch, Tick};

use crate::harmony::{ChordSpan, chord_at};
use crate::motif::{Cell, DevOp, develop};
use crate::parts::drums::beat_ticks;
use crate::parts::{note, single_line, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};
use crate::seed::Rng;

const OCTAVE: i32 = 1200;
const STRONG: i64 = 98;
const PLAIN: i64 = 84;
/// Out of a thousand: how often a quiet part rests between notes.
const REST_BASE: i64 = 150;
/// The widest a step may be, in cents: two scale steps of any common scale.
const MAX_STEP: i32 = 500;
/// Room kept above the line for the peak, in cents.
const HEADROOM: i32 = 500;
/// A leap this big or bigger pulls the line back toward the middle.
const FAR_FROM_CENTRE: i32 = 700;

/// One section of lead melody.
pub(crate) fn lead(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    spans: &[ChordSpan],
    cell: &Cell,
) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let beat = beat_ticks(plan);
    let strong = plan.frame.strong_positions();
    // The line stays below the top of the range so that the peak has room to rise above it.
    let working = PlanTrack {
        id: track.id,
        role: track.role,
        instrument: track.instrument,
        low: track.low,
        high: (track.high - HEADROOM).max(track.low),
        busy: track.busy,
        pattern: track.pattern,
        articulation: track.articulation,
    };
    let mut singer = Singer {
        plan,
        track: &working,
        top: track.high,
        peaks: Vec::new(),
        section,
        spans,
        beat,
        eighth: (beat / 2).max(1),
        strong,
        busy: track.busy.clamp(0.0, 1.0),
        notes: Vec::new(),
        last: track.center(),
    };
    let phrase_bars = if section.bars >= 4 && section.bars.is_multiple_of(4) {
        4
    } else {
        2
    };
    let length = Tick::from(phrase_bars) * plan.span;
    let mut start = section.start;
    let mut index = 0;
    while start < section.end {
        let end = (start + length).min(section.end);
        let last_phrase = end >= section.end;
        let op = match index % 4 {
            0 => DevOp::Repeat,
            1 => DevOp::Sequence(1),
            2 => DevOp::Invert,
            _ => DevOp::AnswerTo(0),
        };
        let idea = develop(cell, op, plan.system);
        singer.phrase(start, end, &idea, last_phrase, &mut rng);
        start = end;
        index += 1;
    }
    singer.raise_peak();
    tidy(single_line(singer.notes), section)
}

struct Singer<'a> {
    plan: &'a Plan<'a>,
    track: &'a PlanTrack<'a>,
    section: &'a PlanSection,
    spans: &'a [ChordSpan],
    beat: Tick,
    eighth: Tick,
    strong: Vec<Tick>,
    busy: f64,
    /// The top of the whole range, above the working range.
    top: i32,
    /// Which notes are strong chord tones: the peak is chosen among them.
    peaks: Vec<usize>,
    notes: Vec<Note>,
    /// The pitch of the note before, in cents.
    last: i32,
}

impl Singer<'_> {
    fn is_strong(&self, at: Tick) -> bool {
        let in_bar = (at - self.section.start).rem_euclid(self.plan.span.max(1));
        self.strong.contains(&in_bar)
    }

    /// The pitch classes of the chord at a tick; the tonic triad where there are no chords.
    fn chord_classes(&self, at: Tick) -> Vec<i32> {
        match chord_at(self.spans, at) {
            Some(span) => span
                .chord
                .tones
                .iter()
                .map(|tone| tone.cents.rem_euclid(OCTAVE))
                .collect(),
            None => [0, 2, 4]
                .iter()
                .filter(|degree| **degree < i32::from(self.plan.system.degrees()).max(1))
                .map(|degree| {
                    self.plan
                        .system
                        .pitch(self.plan.tuning, *degree)
                        .cents
                        .rem_euclid(OCTAVE)
                })
                .collect(),
        }
    }

    /// The chord tone in range nearest to `near`; one of the two nearest, picked by the stream.
    fn chord_tone(&self, at: Tick, near: i32, rng: &mut Rng, root: bool) -> i32 {
        let classes = self.chord_classes(at);
        let wanted: Vec<i32> = if root {
            classes.iter().take(1).copied().collect()
        } else {
            classes
        };
        let mut options: Vec<i32> = wanted
            .iter()
            .filter_map(|class| self.track.place(*class, near))
            .collect();
        options.sort_by_key(|pitch| ((pitch - near).abs(), *pitch));
        options.dedup();
        let choices = &options[..options.len().min(2)];
        match choices.len() {
            0 => near,
            1 => choices[0],
            _ => *rng.pick(choices).unwrap_or(&choices[0]),
        }
    }

    /// A pitch a step or two from `from`, in the scale, kept near the middle of the range.
    fn step(&self, from: i32, rng: &mut Rng) -> i32 {
        let system = self.plan.system;
        let tuning = self.plan.tuning;
        let here = system.nearest_degree(tuning, Pitch { cents: from });
        let pull = (from - self.track.center()).signum();
        let mut moves = [-2, -1, 1, 2];
        // Mostly single steps; the far end of the range turns the line back.
        let pick = rng.below(10);
        let size = if pick < 6 { 1 } else { 2 };
        let mut direction = if rng.below(2) == 0 { -1 } else { 1 };
        if (from - self.track.center()).abs() > FAR_FROM_CENTRE && rng.below(4) != 0 {
            direction = -pull;
        }
        moves.sort_by_key(|m| i32::from(*m * direction != size) * 10 + i32::from(m.abs() != size));
        for delta in moves {
            let there = here + delta;
            let cents = system.pitch(tuning, there).cents;
            let placed = self.track.place(cents, from);
            if let Some(pitch) = placed
                && system.legal_move(here, there)
                && (pitch - from).abs() <= MAX_STEP
            {
                return pitch;
            }
        }
        from
    }

    fn push(&mut self, start: Tick, dur: Tick, pitch: i32, vel: i64) {
        if dur > 0 {
            if vel == STRONG {
                self.peaks.push(self.notes.len());
            }
            self.notes.push(note(start, dur, pitch, vel));
            self.last = pitch;
        }
    }

    fn phrase(&mut self, start: Tick, end: Tick, idea: &Cell, last_phrase: bool, rng: &mut Rng) {
        // The held note that closes the phrase, then a rest.
        let cadence = (end - 2 * self.beat).max(start);
        let mut at = start + idea.offset.min(cadence - start);
        let opening = idea
            .degrees
            .first()
            .map(|degree| self.plan.system.pitch(self.plan.tuning, *degree).cents);
        // Where the idea's first note really landed; the rest keep their distance from it.
        let mut first: Option<i32> = None;
        for (degree, dur) in idea.degrees.iter().zip(&idea.durs) {
            if at + (self.eighth.min(*dur)) > cadence {
                break;
            }
            let dur = (*dur).min(cadence - at);
            let base = self.plan.system.pitch(self.plan.tuning, *degree).cents;
            let strong = self.is_strong(at);
            let pitch = match (strong, first) {
                (true, _) => self.chord_tone(at, self.last, rng, false),
                (false, Some(start)) => {
                    let wanted = start + opening.map_or(0, |open| base - open);
                    self.track
                        .place(wanted.rem_euclid(OCTAVE), wanted)
                        .unwrap_or(self.last)
                }
                (false, None) => self.last,
            };
            first.get_or_insert(pitch);
            self.push(at, dur, pitch, if strong { STRONG } else { PLAIN });
            at += dur;
        }
        while at < cadence {
            let dur = self.duration(cadence - at, rng);
            let pitch = if self.is_strong(at) {
                self.chord_tone(at, self.last, rng, false)
            } else {
                self.step(self.last, rng)
            };
            let vel = if self.is_strong(at) { STRONG } else { PLAIN };
            self.push(at, dur, pitch, vel);
            at += dur;
            let rest = (1.0 - self.busy) * REST_BASE as f64;
            if at < cadence && (rng.below(1000) as f64) < rest {
                at += self.eighth.min(cadence - at);
            }
        }
        let pitch = self.chord_tone(cadence, self.last, rng, last_phrase);
        self.push(cadence, self.beat.min(end - cadence), pitch, STRONG);
    }

    /// Makes the section's highest note unique, and puts it in the second half.
    ///
    /// A chord tone on a strong beat late in the section is lifted to the
    /// nearest chord tone above every other note. If the range leaves no room
    /// for that, the notes that would reach it drop an octave instead.
    fn raise_peak(&mut self) {
        let middle = self.section.start + (self.section.end - self.section.start) / 2;
        let notes = &self.notes;
        let Some(chosen) = self
            .peaks
            .iter()
            .copied()
            .filter(|index| notes[*index].start >= middle)
            .max_by_key(|index| notes[*index].pitch.cents)
        else {
            return;
        };
        let ceiling = notes
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != chosen)
            .map(|(_, n)| n.pitch.cents)
            .max()
            .unwrap_or(i32::MIN);
        let at = notes[chosen].start;
        let lifted = self
            .chord_classes(at)
            .iter()
            .flat_map(|class| (-8..16).map(move |octave| class + octave * OCTAVE))
            .filter(|pitch| *pitch > ceiling && *pitch <= self.top && *pitch >= self.track.low)
            .min();
        let top = lifted.unwrap_or(self.notes[chosen].pitch.cents);
        self.notes[chosen].pitch = Pitch { cents: top };
        let low = self.track.low;
        let mut index = 0;
        self.notes.retain_mut(|n| {
            let mine = index == chosen;
            index += 1;
            while !mine && n.pitch.cents >= top {
                n.pitch.cents -= OCTAVE;
            }
            mine || n.pitch.cents >= low
        });
    }

    /// How long the next note lasts: shorter when the part is busy, never past `room`.
    fn duration(&self, room: Tick, rng: &mut Rng) -> Tick {
        let eighth = 100.0 + 700.0 * self.busy;
        let half = (1.0 - self.busy) * 250.0;
        let pick = f64::from(rng.below(1000));
        let wanted = if pick < eighth {
            self.eighth
        } else if pick > 1000.0 - half {
            2 * self.beat
        } else {
            self.beat
        };
        wanted.min(room).max(1)
    }
}

#[cfg(test)]
#[path = "melody_tests.rs"]
mod tests;
