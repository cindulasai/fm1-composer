//! Drums and percussion: the groove's patterns laid over a section, with a
//! fill at its end when the section asks for one.

use std::collections::BTreeMap;

use t2m_core::brief::Transition;
use t2m_core::groove::{Band, DrumPattern, Fill, FillSize, Groove, Hit, Lane};
use t2m_core::{Note, Tick, TimeFrame};

use crate::parts::{note, tidy};
use crate::plan::{Plan, PlanSection, PlanTrack};
use crate::seed::Rng;

const ACCENT: i64 = 112;
const NORMAL: i64 = 92;
const GHOST: i64 = 34;
/// Ghost notes are never louder than this.
const GHOST_CEILING: i64 = 50;
/// A part less busy than this leaves ghost notes out.
const GHOST_BUSY: f64 = 0.35;
const CRASH: i64 = 110;
/// Every how many bars the second pattern of a section is heard.
const VARIATION_EVERY: u16 = 4;

/// The kit pieces of a drum track, by name, as pitches (MIDI note times 100).
fn kit<'a>(track: &'a PlanTrack<'a>) -> BTreeMap<&'a str, i32> {
    track
        .instrument
        .kit
        .iter()
        .map(|piece| (piece.name.as_str(), i32::from(piece.note) * 100))
        .collect()
}

/// When each kick drum hit starts, for the bass to lock to.
pub(crate) fn kick_onsets(notes: &[Note], track: &PlanTrack) -> Vec<Tick> {
    let Some(kick) = kit(track).get("kick").copied() else {
        return Vec::new();
    };
    notes
        .iter()
        .filter(|note| note.pitch.cents == kick)
        .map(|note| note.start)
        .collect()
}

/// One section of a drum or percussion part.
pub(crate) fn drums(plan: &Plan, track: &PlanTrack, section: &PlanSection) -> Vec<Note> {
    let mut rng = plan.rng(track.id, &section.id);
    let kit = kit(track);
    let groove = plan.groove;
    let written_for_this_meter = matches!(
        &plan.frame,
        TimeFrame::Meter(meter) if (meter.numerator, meter.denominator) == groove.meter
    );
    let mut notes = if let TimeFrame::Cycle(cycle) = &plan.frame
        && is_timeline(cycle)
    {
        from_timeline(plan, cycle, section, &kit, &mut rng)
    } else if written_for_this_meter && !groove.patterns.is_empty() {
        from_patterns(plan, track, section, &kit, &mut rng)
    } else {
        from_meter(plan, section, &kit, &mut rng)
    };
    if matches!(
        section.entered_by,
        Transition::Crash | Transition::FillHeavy
    ) && let Some(crash) = kit.get("crash")
    {
        notes.push(note(section.start, plan.span / 4, *crash, CRASH));
    }
    if section.transition == Transition::Break {
        let last_beat = section.end - beat_ticks(plan);
        notes.retain(|note| note.start < last_beat);
    }
    tidy(notes, section)
}

/// The length of one beat: a quarter of a 4/4 bar, a group's unit otherwise.
pub(crate) fn beat_ticks(plan: &Plan) -> Tick {
    match &plan.frame {
        TimeFrame::Meter(meter) => plan.span / Tick::from(meter.numerator.max(1)),
        TimeFrame::Cycle(cycle) => plan.span / Tick::from(cycle.beats.max(1)),
    }
}

/// The patterns of one band, or of the nearest band that has any.
fn patterns_for(groove: &Groove, band: Band) -> Vec<&DrumPattern> {
    let distance = |pattern: &DrumPattern| (pattern.energy as i32 - band as i32).abs();
    let nearest = groove.patterns.iter().map(distance).min().unwrap_or(0);
    groove
        .patterns
        .iter()
        .filter(|pattern| distance(pattern) == nearest)
        .collect()
}

/// The fill for a section's last bar, if its transition asks for one.
///
/// `roll` is drawn whether or not a fill is wanted, so that asking for a fill
/// changes nothing else about the section.
fn fill_for(groove: &Groove, transition: Transition, roll: u64) -> Option<&Fill> {
    let size = match transition {
        Transition::FillLight => FillSize::Light,
        Transition::FillHeavy => FillSize::Heavy,
        _ => return None,
    };
    let sized: Vec<&Fill> = groove
        .fills
        .iter()
        .filter(|fill| fill.size == size)
        .collect();
    let any: Vec<&Fill> = groove.fills.iter().collect();
    let pool = if sized.is_empty() { any } else { sized };
    let count = u64::try_from(pool.len()).ok().filter(|count| *count > 0)?;
    pool.get((roll % count) as usize).copied()
}

