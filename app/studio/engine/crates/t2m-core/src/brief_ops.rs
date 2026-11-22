//! Checking and repairing briefs.
//!
//! There is one set of rules. [`repair`] applies every rule in a fixed order,
//! changing whatever breaks it and recording each change. [`validate`] runs the
//! same pass and reports what would change. A brief is valid exactly when the
//! pass changes nothing, so a repaired brief is always valid.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::brief::{
    Articulation, BRIEF_VERSION, Brief, HarmonicLanguage, Length, MeterSpec, Motif, Role, Section,
    SectionHarmony, TonicSpec, TrackSpec,
};
use crate::harmony::parse_roman;
use crate::knowledge::{Kind, Knowledge, Style};
use crate::pitch::parse_pitch_class;
use crate::time::{Meter, TimeFrame};
use crate::tonal::{TonalSystem, Tuning};
use crate::{TICKS_PER_QUARTER, Tick};

pub const MAX_TRACKS: usize = 16;
pub const MAX_SECTIONS: usize = 24;
pub const MAX_BARS: u16 = 512;
/// The longest title and summary kept, in characters.
pub const MAX_TITLE_CHARS: usize = 200;
pub const MAX_SUMMARY_CHARS: usize = 1_000;
/// The longest bar allowed, in quarter notes (8 whole notes), so no meter can be absurd.
pub const MAX_BAR_QUARTERS: i64 = 32;
/// The longest piece allowed, in quarter notes: 512 bars of 4/4. Together with the bar limit
/// this keeps composing time bounded whatever a brief asks for.
pub const MAX_PIECE_QUARTERS: i64 = 2_048;
/// The most beats in a piece (bars x beats per bar): composing time grows with beats, not
/// only with length, so 255/32 must not get more beats than 4/4 does.
pub const MAX_PIECE_BEATS: i64 = 2_048;
pub const MIN_TEMPO: u16 = 30;
pub const MAX_TEMPO: u16 = 300;
/// Most chords one section's progression may list.
pub const MAX_CHORDS: usize = 64;
/// Most notes a motif may have.
pub const MAX_MOTIF_NOTES: usize = 32;
/// Longest motif note, in beats.
pub const MAX_MOTIF_BEATS: f64 = 16.0;
/// Lowest and highest scale step a motif may use.
pub const MOTIF_DEGREES: (i32, i32) = (-21, 21);
/// Tonic frequencies outside this range are not usable.
pub const TONIC_HZ: (f64, f64) = (20.0, 5000.0);

const DEFAULT_TONIC: &str = "C";
const DEFAULT_DRONE: [u8; 2] = [1, 5];
const MIDDLE: f64 = 0.5;

/// One thing wrong with a brief, and what repair does about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefIssue {
    /// JSON pointer to the field, for example `/time/tempo_bpm`.
    pub path: String,
    pub kind: IssueKind,
    /// The value found.
    pub was: String,
    /// The value repair puts in its place; `None` when the value is removed.
    pub now: Option<String>,
}

/// The kinds of problem a brief can have.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    /// An id that the knowledge base does not have.
    UnknownId,
    /// A number outside its allowed range.
    OutOfRange,
    /// More items than allowed.
    TooMany,
    /// Something required is absent or empty.
    Missing,
    /// Two fields that cannot both hold.
    Conflict,
    /// An id used twice.
    Duplicate,
    /// A value that cannot be read.
    Unreadable,
}

