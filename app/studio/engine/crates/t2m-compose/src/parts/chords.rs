//! Chords, pads and drones: the voicings of each chord, played in one of a few styles.

use t2m_core::brief::{Role, SectionHarmony};
use t2m_core::{Note, Tick};

use crate::harmony::ChordSpan;
use crate::parts::drums::beat_ticks;
use crate::parts::{note, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};
use crate::seed::Rng;

const STRONG: i64 = 92;
const PLAIN: i64 = 78;
/// Milliseconds between the strings of a strum.
const STRUM_MS: (i64, i64) = (10, 30);
/// Eighth-note positions in order of how readily a comping hit goes there.
const COMPING_ORDER: [i64; 8] = [0, 3, 6, 2, 5, 4, 7, 1];

/// How a chord part plays its chords.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    Sustain,
    Stabs,
    Strum,
    Comping,
    Broken,
    Drone,
}

impl Family {
    /// A family by name, with or without the `chords.` prefix.
    pub(crate) fn parse(name: &str) -> Option<Family> {
        let name = name.trim();
        let name = name.strip_prefix("chords.").unwrap_or(name);
        Some(match name {
            "sustain" => Family::Sustain,
            "stabs" => Family::Stabs,
            "strum" => Family::Strum,
            "comping" => Family::Comping,
            "broken" => Family::Broken,
            "drone" => Family::Drone,
            _ => return None,
        })
    }

    /// What the role and the groove's style usually play.
    pub(crate) fn default_for(role: Role, groove: &str) -> Family {
        match (role, groove) {
            (Role::Drone, _) => Family::Drone,
            (Role::Pad, _) => Family::Sustain,
            (_, "groove.house" | "groove.techno") => Family::Stabs,
            (_, "groove.one_drop") => Family::Stabs,
            (_, "groove.funk_16" | "groove.jazz_ride" | "groove.bossa" | "groove.afrobeats") => {
                Family::Comping
            }
            (_, "groove.rock_8") => Family::Strum,
            (_, "groove.lofi_swing" | "groove.boom_bap") => Family::Broken,
            _ => Family::Sustain,
        }
    }
}

/// One section of a chord, pad or drone part.
pub(crate) fn chords(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    spans: &[ChordSpan],
) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let drone_section = matches!(section.harmony, SectionHarmony::Drone { .. });
    let family = if drone_section || track.role == Role::Drone && spans.is_empty() {
        Family::Drone
    } else {
        Family::parse(track.pattern)
            .filter(|family| *family != Family::Drone)
            .unwrap_or_else(|| Family::default_for(track.role, &plan.groove.id))
    };
    let mut notes = Vec::new();
    if family == Family::Drone {
        drone(plan, track, section, &mut notes);
    }
    let beat = beat_ticks(plan);
    for span in spans.iter().filter(|_| family != Family::Drone) {
        let pitches = placed(track, span);
        let shape = Shape {
            span,
            pitches: &pitches,
            beat,
            eighth: (beat / 2).max(1),
            bar: plan.span,
            density: section.density,
            tempo: i64::from(plan.tempo),
        };
        match family {
            Family::Sustain => sustain(&shape, &mut notes),
            Family::Stabs => stabs(&shape, &mut notes),
            Family::Strum => strum(&shape, &mut rng, &mut notes),
            Family::Comping => comping(&shape, &mut notes),
            Family::Broken => broken(&shape, &mut notes),
            Family::Drone => {}
        }
    }
    tidy(notes, section)
}

/// The span's voicing, each note moved by whole octaves into the part's range.
fn placed(track: &PlanTrack, span: &ChordSpan) -> Vec<i32> {
    let mut pitches: Vec<i32> = span
        .voicing
        .iter()
        .filter_map(|pitch| track.place(pitch.cents, pitch.cents))
        .collect();
    pitches.sort_unstable();
    pitches.dedup();
    pitches
}

struct Shape<'a> {
    span: &'a ChordSpan,
    pitches: &'a [i32],
    beat: Tick,
    eighth: Tick,
    bar: Tick,
    density: f64,
    tempo: i64,
}