fn from_patterns(
    plan: &Plan,
    track: &PlanTrack,
    section: &PlanSection,
    kit: &BTreeMap<&str, i32>,
    rng: &mut Rng,
) -> Vec<Note> {
    let groove = plan.groove;
    let choices = patterns_for(groove, Band::of(section.energy));
    let (Some(main), Some(variation)) = (rng.pick(&choices).copied(), rng.pick(&choices).copied())
    else {
        return Vec::new();
    };
    let fill = fill_for(groove, section.transition, rng.next_u64());
    let steps = usize::from(groove.steps.max(1));
    let shape = Shape::new(plan, track.busy);

    let mut notes = Vec::new();
    for bar in 0..section.bars {
        let bar_start = section.start + Tick::from(bar) * plan.span;
        let last = bar + 1 == section.bars;
        let pattern = if bar % VARIATION_EVERY == VARIATION_EVERY - 1 {
            variation
        } else {
            main
        };
        let fill_from = fill
            .filter(|_| last)
            .and_then(Fill::first_step)
            .unwrap_or(steps);
        for lane in &pattern.lanes {
            shape.lane(lane, 0..fill_from, bar_start, kit, rng, &mut notes);
        }
        if let Some(fill) = fill.filter(|_| last) {
            for lane in &fill.lanes {
                shape.lane(lane, fill_from..steps, bar_start, kit, rng, &mut notes);
            }
        }
    }
    notes
}

/// How a step on the grid becomes a time and a loudness.
struct Shape<'a> {
    groove: &'a Groove,
    step_ticks: Tick,
    /// How late the second half of each swung pair is played.
    swing_ticks: Tick,
    steps_per_beat: usize,
    ghosts: bool,
}