/// Checks a brief against every rule. `Err` lists what [`repair`] would change.
pub fn validate(brief: &Brief, kb: &Knowledge) -> Result<(), Vec<BriefIssue>> {
    let (_, issues) = repair(brief.clone(), kb);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

/// Makes any brief valid, reporting each change.
///
/// Unknown ids become the nearest known id of the same kind, or the style's
/// default; numbers are clamped; missing tracks and sections come from the
/// style. A valid brief is returned unchanged with no issues.
pub fn repair(mut brief: Brief, kb: &Knowledge) -> (Brief, Vec<BriefIssue>) {
    let mut fix = Fixer { issues: Vec::new() };
    if brief.version != BRIEF_VERSION {
        fix.note(
            "/version",
            IssueKind::OutOfRange,
            brief.version,
            Some(BRIEF_VERSION),
        );
        brief.version = BRIEF_VERSION;
    }
    for (path, text, limit) in [
        (
            "/identity/title",
            &mut brief.identity.title,
            MAX_TITLE_CHARS,
        ),
        (
            "/identity/summary",
            &mut brief.identity.summary,
            MAX_SUMMARY_CHARS,
        ),
    ] {
        if text.chars().count() > limit {
            let was = text.chars().count();
            *text = text.chars().take(limit).collect();
            fix.note(path, IssueKind::OutOfRange, was, Some(limit));
        }
    }
    let Some(style) = fix_style(&mut brief, kb, &mut fix) else {
        // A loaded knowledge base always has a style; without one nothing more can be decided.
        return (brief, fix.issues);
    };
    fix_time(&mut brief, kb, style, &mut fix);
    let Some((system, tuning)) = fix_tonality(&mut brief, kb, style, &mut fix) else {
        return (brief, fix.issues);
    };
    fix_tracks(&mut brief, kb, style, &mut fix);
    fix_form(&mut brief, kb, style, system, tuning, &mut fix);
    fix_length(&mut brief, kb, &mut fix);
    fix_motif(&mut brief, &mut fix);
    for (index, assumption) in brief.assumptions.iter_mut().enumerate() {
        let path = format!("/assumptions/{index}/confidence");
        fix.unit(&path, &mut assumption.confidence);
    }
    (brief, fix.issues)
}

struct Fixer {
    issues: Vec<BriefIssue>,
}

impl Fixer {
    fn note(
        &mut self,
        path: impl Into<String>,
        kind: IssueKind,
        was: impl ToString,
        now: Option<impl ToString>,
    ) {
        self.issues.push(BriefIssue {
            path: path.into(),
            kind,
            was: was.to_string(),
            now: now.map(|value| value.to_string()),
        });
    }

    /// Brings a 0-to-1 control into range; not-a-number becomes the middle.
    fn unit(&mut self, path: &str, value: &mut f64) {
        let fixed = if value.is_nan() {
            MIDDLE
        } else {
            value.clamp(0.0, 1.0)
        };
        if fixed != *value {
            self.note(path, IssueKind::OutOfRange, *value, Some(fixed));
            *value = fixed;
        }
    }

    /// Replaces an unknown id by the nearest known one, else by `fallback`.
    fn id(&mut self, path: &str, kb: &Knowledge, kind: Kind, id: &mut String, fallback: &str) {
        if exists(kb, kind, id) {
            return;
        }
        let replacement = kb.nearest(kind, id).unwrap_or(fallback).to_string();
        self.note(path, IssueKind::UnknownId, &*id, Some(&replacement));
        *id = replacement;
    }
}

fn exists(kb: &Knowledge, kind: Kind, id: &str) -> bool {
    match kind {
        Kind::Style => kb.style(id).is_some(),
        Kind::TonalSystem => kb.tonal_system(id).is_some(),
        Kind::Tuning => kb.tuning(id).is_some(),
        Kind::Cycle => kb.cycle(id).is_some(),
        Kind::Groove => kb.groove(id).is_some(),
        Kind::Instrument => kb.instrument(id).is_some(),
        Kind::Progression => kb.progression(id).is_some(),
    }
}

fn fix_style<'a>(brief: &mut Brief, kb: &'a Knowledge, fix: &mut Fixer) -> Option<&'a Style> {
    let fallback = kb.default_style()?;
    fix.id(
        "/identity/style",
        kb,
        Kind::Style,
        &mut brief.identity.style,
        &fallback.id,
    );
    if let Some(fusion) = &brief.identity.fusion
        && kb.style(fusion).is_none()
    {
        let nearest = kb.nearest(Kind::Style, fusion).map(str::to_string);
        fix.note(
            "/identity/fusion",
            IssueKind::UnknownId,
            fusion,
            nearest.as_ref(),
        );
        brief.identity.fusion = nearest;
    }
    kb.style(&brief.identity.style)
}

