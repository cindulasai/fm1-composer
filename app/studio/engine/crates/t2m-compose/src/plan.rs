//! The resolved plan: a brief with every id looked up, every length in ticks,
//! and every register in cents. No randomness happens here.

use std::collections::BTreeSet;

use t2m_core::brief::{
    Articulation, Brief, HarmonicLanguage, Length, MeterSpec, Motif, Register, Role,
    SectionHarmony, TonicSpec, TrackSpec, Transition,
};
use t2m_core::{
    Groove, Instrument, Knowledge, Meter, Style, TICKS_PER_QUARTER, Tick, TimeFrame, TonalSystem,
    Tonic, Tuning, parse_pitch_class,
};

use crate::seed::{Rerolls, Rng, sub_rng};

const MAX_BARS: u32 = 512;
const CENTS_PER_SEMITONE: i32 = 100;

pub(crate) struct Plan<'a> {
    pub frame: TimeFrame,
    /// Ticks in one bar or cycle.
    pub span: Tick,
    pub tempo: u16,
    pub system: &'a TonalSystem,
    pub tuning: &'a Tuning,
    pub tonic: Tonic,
    /// MIDI note of the tonic in octave 0; pitches are cents above it.
    pub tonic_midi: i32,
    /// The brief's style entry (read by tests; the parts take what they need from the brief).
    #[cfg_attr(not(test), allow(dead_code))]
    pub style: &'a Style,
    pub groove: &'a Groove,
    pub language: HarmonicLanguage,
    pub motif: Option<&'a Motif>,
    pub sections: Vec<PlanSection>,
    pub tracks: Vec<PlanTrack<'a>>,
    pub seed: u64,
    pub rerolls: &'a Rerolls,
}

pub(crate) struct PlanSection {
    pub id: String,
    pub start: Tick,
    pub end: Tick,
    pub bars: u16,
    pub energy: f64,
    pub density: f64,
    /// Named progressions are already looked up; this is never `Named`.
    pub harmony: SectionHarmony,
    pub active: BTreeSet<String>,
    /// What happens at the end of this section.
    pub transition: Transition,
    /// What the section before this one ended with.
    pub entered_by: Transition,
}

pub(crate) struct PlanTrack<'a> {
    pub id: &'a str,
    pub role: Role,
    pub instrument: &'a Instrument,
    /// Lowest and highest pitch the part may use, in cents above the tonic at octave 0.
    pub low: i32,
    pub high: i32,
    pub busy: f64,
    pub pattern: &'a str,
    pub articulation: Articulation,
}

impl Plan<'_> {
    /// The random stream for one part of one section.
    pub(crate) fn rng(&self, track: &str, section: &str) -> Rng {
        sub_rng(
            self.seed,
            track,
            section,
            self.rerolls.count(track, section),
        )
    }

    /// A second stream for one part of one section, for work beyond the notes themselves.
    /// A reroll of the part changes it too.
    pub(crate) fn rng_labelled(&self, track: &str, section: &str, label: &str) -> Rng {
        let reroll = self.rerolls.count(track, section);
        sub_rng(self.seed, track, &format!("{label}:{section}"), reroll)
    }

    /// Cents above the tonic at octave 0 for a MIDI note.
    pub(crate) fn cents_of_midi(&self, midi: i32) -> i32 {
        (midi - self.tonic_midi) * CENTS_PER_SEMITONE
    }

    #[cfg(test)]
    pub(crate) fn track(&self, id: &str) -> Option<&PlanTrack<'_>> {
        self.tracks.iter().find(|track| track.id == id)
    }
}

impl PlanTrack<'_> {
    /// The octave of `cents` nearest to `near` that the part can play, if any.
    pub(crate) fn place(&self, cents: i32, near: i32) -> Option<i32> {
        let class = cents.rem_euclid(1200);
        let first = self.low + (class - self.low).rem_euclid(1200);
        let mut best: Option<i32> = None;
        let mut candidate = first;
        while candidate <= self.high {
            let closer = best.is_none_or(|found| (candidate - near).abs() < (found - near).abs());
            if closer {
                best = Some(candidate);
            }
            candidate += 1200;
        }
        best
    }

    /// The middle of the part's range.
    pub(crate) fn center(&self) -> i32 {
        self.low + (self.high - self.low) / 2
    }
}

/// The usual range of a role, as MIDI notes.
fn role_register(role: Role) -> (i32, i32) {
    match role {
        Role::Bass => (28, 52),
        Role::Chords | Role::Pad => (48, 76),
        Role::Lead => (60, 88),
        Role::Counter => (55, 79),
        Role::Arp | Role::Fx => (60, 96),
        Role::Drone => (36, 60),
        Role::Drums | Role::Percussion => (0, 127),
    }
}

/// How far a brief's register setting moves a role's usual range, in semitones.
fn register_shift(register: Register) -> i32 {
    match register {
        Register::Low => -12,
        Register::MidLow => -5,
        Register::Mid => 0,
        Register::MidHigh => 5,
        Register::High => 12,
    }
}

