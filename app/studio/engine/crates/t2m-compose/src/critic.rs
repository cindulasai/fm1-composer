//! The critic: measures a finished score against the brief and the theory it
//! was meant to follow, and mends what it can.

use t2m_core::brief::{Brief, HarmonicLanguage, Role};
use t2m_core::{Knowledge, Note, Pitch, Score, Tick, repair};

use crate::harmony::{ChordSpan, chord_at, chord_timeline};
use crate::plan::{Plan, resolve};
use crate::seed::Rerolls;

/// A pitch this close to a scale degree or chord tone counts as being on it, in cents.
const TOLERANCE: i32 = 30;
const OCTAVE: i32 = 1200;
/// Most of a lead's notes that may sound in unison with another part. Some doubling
/// of a chord tone is ordinary; measured over 40 starter pieces the most is 11%.
const UNISON_CAP: f64 = 0.15;
/// Smallest change in loudness from one section to the next, as a share of 127.
const CONTRAST_MIN: f64 = 0.03;
/// Energies closer than this count as the same level.
const ENERGY_SPREAD: f64 = 0.2;
const AGREEMENT_MIN: f64 = 0.85;
/// How many times a part is mended before it is accepted as it is.
pub(crate) const PASSES: usize = 3;

/// What a metric's value must be to pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Gate {
    AtLeast(f64),
    AtMost(f64),
    Exactly(f64),
    /// Reported, never failed.
    Info,
}

impl Gate {
    fn admits(self, value: f64) -> bool {
        match self {
            Gate::AtLeast(least) => value >= least,
            Gate::AtMost(most) => value <= most,
            Gate::Exactly(exact) => (value - exact).abs() < 1e-9,
            Gate::Info => true,
        }
    }
}

/// One measurement.
#[derive(Debug, Clone, PartialEq)]
pub struct Metric {
    pub id: &'static str,
    pub value: f64,
    pub gate: Gate,
    pub passed: bool,
}

/// Every measurement of a score, and the ids of the gates it failed.
#[derive(Debug, Clone, PartialEq)]
pub struct Scorecard {
    pub metrics: Vec<Metric>,
    pub hard_failures: Vec<String>,
}

impl Scorecard {
    /// A metric by id.
    pub fn get(&self, id: &str) -> Option<&Metric> {
        self.metrics.iter().find(|metric| metric.id == id)
    }
}

/// Measures a score against the brief it was composed from.
pub fn critique(score: &Score, brief: &Brief, kb: &Knowledge) -> Scorecard {
    let (brief, _) = repair(brief.clone(), kb);
    let rerolls = Rerolls::default();
    let Some(plan) = resolve(&brief, kb, score.meta.seed, &rerolls) else {
        return card(vec![
            measure("playability", playability(score), Gate::Exactly(1.0)),
            measure("brief_adherence", 0.0, Gate::Exactly(1.0)),
        ]);
    };
    let timeline = chord_timeline(&plan);
    measure_all(score, &brief, &plan, &timeline)
}

pub(crate) fn measure_all(
    score: &Score,
    brief: &Brief,
    plan: &Plan,
    timeline: &[Vec<ChordSpan>],
) -> Scorecard {
    let ctx = Ctx {
        score,
        plan,
        timeline,
    };
    let spread = energy_spread(brief) >= ENERGY_SPREAD;
    card(vec![
        measure(
            "harmonic_agreement",
            ctx.harmonic_agreement(),
            Gate::AtLeast(AGREEMENT_MIN),
        ),
        measure(
            "out_of_system",
            ctx.out_of_system().len() as f64,
            Gate::Exactly(0.0),
        ),
        measure(
            "cycle_integrity",
            cycle_integrity(score, plan),
            Gate::Exactly(1.0),
        ),
        measure(
            "register_overlap",
            ctx.unison_share(),
            Gate::AtMost(UNISON_CAP),
        ),
        measure(
            "form_contrast",
            if spread { ctx.form_contrast() } else { 1.0 },
            Gate::AtLeast(CONTRAST_MIN),
        ),
        measure("motif_recurrence", ctx.motif_recurrence(), Gate::Info),
        measure("groove_realism", groove_realism(score), Gate::Exactly(1.0)),
        measure(
            "brief_adherence",
            brief_adherence(score, brief),
            Gate::Exactly(1.0),
        ),
        measure("playability", playability(score), Gate::Exactly(1.0)),
    ])
}