fn fix_time(brief: &mut Brief, kb: &Knowledge, style: &Style, fix: &mut Fixer) {
    let time = &mut brief.time;
    let tempo = time.tempo_bpm.clamp(MIN_TEMPO, MAX_TEMPO);
    if tempo != time.tempo_bpm {
        fix.note(
            "/time/tempo_bpm",
            IssueKind::OutOfRange,
            time.tempo_bpm,
            Some(tempo),
        );
        time.tempo_bpm = tempo;
    }

    let style_meter = MeterSpec::Meter {
        numerator: style.meter.0,
        denominator: style.meter.1,
        grouping: None,
    };
    match &mut time.meter {
        MeterSpec::Meter {
            numerator,
            denominator,
            grouping,
        } => {
            let too_long = Meter::new(*numerator, *denominator, None)
                .map(|m| m.bar_ticks() > MAX_BAR_QUARTERS * TICKS_PER_QUARTER)
                .unwrap_or(false);
            if Meter::new(*numerator, *denominator, None).is_err() || too_long {
                let was = format!("{numerator}/{denominator}");
                let now = format!("{}/{}", style.meter.0, style.meter.1);
                fix.note("/time/meter", IssueKind::OutOfRange, was, Some(now));
                time.meter = style_meter.clone();
            } else if grouping.is_some()
                && Meter::new(*numerator, *denominator, grouping.clone()).is_err()
            {
                let default = Meter::default_grouping(*numerator, *denominator);
                let was = format!("{:?}", grouping.as_deref().unwrap_or_default());
                fix.note(
                    "/time/meter/grouping",
                    IssueKind::Conflict,
                    was,
                    Some(format!("{default:?}")),
                );
                *grouping = None;
            }
        }
        MeterSpec::Cycle { id } => {
            if kb.cycle(id).is_none() {
                match kb.nearest(Kind::Cycle, id).map(str::to_string) {
                    Some(nearest) => {
                        fix.note("/time/meter/id", IssueKind::UnknownId, &*id, Some(&nearest));
                        *id = nearest;
                    }
                    None => {
                        let now = format!("{}/{}", style.meter.0, style.meter.1);
                        fix.note("/time/meter", IssueKind::UnknownId, &*id, Some(now));
                        time.meter = style_meter.clone();
                    }
                }
            }
        }
    }

    fix.id("/time/feel", kb, Kind::Groove, &mut time.feel, &style.feel);
    fix_cycle_feel(brief, kb, fix);
}

/// A cycle needs a groove that can be played in it. Failing that, the cycle
/// is kept as the bar-based meter with the same beats and grouping.
fn fix_cycle_feel(brief: &mut Brief, kb: &Knowledge, fix: &mut Fixer) {
    let time = &mut brief.time;
    let MeterSpec::Cycle { id } = &time.meter else {
        return;
    };
    let plays = |groove_id: &str| {
        kb.groove(groove_id)
            .is_some_and(|groove| groove.cycles.contains(id))
    };
    if plays(&time.feel) {
        return;
    }
    let compatible = kb.grooves().find(|groove| groove.cycles.contains(id));
    if let Some(groove) = compatible {
        fix.note(
            "/time/feel",
            IssueKind::Conflict,
            &time.feel,
            Some(&groove.id),
        );
        time.feel = groove.id.clone();
    } else if let Some(cycle) = kb.cycle(id) {
        let now = format!(
            "{}/{} grouped {:?}",
            cycle.beats, cycle.beat_unit, cycle.grouping
        );
        fix.note("/time/meter", IssueKind::Conflict, id, Some(now));
        time.meter = MeterSpec::Meter {
            numerator: cycle.beats,
            denominator: cycle.beat_unit,
            grouping: Some(cycle.grouping.clone()),
        };
    }
}

