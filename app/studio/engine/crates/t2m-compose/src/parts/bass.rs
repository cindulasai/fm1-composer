//! The bass: one note at a time, built on each chord's lowest tone and
//! locked to the kick drum where the style does that.

use t2m_core::brief::SectionHarmony;
use t2m_core::{Note, Tick};

use crate::harmony::ChordSpan;
use crate::parts::drums::beat_ticks;
use crate::parts::{note, single_line, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};
use crate::seed::Rng;

const OCTAVE: i32 = 1200;
const STRONG: i64 = 100;
const PLAIN: i64 = 86;
/// A bass that is less busy than this plays quarter notes where it would play eighths.
const SPARSE: f64 = 0.35;
/// Out of a thousand: how often a kick-locked note jumps an octave.
const OCTAVE_JUMP: u32 = 250;
/// Eighth-note positions of the syncopated patterns, one list per busyness step.
const SYNCOPATED: [&[i64]; 4] = [&[0, 3], &[0, 3, 6], &[0, 3, 4, 7], &[0, 2, 3, 5, 6]];

/// The bass patterns, chosen by the brief's `pattern` or else by the groove.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Family {
    RootHold,
    RootEighths,
    KickLock,
    Walking,
    OctaveBounce,
    Syncopated,
    TumbaoAnticipated,
}

impl Family {
    /// A family by name, with or without the `bass.` prefix.
    pub(crate) fn parse(name: &str) -> Option<Family> {
        let name = name.trim().strip_prefix("bass.").unwrap_or(name.trim());
        Some(match name {
            "root_hold" => Family::RootHold,
            "root_eighths" => Family::RootEighths,
            "kick_lock" => Family::KickLock,
            "walking" => Family::Walking,
            "octave_bounce" => Family::OctaveBounce,
            "syncopated" => Family::Syncopated,
            "tumbao_anticipated" => Family::TumbaoAnticipated,
            _ => return None,
        })
    }

    /// What the groove's style usually plays.
    pub(crate) fn for_groove(groove: &str) -> Family {
        match groove {
            "groove.house" | "groove.techno" => Family::OctaveBounce,
            "groove.trap" | "groove.lofi_swing" => Family::RootHold,
            "groove.boom_bap" => Family::KickLock,
            "groove.jazz_ride" => Family::Walking,
            "groove.funk_16" | "groove.one_drop" | "groove.afrobeats" => Family::Syncopated,
            "groove.bossa" => Family::TumbaoAnticipated,
            _ => Family::RootEighths,
        }
    }
}

/// One section of bass, over the section's chords and the kick drum's onsets.
pub(crate) fn bass(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    chords: &[ChordSpan],
    kicks: &[Tick],
) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let family =
        Family::parse(track.pattern).unwrap_or_else(|| Family::for_groove(&plan.groove.id));
    let mut line = Line::new(track);
    if chords.is_empty() {
        hold_drone(plan, section, &mut line);
    } else {
        let beat = beat_ticks(plan);
        for (index, span) in chords.iter().enumerate() {
            let next = chords.get(index + 1);
            let shape = Shape {
                span,
                next,
                beat,
                eighth: (beat / 2).max(1),
                bar: plan.span,
                busy: track.busy,
                first: index == 0,
            };
            match family {
                Family::RootHold => root_hold(&shape, &mut line),
                Family::RootEighths => root_eighths(&shape, &mut line),
                Family::KickLock => kick_lock(&shape, kicks, &mut rng, &mut line),
                Family::Walking => walking(&shape, &mut rng, &mut line),
                Family::OctaveBounce => octave_bounce(&shape, &mut line),
                Family::Syncopated => syncopated(&shape, &mut rng, &mut line),
                Family::TumbaoAnticipated => tumbao(&shape, &mut line),
            }
        }
    }
    tidy(single_line(line.notes), section)
}

/// The notes so far, and where the line last was, so that each note takes
/// the octave nearest to the one before it.
struct Line<'a> {
    track: &'a PlanTrack<'a>,
    notes: Vec<Note>,
    last: i32,
}