fn measure(id: &'static str, value: f64, gate: Gate) -> Metric {
    Metric {
        id,
        value,
        gate,
        passed: gate.admits(value),
    }
}

fn card(metrics: Vec<Metric>) -> Scorecard {
    let hard_failures = metrics
        .iter()
        .filter(|metric| !metric.passed)
        .map(|metric| metric.id.to_string())
        .collect();
    Scorecard {
        metrics,
        hard_failures,
    }
}

fn energy_spread(brief: &Brief) -> f64 {
    let energies = brief.form.iter().map(|section| section.energy);
    let high = energies.clone().fold(f64::MIN, f64::max);
    let low = energies.fold(f64::MAX, f64::min);
    if brief.form.is_empty() {
        0.0
    } else {
        high - low
    }
}

fn pitched(role: Role) -> bool {
    !matches!(role, Role::Drums | Role::Percussion | Role::Fx)
}

struct Ctx<'a> {
    score: &'a Score,
    plan: &'a Plan<'a>,
    timeline: &'a [Vec<ChordSpan>],
}

impl Ctx<'_> {
    fn section_of(&self, tick: Tick) -> Option<usize> {
        self.score
            .sections
            .iter()
            .position(|section| section.start <= tick && tick < section.end)
    }

    /// The pitch classes of the chord at a tick, if there is one.
    fn chord_classes(&self, tick: Tick) -> Option<Vec<i32>> {
        let spans = self.timeline.get(self.section_of(tick)?)?;
        chord_at(spans, tick).map(|span| {
            span.chord
                .tones
                .iter()
                .map(|tone| tone.cents.rem_euclid(OCTAVE))
                .collect()
        })
    }

    fn strong(&self, tick: Tick) -> bool {
        let in_bar = tick.rem_euclid(self.plan.span.max(1));
        self.plan.frame.strong_positions().contains(&in_bar)
    }

    /// Share of lead notes on strong beats that are chord tones, where there are chords.
    fn harmonic_agreement(&self) -> f64 {
        if !matches!(
            self.plan.language,
            HarmonicLanguage::Functional | HarmonicLanguage::Modal
        ) {
            return 1.0;
        }
        let (mut good, mut total) = (0usize, 0usize);
        for track in self.score.tracks.iter().filter(|t| t.role == Role::Lead) {
            for note in track.notes.iter().filter(|n| self.strong(n.start)) {
                if let Some(classes) = self.chord_classes(note.start) {
                    total += 1;
                    good += usize::from(near_class(note.pitch.cents, &classes));
                }
            }
        }
        if total == 0 {
            1.0
        } else {
            good as f64 / total as f64
        }
    }

    /// (track, note) of every pitched note that is neither in the system nor in its chord.
    fn out_of_system(&self) -> Vec<(usize, usize)> {
        let mut found = Vec::new();
        for (ti, track) in self.score.tracks.iter().enumerate() {
            if !pitched(track.role) {
                continue;
            }
            for (ni, note) in track.notes.iter().enumerate() {
                if !self.in_system(note) {
                    found.push((ti, ni));
                }
            }
        }
        found
    }

    fn in_system(&self, note: &Note) -> bool {
        let system = self.plan.system;
        let tuning = self.plan.tuning;
        let degree = system.nearest_degree(tuning, note.pitch);
        let on_scale = (system.pitch(tuning, degree).cents - note.pitch.cents).abs() <= TOLERANCE;
        on_scale
            || self
                .chord_classes(note.start)
                .is_some_and(|classes| near_class(note.pitch.cents, &classes))
    }

    /// Share of lead notes that sound at the very pitch of another pitched part.
    fn unison_share(&self) -> f64 {
        let leads: Vec<&Note> = self
            .score
            .tracks
            .iter()
            .filter(|t| t.role == Role::Lead)
            .flat_map(|t| &t.notes)
            .collect();
        if leads.is_empty() {
            return 0.0;
        }
        let others: Vec<&Note> = self
            .score
            .tracks
            .iter()
            .filter(|t| pitched(t.role) && t.role != Role::Lead)
            .flat_map(|t| &t.notes)
            .collect();
        let doubled = leads
            .iter()
            .filter(|lead| {
                others
                    .iter()
                    .any(|o| o.pitch == lead.pitch && o.start <= lead.start && lead.start < o.end())
            })
            .count();
        doubled as f64 / leads.len() as f64
    }

    /// (track, note) of every note of another pitched part that sounds at a lead note's pitch.
    fn doublings(&self) -> Vec<(usize, usize)> {
        let leads: Vec<&Note> = self
            .score
            .tracks
            .iter()
            .filter(|t| t.role == Role::Lead)
            .flat_map(|t| &t.notes)
            .collect();
        let mut found = Vec::new();
        for (ti, track) in self.score.tracks.iter().enumerate() {
            if !pitched(track.role) || track.role == Role::Lead {
                continue;
            }
            for (ni, note) in track.notes.iter().enumerate() {
                let doubled = leads.iter().any(|lead| {
                    lead.pitch == note.pitch && note.start <= lead.start && lead.start < note.end()
                });
                if doubled {
                    found.push((ti, ni));
                }
            }
        }
        found
    }

    /// The biggest change in average loudness between neighbouring sections, out of 127.
    fn form_contrast(&self) -> f64 {
        let means: Vec<f64> = self
            .score
            .sections
            .iter()
            .filter_map(|section| {
                let velocities: Vec<f64> = self
                    .score
                    .tracks
                    .iter()
                    .flat_map(|t| &t.notes)
                    .filter(|n| n.start >= section.start && n.start < section.end)
                    .map(|n| f64::from(n.vel))
                    .collect();
                (!velocities.is_empty())
                    .then(|| velocities.iter().sum::<f64>() / velocities.len() as f64)
            })
            .collect();
        means
            .windows(2)
            .map(|pair| (pair[0] - pair[1]).abs() / 127.0)
            .fold(0.0, f64::max)
    }

    /// Share of lead sections that open the way the first one does.
    fn motif_recurrence(&self) -> f64 {
        let mut openings: Vec<(Tick, Tick)> = Vec::new();
        for track in self.score.tracks.iter().filter(|t| t.role == Role::Lead) {
            for section in &self.score.sections {
                if let Some(first) = track
                    .notes
                    .iter()
                    .find(|n| n.start >= section.start && n.start < section.end)
                {
                    openings.push((first.start - section.start, first.dur));
                }
            }
        }
        match openings.first() {
            None => 1.0,
            Some(first) => {
                openings.iter().filter(|o| *o == first).count() as f64 / openings.len() as f64
            }
        }
    }
}