impl<'a> Shape<'a> {
    fn new(plan: &'a Plan, busy: f64) -> Shape<'a> {
        let groove = plan.groove;
        let steps = Tick::from(groove.steps.max(1));
        let step_ticks = plan.span / steps;
        // A swung pair of length P at ratio r:1 puts its second note at P*r/(r+1), not P/2.
        let pair = step_ticks * Tick::from(groove.swing_steps);
        let ratio = Tick::from(groove.swing_ratio(plan.tempo));
        let swing_ticks = pair * ratio / (ratio + 1000) - pair / 2;
        let beats = usize::from(groove.meter.0.max(1));
        Shape {
            groove,
            step_ticks,
            swing_ticks,
            steps_per_beat: (usize::from(groove.steps) / beats).max(1),
            ghosts: busy >= GHOST_BUSY,
        }
    }

    fn offset(&self, step: usize, lane: &Lane) -> Tick {
        let pair = usize::from(self.groove.swing_steps);
        let swung = step % pair == pair / 2;
        let swing = if swung { self.swing_ticks } else { 0 };
        let push = self.groove.timing.get(step).copied().unwrap_or(0);
        swing + Tick::from(push) + Tick::from(lane.late)
    }

    fn loudness(&self, step: usize, hit: Hit, rng: &mut Rng) -> i64 {
        let accent = i64::from(self.groove.accent.get(step).copied().unwrap_or(0));
        // Hits on the beat are a little stronger, hits between the half-beats a little weaker.
        let within = step % self.steps_per_beat;
        let metric = if within == 0 {
            4
        } else if within * 2 == self.steps_per_beat {
            0
        } else {
            -6
        };
        let human = rng.between(-3, 3);
        match hit {
            Hit::Accent => ACCENT + accent + metric + human,
            Hit::Normal => NORMAL + accent + metric + human,
            Hit::Ghost => (GHOST + accent + human).min(GHOST_CEILING),
        }
    }

    fn lane(
        &self,
        lane: &Lane,
        steps: std::ops::Range<usize>,
        bar_start: Tick,
        kit: &BTreeMap<&str, i32>,
        rng: &mut Rng,
        notes: &mut Vec<Note>,
    ) {
        let Some(piece) = kit.get(lane.piece.as_str()) else {
            return;
        };
        for (step, hit) in lane.steps().enumerate() {
            let Some(hit) = hit else { continue };
            // Drawn for every written hit, played or not, so that a fill or a
            // quieter part does not change the loudness of the hits around it.
            let vel = self.loudness(step, hit, rng);
            if !steps.contains(&step) || (hit == Hit::Ghost && !self.ghosts) {
                continue;
            }
            let start = bar_start + step as Tick * self.step_ticks + self.offset(step, lane);
            notes.push(note(start, (self.step_ticks / 2).max(1), *piece, vel));
        }
    }
}

/// A cycle whose strokes are pulses: `x` is an onset, `.` a rest (the clave and bell timelines).
fn is_timeline(cycle: &t2m_core::Cycle) -> bool {
    !cycle.strokes.is_empty()
        && cycle.strokes.iter().all(|s| s.name == "x" || s.name == ".")
        && cycle.strokes.iter().any(|s| s.name == "x")
}

/// The timeline itself on one piece (a bell on a cowbell, a clave on the side stick), the first
/// pulse of every cycle on the kick, and a soft hi-hat on every pulse.
fn from_timeline(
    plan: &Plan,
    cycle: &t2m_core::Cycle,
    section: &PlanSection,
    kit: &BTreeMap<&str, i32>,
    rng: &mut Rng,
) -> Vec<Note> {
    let pulse = beat_ticks(plan).max(1);
    let length = (pulse / 2).max(1);
    let voice = if cycle.id.contains("bell") {
        "cowbell"
    } else {
        "side_stick"
    };
    let mut notes = Vec::new();
    let mut hit = |at: Tick, piece: &str, vel: i64, rng: &mut Rng| {
        if let Some(pitch) = kit.get(piece) {
            notes.push(note(at, length, *pitch, vel + rng.between(-3, 3)));
        }
    };
    for bar in 0..section.bars {
        let bar_start = section.start + Tick::from(bar) * plan.span;
        hit(bar_start, "kick", ACCENT, rng);
        for (index, stroke) in cycle.strokes.iter().enumerate() {
            let at = bar_start + Tick::try_from(index).unwrap_or(0) * pulse;
            if stroke.name == "x" {
                hit(at, voice, ACCENT, rng);
            }
            hit(at, "closed_hat", NORMAL - 24, rng);
        }
    }
    notes
}

/// A plain beat for any meter or cycle the groove has no patterns for: kick on
/// the strong positions, snare on the other group starts, hi-hat on every beat.
fn from_meter(
    plan: &Plan,
    section: &PlanSection,
    kit: &BTreeMap<&str, i32>,
    rng: &mut Rng,
) -> Vec<Note> {
    let strong = plan.frame.strong_positions();
    let groups = match &plan.frame {
        TimeFrame::Meter(meter) => meter.accents(),
        TimeFrame::Cycle(_) => strong.clone(),
    };
    let beat = beat_ticks(plan).max(1);
    let length = (beat / 2).max(1);
    let mut notes = Vec::new();
    let mut hit = |at: Tick, piece: &str, vel: i64, rng: &mut Rng| {
        if let Some(pitch) = kit.get(piece) {
            notes.push(note(at, length, *pitch, vel + rng.between(-3, 3)));
        }
    };
    let fill = matches!(
        section.transition,
        Transition::FillLight | Transition::FillHeavy
    );
    let last_group = groups.last().copied().unwrap_or(0);
    for bar in 0..section.bars {
        let bar_start = section.start + Tick::from(bar) * plan.span;
        let filling = fill && bar + 1 == section.bars;
        for position in &groups {
            let piece = if strong.contains(position) {
                "kick"
            } else {
                "snare"
            };
            if !(filling && *position >= last_group) {
                hit(bar_start + position, piece, ACCENT, rng);
            }
        }
        let mut at = 0;
        while at < plan.span {
            if filling && at >= last_group {
                hit(bar_start + at, "snare", NORMAL + 8, rng);
            } else {
                let on_group = groups.contains(&at);
                hit(
                    bar_start + at,
                    "closed_hat",
                    if on_group { NORMAL } else { NORMAL - 14 },
                    rng,
                );
            }
            at += beat;
        }
    }
    notes
}

#[cfg(test)]
#[path = "drums_tests.rs"]
mod tests;