fn fix_tonality<'a>(
    brief: &mut Brief,
    kb: &'a Knowledge,
    style: &Style,
    fix: &mut Fixer,
) -> Option<(&'a TonalSystem, &'a Tuning)> {
    let tonality = &mut brief.tonality;
    let usable = match &tonality.tonic {
        TonicSpec::Name(name) => parse_pitch_class(name).is_some(),
        TonicSpec::Hz { hz } => *hz >= TONIC_HZ.0 && *hz <= TONIC_HZ.1,
    };
    if !usable {
        let was = match &tonality.tonic {
            TonicSpec::Name(name) => name.clone(),
            TonicSpec::Hz { hz } => format!("{hz} Hz"),
        };
        fix.note(
            "/tonality/tonic",
            IssueKind::Unreadable,
            was,
            Some(DEFAULT_TONIC),
        );
        tonality.tonic = TonicSpec::Name(DEFAULT_TONIC.to_string());
    }

    fix.id(
        "/tonality/system",
        kb,
        Kind::TonalSystem,
        &mut tonality.system,
        &style.system,
    );
    let system = kb.tonal_system(&tonality.system)?;

    let allowed = |id: &str| id == system.default_tuning || system.tunings.iter().any(|t| t == id);
    if !allowed(&tonality.tuning) {
        let known = kb.tuning(&tonality.tuning).is_some();
        let nearest = kb
            .nearest(Kind::Tuning, &tonality.tuning)
            .filter(|id| allowed(id));
        let replacement = nearest
            .unwrap_or(system.default_tuning.as_str())
            .to_string();
        let kind = if known {
            IssueKind::Conflict
        } else {
            IssueKind::UnknownId
        };
        fix.note(
            "/tonality/tuning",
            kind,
            &tonality.tuning,
            Some(&replacement),
        );
        tonality.tuning = replacement;
    }
    let tuning = kb.tuning(&tonality.tuning)?;

    let chordal = matches!(
        tonality.harmonic_language,
        HarmonicLanguage::Functional | HarmonicLanguage::Modal
    );
    if chordal && !system.chordable {
        let was = language_name(tonality.harmonic_language);
        let now = language_name(HarmonicLanguage::Drone);
        fix.note(
            "/tonality/harmonic_language",
            IssueKind::Conflict,
            was,
            Some(now),
        );
        tonality.harmonic_language = HarmonicLanguage::Drone;
    }
    Some((system, tuning))
}

fn language_name(language: HarmonicLanguage) -> &'static str {
    match language {
        HarmonicLanguage::Functional => "functional",
        HarmonicLanguage::Modal => "modal",
        HarmonicLanguage::Drone => "drone",
        HarmonicLanguage::None => "none",
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Lead => "lead",
        Role::Counter => "counter",
        Role::Chords => "chords",
        Role::Pad => "pad",
        Role::Bass => "bass",
        Role::Drums => "drums",
        Role::Percussion => "percussion",
        Role::Arp => "arp",
        Role::Drone => "drone",
        Role::Fx => "fx",
    }
}

fn needs_kit(role: Role) -> bool {
    matches!(role, Role::Drums | Role::Percussion)
}

fn fix_tracks(brief: &mut Brief, kb: &Knowledge, style: &Style, fix: &mut Fixer) {
    if brief.tracks.is_empty() {
        brief.tracks = style
            .palette
            .iter()
            .take(MAX_TRACKS)
            .map(|entry| TrackSpec {
                id: role_name(entry.role).to_string(),
                role: entry.role,
                instrument: entry.instrument.clone(),
                register: entry.register,
                pattern: entry.pattern.clone(),
                busy: MIDDLE,
                articulation: Articulation::Normal,
            })
            .collect();
        let now = format!("{} tracks from style {}", brief.tracks.len(), style.id);
        fix.note("/tracks", IssueKind::Missing, 0, Some(now));
    }
    if brief.tracks.len() > MAX_TRACKS {
        fix.note(
            "/tracks",
            IssueKind::TooMany,
            brief.tracks.len(),
            Some(MAX_TRACKS),
        );
        brief.tracks.truncate(MAX_TRACKS);
    }
    let ids = brief.tracks.iter_mut().map(|track| &mut track.id);
    unique_ids(ids, "/tracks", "track", fix);

    for (index, track) in brief.tracks.iter_mut().enumerate() {
        let path = format!("/tracks/{index}/instrument");
        fix_instrument(&path, track, kb, style, fix);
        fix.unit(&format!("/tracks/{index}/busy"), &mut track.busy);
    }
}