/// Whether a pitch is within tolerance of any of the pitch classes.
fn near_class(cents: i32, classes: &[i32]) -> bool {
    let class = cents.rem_euclid(OCTAVE);
    classes.iter().any(|c| {
        let apart = (class - c).rem_euclid(OCTAVE);
        apart.min(OCTAVE - apart) <= TOLERANCE
    })
}

/// Share of sections that are a whole number of bars or cycles long.
fn cycle_integrity(score: &Score, plan: &Plan) -> f64 {
    if score.sections.is_empty() {
        return 1.0;
    }
    let whole = score
        .sections
        .iter()
        .filter(|s| (s.end - s.start) % plan.span.max(1) == 0)
        .count();
    whole as f64 / score.sections.len() as f64
}

/// Share of busy parts (four notes or more in a section) whose loudness is not flat.
fn groove_realism(score: &Score) -> f64 {
    let (mut varied, mut total) = (0usize, 0usize);
    for track in &score.tracks {
        for section in &score.sections {
            let velocities: Vec<u8> = track
                .notes
                .iter()
                .filter(|n| n.start >= section.start && n.start < section.end)
                .map(|n| n.vel)
                .collect();
            if velocities.len() >= 4 {
                total += 1;
                varied += usize::from(velocities.iter().any(|v| *v != velocities[0]));
            }
        }
    }
    if total == 0 {
        1.0
    } else {
        varied as f64 / total as f64
    }
}

/// 1 when the score has the brief's tracks and sections, in order, with its roles and instruments.
fn brief_adherence(score: &Score, brief: &Brief) -> f64 {
    let tracks = score.tracks.len() == brief.tracks.len()
        && score.tracks.iter().zip(&brief.tracks).all(|(t, spec)| {
            t.id == spec.id && t.role == spec.role && t.instrument == spec.instrument
        });
    let sections = score.sections.len() == brief.form.len()
        && score
            .sections
            .iter()
            .zip(&brief.form)
            .all(|(s, spec)| s.id == spec.id);
    f64::from(u8::from(tracks && sections))
}

fn playability(score: &Score) -> f64 {
    f64::from(u8::from(score.check().is_ok()))
}