impl<'a> Line<'a> {
    fn new(track: &'a PlanTrack<'a>) -> Line<'a> {
        Line {
            track,
            notes: Vec::new(),
            last: track.low + (track.high - track.low) / 3,
        }
    }

    /// A note of pitch class `cents`, in the octave nearest the last note.
    fn play(&mut self, start: Tick, until: Tick, cents: i32, vel: i64) {
        if until <= start {
            return;
        }
        if let Some(pitch) = self.track.place(cents, self.last) {
            self.push(start, until, pitch, vel);
        }
    }

    /// A note at an exact pitch, if the instrument can play it.
    fn play_exact(&mut self, start: Tick, until: Tick, pitch: i32, vel: i64) {
        if until > start && pitch >= self.track.low && pitch <= self.track.high {
            self.push(start, until, pitch, vel);
        }
    }

    fn push(&mut self, start: Tick, until: Tick, pitch: i32, vel: i64) {
        self.notes.push(note(start, until - start, pitch, vel));
        self.last = pitch;
    }
}

struct Shape<'a> {
    span: &'a ChordSpan,
    next: Option<&'a ChordSpan>,
    beat: Tick,
    eighth: Tick,
    bar: Tick,
    busy: f64,
    first: bool,
}

impl Shape<'_> {
    fn root(&self) -> i32 {
        self.span.chord.bass.cents
    }

    fn bars(&self) -> impl Iterator<Item = Tick> {
        let bars = (self.span.end - self.span.start + self.bar - 1) / self.bar;
        let (start, bar) = (self.span.start, self.bar);
        (0..bars.max(1)).map(move |index| start + index * bar)
    }
}

/// Where a drone section has no chords the bass holds the first drone degree.
fn hold_drone(plan: &Plan, section: &PlanSection, line: &mut Line) {
    if let SectionHarmony::Drone { degrees } = &section.harmony
        && let Some(degree) = degrees.first()
    {
        let pitch = plan.system.pitch(plan.tuning, i32::from(*degree) - 1);
        line.play(section.start, section.end, pitch.cents, PLAIN);
    }
}

fn root_hold(shape: &Shape, line: &mut Line) {
    line.play(shape.span.start, shape.span.end, shape.root(), STRONG);
}

fn root_eighths(shape: &Shape, line: &mut Line) {
    let step = if shape.busy < SPARSE {
        shape.beat
    } else {
        shape.eighth
    };
    let mut at = shape.span.start;
    while at < shape.span.end {
        let vel = if (at - shape.span.start) % shape.bar == 0 {
            STRONG
        } else {
            PLAIN
        };
        line.play(at, (at + step).min(shape.span.end), shape.root(), vel);
        at += step;
    }
}

fn kick_lock(shape: &Shape, kicks: &[Tick], rng: &mut Rng, line: &mut Line) {
    let mut onsets: Vec<Tick> = kicks
        .iter()
        .copied()
        .filter(|kick| *kick >= shape.span.start && *kick < shape.span.end)
        .collect();
    if onsets.first() != Some(&shape.span.start) {
        onsets.insert(0, shape.span.start);
    }
    for (index, at) in onsets.iter().enumerate() {
        let until = onsets.get(index + 1).copied().unwrap_or(shape.span.end);
        let jump = index > 0 && rng.chance(OCTAVE_JUMP);
        let vel = if index == 0 { STRONG } else { PLAIN };
        line.play(*at, until, shape.root(), vel);
        if jump && let Some(last) = line.notes.last().map(|n| n.pitch.cents) {
            let up = last + OCTAVE;
            if up <= line.track.high {
                line.notes.pop();
                line.play_exact(*at, until, up, vel);
            }
        }
    }
}

fn walking(shape: &Shape, rng: &mut Rng, line: &mut Line) {
    let beats = ((shape.span.end - shape.span.start + shape.beat - 1) / shape.beat).max(1);
    let tones: Vec<i32> = shape.span.chord.tones.iter().map(|t| t.cents).collect();
    for index in 0..beats {
        let at = shape.span.start + index * shape.beat;
        let until = (at + shape.beat).min(shape.span.end);
        let last_of_chord = index == beats - 1;
        let vel = if index == 0 { STRONG } else { PLAIN };
        if index == 0 {
            line.play(at, until, shape.root(), vel);
        } else if last_of_chord {
            let target_class = shape
                .next
                .map_or(shape.root(), |next| next.chord.bass.cents);
            approach(line, at, until, target_class, rng, vel);
        } else {
            let pick = tones.get(rng.below(tones.len() as u32) as usize).copied();
            line.play(at, until, pick.unwrap_or(shape.root()), vel);
        }
    }
}