/// A drum or percussion part needs a kit; every other part needs a pitched instrument.
fn fix_instrument(
    path: &str,
    track: &mut TrackSpec,
    kb: &Knowledge,
    style: &Style,
    fix: &mut Fixer,
) {
    let kit = needs_kit(track.role);
    let fits = |id: &str| kb.instrument(id).is_some_and(|i| i.is_kit() == kit);
    if fits(&track.instrument) {
        return;
    }
    let known = kb.instrument(&track.instrument).is_some();
    let nearest = kb
        .nearest(Kind::Instrument, &track.instrument)
        .filter(|id| !known && fits(id));
    let from_style = style
        .palette
        .iter()
        .find(|entry| entry.role == track.role && fits(&entry.instrument))
        .map(|entry| entry.instrument.as_str());
    let any = kb
        .instruments()
        .find(|i| i.is_kit() == kit)
        .map(|i| i.id.as_str());
    let Some(replacement) = nearest.or(from_style).or(any) else {
        return;
    };
    let kind = if known {
        IssueKind::Conflict
    } else {
        IssueKind::UnknownId
    };
    fix.note(path, kind, &track.instrument, Some(replacement));
    track.instrument = replacement.to_string();
}

/// Gives every empty or repeated id a fresh one.
fn unique_ids<'a>(
    ids: impl Iterator<Item = &'a mut String>,
    path: &str,
    word: &str,
    fix: &mut Fixer,
) {
    let mut seen = BTreeSet::new();
    for (index, id) in ids.enumerate() {
        let empty = id.is_empty();
        if empty || seen.contains(id.as_str()) {
            let base = if empty {
                format!("{word}{}", index + 1)
            } else {
                id.clone()
            };
            let mut fresh = base.clone();
            let mut suffix = 2;
            while seen.contains(&fresh) {
                fresh = format!("{base}_{suffix}");
                suffix += 1;
            }
            let kind = if empty {
                IssueKind::Missing
            } else {
                IssueKind::Duplicate
            };
            fix.note(format!("{path}/{index}/id"), kind, &*id, Some(&fresh));
            *id = fresh;
        }
        seen.insert(id.clone());
    }
}

fn fix_form(
    brief: &mut Brief,
    kb: &Knowledge,
    style: &Style,
    system: &TonalSystem,
    tuning: &Tuning,
    fix: &mut Fixer,
) {
    let language = brief.tonality.harmonic_language;
    let harmony = Harmony {
        kb,
        style,
        system,
        tuning,
        language,
    };
    if brief.form.is_empty() {
        brief.form = style
            .form
            .iter()
            .take(MAX_SECTIONS)
            .map(|template| Section {
                id: template.id.clone(),
                kind: template.kind.clone(),
                bars: template.bars,
                energy: template.energy,
                density: template.density,
                harmony: harmony.default(),
                tracks: brief
                    .tracks
                    .iter()
                    .filter(|track| {
                        template.roles.is_empty() || template.roles.contains(&track.role)
                    })
                    .map(|track| track.id.clone())
                    .collect(),
                transition: template.transition,
            })
            .collect();
        let now = format!("{} sections from style {}", brief.form.len(), style.id);
        fix.note("/form", IssueKind::Missing, 0, Some(now));
    }
    if brief.form.len() > MAX_SECTIONS {
        fix.note(
            "/form",
            IssueKind::TooMany,
            brief.form.len(),
            Some(MAX_SECTIONS),
        );
        brief.form.truncate(MAX_SECTIONS);
    }
    let limit = bar_limit(
        span_ticks(&brief.time.meter, kb),
        beats_per_bar(&brief.time.meter, kb),
    );
    fix_bars(&mut brief.form, limit, fix);
    let ids = brief.form.iter_mut().map(|section| &mut section.id);
    unique_ids(ids, "/form", "section", fix);

    let track_ids: BTreeSet<&str> = brief.tracks.iter().map(|track| track.id.as_str()).collect();
    for (index, section) in brief.form.iter_mut().enumerate() {
        let at = format!("/form/{index}");
        fix.unit(&format!("{at}/energy"), &mut section.energy);
        fix.unit(&format!("{at}/density"), &mut section.density);

        let mut kept: Vec<String> = Vec::with_capacity(section.tracks.len());
        for id in &section.tracks {
            if !track_ids.contains(id.as_str()) {
                fix.note(
                    format!("{at}/tracks"),
                    IssueKind::UnknownId,
                    id,
                    None::<&str>,
                );
            } else if kept.contains(id) {
                fix.note(
                    format!("{at}/tracks"),
                    IssueKind::Duplicate,
                    id,
                    None::<&str>,
                );
            } else {
                kept.push(id.clone());
            }
        }
        section.tracks = kept;
        harmony.fix(&format!("{at}/harmony"), &mut section.harmony, fix);
    }
}