fn plan_track<'a>(
    spec: &'a TrackSpec,
    instrument: &'a Instrument,
    tonic_midi: i32,
) -> PlanTrack<'a> {
    let (role_low, role_high) = role_register(spec.role);
    let shift = register_shift(spec.register);
    let (low, high) = (i32::from(instrument.low), i32::from(instrument.high));
    let mut range = ((role_low + shift).max(low), (role_high + shift).min(high));
    if range.0 > range.1 {
        // The role's range misses the instrument entirely: use what the instrument has.
        range = (low, high);
    }
    PlanTrack {
        id: &spec.id,
        role: spec.role,
        instrument,
        low: (range.0 - tonic_midi) * CENTS_PER_SEMITONE,
        high: (range.1 - tonic_midi) * CENTS_PER_SEMITONE,
        busy: spec.busy,
        pattern: &spec.pattern,
        articulation: spec.articulation,
    }
}

/// Shares `total` bars among sections in proportion to their weights.
///
/// Every section gets at least one bar whenever `total` allows. When `total`
/// equals the sum of the weights, each section gets exactly its weight.
pub(crate) fn allocate(weights: &[u16], total: u32) -> Vec<u16> {
    let sum: u64 = weights.iter().map(|weight| u64::from(*weight)).sum();
    if sum == 0 {
        return vec![0; weights.len()];
    }
    let total = u64::from(total);
    let mut bars: Vec<u64> = weights
        .iter()
        .map(|w| total * u64::from(*w) / sum)
        .collect();
    // Hand out what flooring left over, largest remainder first, earliest on a tie.
    let mut order: Vec<usize> = (0..weights.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(total * u64::from(weights[i]) % sum));
    let handed: u64 = bars.iter().sum();
    for &i in order.iter().take((total - handed) as usize) {
        bars[i] += 1;
    }
    // No section may be empty: take a bar from the longest for each one that is.
    while let Some(empty) = bars.iter().position(|count| *count == 0) {
        let Some((longest, _)) = bars
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 1)
            .max_by_key(|(i, count)| (**count, std::cmp::Reverse(*i)))
        else {
            break;
        };
        bars[longest] -= 1;
        bars[empty] += 1;
    }
    bars.into_iter()
        .map(|count| u16::try_from(count).unwrap_or(u16::MAX))
        .collect()
}

/// How many bars (or cycles) the whole piece lasts.
fn total_bars(brief: &Brief, span: Tick) -> u32 {
    let sections = u32::try_from(brief.form.len()).unwrap_or(MAX_BARS);
    let wanted = match brief.time.length {
        Length::Bars(count) | Length::Cycles(count) => u32::from(count),
        Length::Seconds(seconds) => {
            let ticks =
                Tick::from(seconds) * Tick::from(brief.time.tempo_bpm) * TICKS_PER_QUARTER / 60;
            // Rounded to the nearest whole bar.
            u32::try_from((ticks + span / 2) / span.max(1)).unwrap_or(MAX_BARS)
        }
    };
    wanted.clamp(sections.min(MAX_BARS), MAX_BARS)
}

/// Looks up everything a brief names. `None` only for a brief that is not valid.
pub(crate) fn resolve<'a>(
    brief: &'a Brief,
    kb: &'a Knowledge,
    seed: u64,
    rerolls: &'a Rerolls,
) -> Option<Plan<'a>> {
    let style = kb.style(&brief.identity.style)?;
    let groove = kb.groove(&brief.time.feel)?;
    let system = kb.tonal_system(&brief.tonality.system)?;
    let tuning = kb.tuning(&brief.tonality.tuning)?;
    let frame = match &brief.time.meter {
        MeterSpec::Meter {
            numerator,
            denominator,
            grouping,
        } => TimeFrame::Meter(Meter::new(*numerator, *denominator, grouping.clone()).ok()?),
        MeterSpec::Cycle { id } => TimeFrame::Cycle(kb.cycle(id)?.clone()),
    };
    let span = frame.span_ticks();
    let tonic = match &brief.tonality.tonic {
        TonicSpec::Name(name) => Tonic::Class(parse_pitch_class(name)?),
        TonicSpec::Hz { hz } => Tonic::Hz(*hz),
    };
    let tonic_midi = i32::from(tonic.midi_and_offset(0).0);

    let mut tracks = Vec::with_capacity(brief.tracks.len());
    for spec in &brief.tracks {
        tracks.push(plan_track(
            spec,
            kb.instrument(&spec.instrument)?,
            tonic_midi,
        ));
    }

    let weights: Vec<u16> = brief.form.iter().map(|section| section.bars).collect();
    let bars = allocate(&weights, total_bars(brief, span));
    let mut sections = Vec::with_capacity(brief.form.len());
    let mut start: Tick = 0;
    let mut entered_by = Transition::None;
    for (section, bars) in brief.form.iter().zip(bars) {
        let end = start + Tick::from(bars) * span;
        let harmony = match &section.harmony {
            SectionHarmony::Named { id } => {
                let progression = kb.progression(id)?;
                SectionHarmony::Progression {
                    roman: progression.roman.clone(),
                    bars_per_chord: progression.bars_per_chord,
                }
            }
            other => other.clone(),
        };
        sections.push(PlanSection {
            id: section.id.clone(),
            start,
            end,
            bars,
            energy: section.energy,
            density: section.density,
            harmony,
            active: section.tracks.iter().cloned().collect(),
            transition: section.transition,
            entered_by,
        });
        start = end;
        entered_by = section.transition;
    }

    Some(Plan {
        frame,
        span,
        tempo: brief.time.tempo_bpm,
        system,
        tuning,
        tonic,
        tonic_midi,
        style,
        groove,
        language: brief.tonality.harmonic_language,
        motif: brief.motif.as_ref(),
        sections,
        tracks,
        seed,
        rerolls,
    })
}

#[cfg(test)]
#[path = "plan_tests.rs"]
mod tests;