/// Mends the score in place: out-of-system notes are moved onto the system or
/// their chord, and lead notes on strong beats onto chord tones. Each pass
/// measures first; after `PASSES` passes what is left is accepted.
pub(crate) fn polish(score: &mut Score, brief: &Brief, plan: &Plan, timeline: &[Vec<ChordSpan>]) {
    for _ in 0..PASSES {
        let card = measure_all(score, brief, plan, timeline);
        let needs = |id: &str| card.hard_failures.iter().any(|f| f == id);
        if !needs("out_of_system") && !needs("harmonic_agreement") && !needs("register_overlap") {
            return;
        }
        let ctx = Ctx {
            score,
            plan,
            timeline,
        };
        let mut moves: Vec<(usize, usize, i32)> = Vec::new();
        for (ti, ni) in ctx.out_of_system() {
            let note = &score.tracks[ti].notes[ni];
            moves.push((ti, ni, mend(&ctx, plan, ti, note)));
        }
        for (ti, track) in score
            .tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| t.role == Role::Lead)
        {
            for (ni, note) in track.notes.iter().enumerate() {
                if ctx.strong(note.start)
                    && let Some(classes) = ctx.chord_classes(note.start)
                    && !near_class(note.pitch.cents, &classes)
                {
                    moves.push((ti, ni, nearest_in(&classes, note.pitch.cents)));
                }
            }
        }
        let doubled = if needs("register_overlap") {
            ctx.doublings()
        } else {
            Vec::new()
        };
        for (ti, ni, cents) in moves {
            let range = (plan.tracks[ti].low, plan.tracks[ti].high);
            score.tracks[ti].notes[ni].pitch = Pitch {
                cents: fit(cents, range),
            };
        }
        // A part sounding the lead's own pitch steps out of the way, an octave down if it can.
        for (ti, ni) in doubled {
            let (low, high) = (plan.tracks[ti].low, plan.tracks[ti].high);
            let note = &mut score.tracks[ti].notes[ni];
            if note.pitch.cents - OCTAVE >= low {
                note.pitch.cents -= OCTAVE;
            } else if note.pitch.cents + OCTAVE <= high {
                note.pitch.cents += OCTAVE;
            }
        }
        for track in &mut score.tracks {
            trim_overlaps(&mut track.notes);
        }
    }
}

/// Where an out-of-system note should go: the nearer of the nearest scale degree
/// and the nearest tone of its chord.
fn mend(ctx: &Ctx, plan: &Plan, track: usize, note: &Note) -> i32 {
    let _ = track;
    let scale = plan
        .system
        .pitch(
            plan.tuning,
            plan.system.nearest_degree(plan.tuning, note.pitch),
        )
        .cents;
    match ctx.chord_classes(note.start) {
        Some(classes) => {
            let tone = nearest_in(&classes, note.pitch.cents);
            if (tone - note.pitch.cents).abs() <= (scale - note.pitch.cents).abs() {
                tone
            } else {
                scale
            }
        }
        None => scale,
    }
}

/// The pitch with one of the classes that is nearest to `cents`.
fn nearest_in(classes: &[i32], cents: i32) -> i32 {
    let octave = cents.div_euclid(OCTAVE) * OCTAVE;
    classes
        .iter()
        .flat_map(|class| {
            [
                octave - OCTAVE + class,
                octave + class,
                octave + OCTAVE + class,
            ]
        })
        .min_by_key(|pitch| ((pitch - cents).abs(), *pitch))
        .unwrap_or(cents)
}

/// Moves a pitch by octaves into a range, if it fits at all.
fn fit(mut cents: i32, (low, high): (i32, i32)) -> i32 {
    while cents > high && cents - OCTAVE >= low {
        cents -= OCTAVE;
    }
    while cents < low && cents + OCTAVE <= high {
        cents += OCTAVE;
    }
    cents
}

/// Where a mend put two notes of one pitch together, the earlier gives way.
fn trim_overlaps(notes: &mut Vec<Note>) {
    for index in 1..notes.len() {
        let (before, after) = notes.split_at_mut(index);
        let current = &after[0];
        if let Some(earlier) = before
            .iter_mut()
            .rev()
            .find(|n| n.pitch == current.pitch && n.end() > current.start)
        {
            earlier.dur = current.start - earlier.start;
        }
    }
    notes.retain(|note| note.dur > 0);
}

#[cfg(test)]
#[path = "critic_tests.rs"]
mod tests;