/// Every section lasts at least one bar, and the form at most [`MAX_BARS`].
fn fix_bars(form: &mut Vec<Section>, limit: u16, fix: &mut Fixer) {
    let total: u32 = form
        .iter()
        .map(|section| u32::from(section.bars.max(1)))
        .sum();
    let mut budget = limit;
    let mut kept = 0;
    for (index, section) in form.iter_mut().enumerate() {
        if budget == 0 {
            break;
        }
        let bars = section.bars.clamp(1, budget);
        if section.bars == 0 {
            fix.note(
                format!("/form/{index}/bars"),
                IssueKind::OutOfRange,
                section.bars,
                Some(bars),
            );
        }
        section.bars = bars;
        budget -= bars;
        kept += 1;
    }
    if total > u32::from(limit) {
        fix.note(
            "/form",
            IssueKind::TooMany,
            format!("{total} bars"),
            Some(format!("{limit} bars")),
        );
    }
    form.truncate(kept);
}

struct Harmony<'a> {
    kb: &'a Knowledge,
    style: &'a Style,
    system: &'a TonalSystem,
    tuning: &'a Tuning,
    language: HarmonicLanguage,
}

impl Harmony<'_> {
    /// The harmony a section gets when it has none of its own.
    fn default(&self) -> SectionHarmony {
        match self.language {
            HarmonicLanguage::None => SectionHarmony::None,
            HarmonicLanguage::Drone => SectionHarmony::Drone {
                degrees: self.drone(&DEFAULT_DRONE),
            },
            HarmonicLanguage::Functional | HarmonicLanguage::Modal => {
                match self.style.progressions.first() {
                    Some(id) => SectionHarmony::Named { id: id.clone() },
                    None => SectionHarmony::Progression {
                        roman: vec![self.tonic_chord().to_string()],
                        bars_per_chord: 1,
                    },
                }
            }
        }
    }

    /// The tonic triad's symbol: minor when the system's third is nearer a minor third.
    fn tonic_chord(&self) -> &'static str {
        let third = self.system.pitch(self.tuning, 2).cents;
        if self.system.degrees() == 7 && third < 350 {
            "i"
        } else {
            "I"
        }
    }

    /// The listed degrees that the system has, counted from 1; the tonic if none are left.
    fn drone(&self, degrees: &[u8]) -> Vec<u8> {
        let mut kept: Vec<u8> = Vec::new();
        for degree in degrees {
            let inside = *degree >= 1 && *degree <= self.system.degrees();
            if inside && !kept.contains(degree) {
                kept.push(*degree);
            }
        }
        if kept.is_empty() {
            kept.push(1);
        }
        kept
    }

    fn fix(&self, path: &str, harmony: &mut SectionHarmony, fix: &mut Fixer) {
        let chordal = matches!(
            self.language,
            HarmonicLanguage::Functional | HarmonicLanguage::Modal
        );
        let has_chords = matches!(
            harmony,
            SectionHarmony::Progression { .. } | SectionHarmony::Named { .. }
        );
        if has_chords && !chordal {
            let now = self.default();
            fix.note(
                path,
                IssueKind::Conflict,
                harmony_name(harmony),
                Some(harmony_name(&now)),
            );
            *harmony = now;
        }
        match harmony {
            SectionHarmony::None => {}
            SectionHarmony::Drone { degrees } => {
                let wanted: &[u8] = if degrees.is_empty() {
                    &DEFAULT_DRONE
                } else {
                    degrees.as_slice()
                };
                let kept = self.drone(wanted);
                if kept != *degrees {
                    let was = format!("{degrees:?}");
                    fix.note(
                        format!("{path}/degrees"),
                        IssueKind::OutOfRange,
                        was,
                        Some(format!("{kept:?}")),
                    );
                    *degrees = kept;
                }
            }
            SectionHarmony::Named { id } => {
                if self.kb.progression(id).is_none() {
                    let nearest = self.kb.nearest(Kind::Progression, id);
                    let now = match nearest.or(self.style.progressions.first().map(String::as_str))
                    {
                        Some(known) => SectionHarmony::Named {
                            id: known.to_string(),
                        },
                        None => SectionHarmony::Progression {
                            roman: vec![self.tonic_chord().to_string()],
                            bars_per_chord: 1,
                        },
                    };
                    let shown = match &now {
                        SectionHarmony::Named { id } => id.clone(),
                        _ => self.tonic_chord().to_string(),
                    };
                    fix.note(
                        format!("{path}/id"),
                        IssueKind::UnknownId,
                        &*id,
                        Some(shown),
                    );
                    *harmony = now;
                }
            }
            SectionHarmony::Progression {
                roman,
                bars_per_chord,
            } => {
                if *bars_per_chord == 0 {
                    fix.note(
                        format!("{path}/bars_per_chord"),
                        IssueKind::OutOfRange,
                        0,
                        Some(1),
                    );
                    *bars_per_chord = 1;
                }
                if roman.is_empty() {
                    let tonic = self.tonic_chord();
                    fix.note(
                        format!("{path}/roman"),
                        IssueKind::Missing,
                        "[]",
                        Some(tonic),
                    );
                    roman.push(tonic.to_string());
                }
                if roman.len() > MAX_CHORDS {
                    fix.note(
                        format!("{path}/roman"),
                        IssueKind::TooMany,
                        roman.len(),
                        Some(MAX_CHORDS),
                    );
                    roman.truncate(MAX_CHORDS);
                }
                for (index, symbol) in roman.iter_mut().enumerate() {
                    if parse_roman(symbol, self.system, self.tuning).is_err() {
                        let tonic = self.tonic_chord();
                        let shown: String = symbol.chars().take(32).collect();
                        fix.note(
                            format!("{path}/roman/{index}"),
                            IssueKind::Unreadable,
                            shown,
                            Some(tonic),
                        );
                        *symbol = tonic.to_string();
                    }
                }
            }
        }
    }
}