/// A note a half step or a whole step from the next root.
fn approach(line: &mut Line, at: Tick, until: Tick, target: i32, rng: &mut Rng, vel: i64) {
    let Some(goal) = line.track.place(target, line.last) else {
        return;
    };
    let steps = [-100, 100, -200];
    let first = rng.below(3) as usize;
    for offset in 0..3 {
        let step = steps[(first + offset) % 3];
        if goal + step >= line.track.low && goal + step <= line.track.high {
            line.play_exact(at, until, goal + step, vel);
            return;
        }
    }
    line.play_exact(at, until, goal, vel);
}

fn octave_bounce(shape: &Shape, line: &mut Line) {
    let mut at = shape.span.start;
    let mut high = false;
    while at < shape.span.end {
        let until = (at + shape.eighth).min(shape.span.end);
        let vel = if high { PLAIN } else { STRONG };
        line.play(at, until, shape.root(), vel);
        if high && let Some(last) = line.notes.last().map(|n| n.pitch.cents) {
            let up = last + OCTAVE;
            if up <= line.track.high {
                line.notes.pop();
                line.play_exact(at, until, up, vel);
            }
        }
        high = !high;
        at += shape.eighth;
    }
}

fn syncopated(shape: &Shape, rng: &mut Rng, line: &mut Line) {
    let eighths_per_bar = (shape.bar / shape.eighth).max(1);
    let level = ((shape.busy.clamp(0.0, 1.0) * 3.99) as usize).min(3);
    let fifth = shape.span.chord.tones.get(2).map(|t| t.cents);
    for bar_start in shape.bars() {
        let positions: Vec<i64> = SYNCOPATED[level]
            .iter()
            .copied()
            .filter(|p| *p < eighths_per_bar)
            .collect();
        for (index, position) in positions.iter().enumerate() {
            let at = bar_start + position * shape.eighth;
            if at >= shape.span.end {
                break;
            }
            let end_of_bar = bar_start + shape.bar;
            let until = positions
                .get(index + 1)
                .map_or(end_of_bar, |p| bar_start + p * shape.eighth)
                .min(shape.span.end);
            let use_fifth = index > 0 && rng.chance(200);
            let class = if use_fifth {
                fifth.unwrap_or(shape.root())
            } else {
                shape.root()
            };
            let vel = if index == 0 { STRONG } else { PLAIN };
            line.play(at, until, class, vel);
        }
    }
}

/// The root arrives one eighth early, on the last eighth of the bar before.
fn tumbao(shape: &Shape, line: &mut Line) {
    let eighths_per_bar = (shape.bar / shape.eighth).max(1);
    for bar_start in shape.bars() {
        let this_bar_end = bar_start + shape.bar;
        let last_bar_of_span = this_bar_end >= shape.span.end;
        let anticipated = bar_start == shape.span.start && !shape.first;
        if !anticipated {
            // Nothing sounded the root early, so it is struck on the first beat.
            let until = bar_start + 3.min(eighths_per_bar - 1).max(1) * shape.eighth;
            line.play(bar_start, until.min(shape.span.end), shape.root(), STRONG);
        }
        if eighths_per_bar > 3 {
            let at = bar_start + 3 * shape.eighth;
            let until = (this_bar_end - shape.eighth).min(shape.span.end);
            line.play(at, until, shape.root(), PLAIN);
        }
        // The last eighth of the bar belongs to the chord that follows.
        let early = this_bar_end - shape.eighth;
        let target = if last_bar_of_span {
            shape.next.map(|next| next.chord.bass.cents)
        } else {
            Some(shape.root())
        };
        if let Some(class) = target
            && early >= bar_start
            && early < shape.span.end
        {
            // Held through the downbeat of the new chord, up to its own first hit.
            let until = if last_bar_of_span {
                this_bar_end + 3 * shape.eighth
            } else {
                this_bar_end
            };
            line.play(early, until, class, STRONG);
        }
    }
}

#[cfg(test)]
#[path = "bass_tests.rs"]
mod tests;