impl Shape<'_> {
    fn bars(&self) -> impl Iterator<Item = Tick> {
        let bars = (self.span.end - self.span.start + self.bar - 1) / self.bar;
        let (start, bar) = (self.span.start, self.bar);
        (0..bars.max(1)).map(move |index| start + index * bar)
    }

    fn hit(&self, notes: &mut Vec<Note>, at: Tick, until: Tick, vel: i64) {
        let until = until.min(self.span.end);
        if until > at {
            for pitch in self.pitches {
                notes.push(note(at, until - at, *pitch, vel));
            }
        }
    }
}

fn sustain(shape: &Shape, notes: &mut Vec<Note>) {
    shape.hit(notes, shape.span.start, shape.span.end, STRONG);
}

/// Short hits: one on the first beat of each bar, and more as the section gets denser.
fn stabs(shape: &Shape, notes: &mut Vec<Note>) {
    let per_bar = 1 + (shape.density.clamp(0.0, 1.0) * 3.0).round() as i64;
    for bar_start in shape.bars() {
        for index in 0..per_bar {
            let at = bar_start + index * shape.beat;
            if at >= bar_start + shape.bar {
                break;
            }
            let vel = if index == 0 { STRONG } else { PLAIN };
            shape.hit(notes, at, at + shape.eighth, vel);
        }
    }
}

/// The strings of the chord sounded low to high a few milliseconds apart, once a bar.
fn strum(shape: &Shape, rng: &mut Rng, notes: &mut Vec<Note>) {
    let ms = rng.between(STRUM_MS.0, STRUM_MS.1);
    let gap = (ms * shape.tempo * 16 / 1000).max(1);
    for bar_start in shape.bars() {
        let until = (bar_start + shape.bar).min(shape.span.end);
        for (index, pitch) in shape.pitches.iter().enumerate() {
            let at = bar_start + Tick::try_from(index).unwrap_or(0) * gap;
            if at < until {
                notes.push(note(at, until - at, *pitch, STRONG));
            }
        }
    }
}

/// Syncopated hits on the off-beats first; a denser section adds more of them.
fn comping(shape: &Shape, notes: &mut Vec<Note>) {
    let eighths = (shape.bar / shape.eighth).max(1);
    let wanted = 2 + (shape.density.clamp(0.0, 1.0) * 5.0).round() as usize;
    for bar_start in shape.bars() {
        let mut positions: Vec<i64> = COMPING_ORDER
            .iter()
            .copied()
            .filter(|p| *p < eighths)
            .take(wanted)
            .collect();
        positions.sort_unstable();
        for (index, position) in positions.iter().enumerate() {
            let at = bar_start + position * shape.eighth;
            let next = positions
                .get(index + 1)
                .map_or(bar_start + shape.bar, |p| bar_start + p * shape.eighth);
            // A hit rings for half the time to the next one.
            let until = at + ((next - at) / 2).max(shape.eighth.min(next - at));
            let vel = if *position == 0 { STRONG } else { PLAIN };
            shape.hit(notes, at, until, vel);
        }
    }
}

/// The chord's notes one at a time, upward, an eighth apart.
fn broken(shape: &Shape, notes: &mut Vec<Note>) {
    if shape.pitches.is_empty() {
        return;
    }
    let mut at = shape.span.start;
    let mut index = 0;
    while at < shape.span.end {
        let pitch = shape.pitches[index % shape.pitches.len()];
        let until = (at + shape.eighth).min(shape.span.end);
        notes.push(note(
            at,
            until - at,
            pitch,
            if index == 0 { STRONG } else { PLAIN },
        ));
        index += 1;
        at += shape.eighth;
    }
}

/// The drone degrees, held through the whole section.
fn drone(plan: &Plan, track: &PlanTrack, section: &PlanSection, notes: &mut Vec<Note>) {
    let degrees: Vec<i32> = match &section.harmony {
        SectionHarmony::Drone { degrees } => degrees.iter().map(|d| i32::from(*d) - 1).collect(),
        // A drone part under chords holds the tonic.
        _ => vec![0],
    };
    for degree in degrees {
        let cents = plan.system.pitch(plan.tuning, degree).cents;
        if let Some(pitch) = track.place(cents, track.center()) {
            notes.push(note(
                section.start,
                section.end - section.start,
                pitch,
                PLAIN,
            ));
        }
    }
}

#[cfg(test)]
#[path = "chords_tests.rs"]
mod tests;