fn harmony_name(harmony: &SectionHarmony) -> &'static str {
    match harmony {
        SectionHarmony::Progression { .. } => "progression",
        SectionHarmony::Drone { .. } => "drone",
        SectionHarmony::Named { .. } => "named",
        SectionHarmony::None => "none",
    }
}

/// Ticks in one bar or cycle of the brief's meter.
/// How many bars fit in the piece-size limit for a bar of `bar_ticks`.
fn bar_limit(bar_ticks: Tick, beats: i64) -> u16 {
    let by_length = (MAX_PIECE_QUARTERS * TICKS_PER_QUARTER) / bar_ticks.max(1);
    let fit = by_length.min(MAX_PIECE_BEATS / beats.max(1));
    u16::try_from(fit.clamp(1, i64::from(MAX_BARS))).unwrap_or(MAX_BARS)
}

fn beats_per_bar(meter: &MeterSpec, kb: &Knowledge) -> i64 {
    match meter {
        MeterSpec::Meter { numerator, .. } => i64::from(*numerator),
        MeterSpec::Cycle { id } => kb.cycle(id).map_or(4, |cycle| i64::from(cycle.beats)),
    }
}

fn span_ticks(meter: &MeterSpec, kb: &Knowledge) -> Tick {
    match meter {
        MeterSpec::Meter {
            numerator,
            denominator,
            ..
        } => Meter::new(*numerator, *denominator, None)
            .map(|meter| meter.bar_ticks())
            .unwrap_or(TICKS_PER_QUARTER * 4),
        MeterSpec::Cycle { id } => kb
            .cycle(id)
            .map(|cycle| TimeFrame::Cycle(cycle.clone()).span_ticks())
            .unwrap_or(TICKS_PER_QUARTER * 4),
    }
}

/// The piece is 1 to [`MAX_BARS`] bars long and has at least a bar per section.
fn fix_length(brief: &mut Brief, kb: &Knowledge, fix: &mut Fixer) {
    let limit = bar_limit(
        span_ticks(&brief.time.meter, kb),
        beats_per_bar(&brief.time.meter, kb),
    );
    let sections = u16::try_from(brief.form.len()).unwrap_or(limit).min(limit);
    let path = "/time/length";
    match &mut brief.time.length {
        Length::Bars(count) | Length::Cycles(count) => {
            let fixed = (*count).clamp(sections.max(1), limit);
            if fixed != *count {
                fix.note(path, IssueKind::OutOfRange, *count, Some(fixed));
                *count = fixed;
            }
        }
        Length::Seconds(seconds) => {
            // bars = seconds * tempo * ticks-per-quarter / (60 * ticks-per-bar)
            let per_minute = Tick::from(brief.time.tempo_bpm) * TICKS_PER_QUARTER;
            let longest = Tick::from(limit) * 60 * span_ticks(&brief.time.meter, kb) / per_minute;
            let longest = u16::try_from(longest.max(1)).unwrap_or(u16::MAX);
            let fixed = (*seconds).clamp(1, longest);
            if fixed != *seconds {
                fix.note(
                    path,
                    IssueKind::OutOfRange,
                    format!("{seconds} s"),
                    Some(format!("{fixed} s")),
                );
                *seconds = fixed;
            }
        }
    }
}

fn fix_motif(brief: &mut Brief, fix: &mut Fixer) {
    let Some(motif) = &mut brief.motif else {
        return;
    };
    let notes = motif.degrees.len().min(motif.durations.len());
    if motif.degrees.len() != motif.durations.len() {
        let was = format!(
            "{} degrees, {} durations",
            motif.degrees.len(),
            motif.durations.len()
        );
        fix.note(
            "/motif",
            IssueKind::Conflict,
            was,
            Some(format!("{notes} of each")),
        );
        motif.degrees.truncate(notes);
        motif.durations.truncate(notes);
    }
    if notes == 0 {
        fix.note("/motif", IssueKind::Missing, "no notes", None::<&str>);
        brief.motif = None;
        return;
    }
    if notes > MAX_MOTIF_NOTES {
        fix.note("/motif", IssueKind::TooMany, notes, Some(MAX_MOTIF_NOTES));
        motif.degrees.truncate(MAX_MOTIF_NOTES);
        motif.durations.truncate(MAX_MOTIF_NOTES);
    }
    fix_motif_notes(motif, fix);
}

fn fix_motif_notes(motif: &mut Motif, fix: &mut Fixer) {
    for (index, degree) in motif.degrees.iter_mut().enumerate() {
        let fixed = (*degree).clamp(MOTIF_DEGREES.0, MOTIF_DEGREES.1);
        if fixed != *degree {
            fix.note(
                format!("/motif/degrees/{index}"),
                IssueKind::OutOfRange,
                *degree,
                Some(fixed),
            );
            *degree = fixed;
        }
    }
    for (index, beats) in motif.durations.iter_mut().enumerate() {
        let positive = beats.is_finite() && *beats > 0.0;
        let fixed = if positive {
            beats.min(MAX_MOTIF_BEATS)
        } else {
            1.0
        };
        if fixed != *beats {
            fix.note(
                format!("/motif/durations/{index}"),
                IssueKind::OutOfRange,
                *beats,
                Some(fixed),
            );
            *beats = fixed;
        }
    }
}
