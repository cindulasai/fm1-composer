//! The knowledge base: styles, tonal systems, tunings, cycles, grooves,
//! instruments, progressions and the starter gallery.
//!
//! Data lives in `data/<kind>/*.ron` (gallery briefs are `.json`), is embedded
//! in the binary, parsed once into immutable tables and cross-checked: every
//! id referenced must exist and every entry must be internally consistent.
//! Every entry carries a `source` and a `verified` flag.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use include_dir::{Dir, include_dir};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::brief::{
    Brief, HarmonicLanguage, MeterSpec, Register, Role, SectionHarmony, Transition,
};
use crate::groove::Groove;
use crate::harmony::parse_roman;
use crate::time::{Cycle, Meter, TimeError};
use crate::tonal::{MoveGraph, TonalError, TonalFamily, TonalSystem, Tuning};

static EMBEDDED: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data");

/// Longest query, in characters, that name matching looks at.
const MAX_QUERY_CHARS: usize = 64;
/// Shortest text for which "contains" counts as a match.
const MIN_CONTAINS_CHARS: usize = 3;
/// Least similarity (0 to 1) for a fuzzy match to count.
const MIN_SIMILARITY: f64 = 0.8;
/// The style used when a brief names none that can be recognised.
const DEFAULT_STYLE: &str = "pop.generic";

/// Why the knowledge base could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum KnowledgeError {
    #[error("cannot read {path}: {message}")]
    Io { path: String, message: String },
    #[error("{path} is not a knowledge file this build understands")]
    Unexpected { path: String },
    #[error("{file}: {message}")]
    Parse { file: String, message: String },
    #[error("the {kind} id {id} is defined twice")]
    DuplicateId { kind: &'static str, id: String },
    #[error("{from} refers to {to}, which does not exist")]
    MissingRef { from: String, to: String },
    #[error("system {system} does not fit tuning {tuning}")]
    DegreeMismatch { system: String, tuning: String },
    #[error("cycle {cycle} has {got} strokes for {expected} beats")]
    StrokeLength {
        cycle: String,
        expected: u8,
        got: usize,
    },
    #[error("{id}: {reason}")]
    Invalid { id: String, reason: String },
}

/// The categories of entry in the knowledge base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Style,
    TonalSystem,
    Tuning,
    Cycle,
    Groove,
    Instrument,
    Progression,
}

/// A musical style: its defaults and the material it draws on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub tempo: TempoRange,
    /// Default meter as (numerator, denominator).
    pub meter: (u8, u8),
    /// Default groove id.
    pub feel: String,
    /// Default tonal system id.
    pub system: String,
    pub harmonic_language: HarmonicLanguage,
    /// The form used when a brief gives none.
    pub form: Vec<FormTemplate>,
    /// Ids of progressions typical of the style, most typical first.
    #[serde(default)]
    pub progressions: Vec<String>,
    /// Typical instrument and pattern for each role.
    pub palette: Vec<PaletteEntry>,
    pub source: String,
    pub verified: bool,
}

/// Usual tempo limits of a style, in beats per minute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TempoRange {
    pub min: u16,
    pub max: u16,
    pub default: u16,
}

/// One section of a style's default form.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FormTemplate {
    pub id: String,
    pub kind: String,
    pub bars: u16,
    pub energy: f64,
    pub density: f64,
    #[serde(default)]
    pub transition: Transition,
    /// Roles that play in this section; empty means all of them.
    #[serde(default)]
    pub roles: Vec<Role>,
}

/// A style's usual choice for one role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaletteEntry {
    pub role: Role,
    pub instrument: String,
    #[serde(default)]
    pub pattern: String,
    #[serde(default)]
    pub register: Register,
}

/// An instrument: how it is selected in General MIDI and what it can play.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Instrument {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    /// General MIDI program, counted from 0.
    pub gm_program: u8,
    /// Lowest and highest MIDI note. Unused for kits.
    pub low: u8,
    pub high: u8,
    pub polyphonic: bool,
    /// Roles the instrument suits.
    pub roles: Vec<Role>,
    /// For a drum kit, its pieces; empty for pitched instruments.
    #[serde(default)]
    pub kit: Vec<KitPiece>,
    pub source: String,
    pub verified: bool,
}

impl Instrument {
    /// Whether this is a drum kit or percussion set.
    pub fn is_kit(&self) -> bool {
        !self.kit.is_empty()
    }
}

/// One piece of a drum kit and its MIDI note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KitPiece {
    pub name: String,
    pub note: u8,
}

/// A named chord progression in roman numerals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progression {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub roman: Vec<String>,
    /// Bars each chord lasts.
    pub bars_per_chord: u8,
    pub source: String,
    pub verified: bool,
}

/// A complete brief offered as a starting point.
#[derive(Debug, Clone, PartialEq)]
pub struct GalleryEntry {
    pub id: String,
    pub brief: Brief,
}

/// The loaded, cross-checked knowledge base.
#[derive(Debug, Clone, PartialEq)]
pub struct Knowledge {
    styles: BTreeMap<String, Style>,
    systems: BTreeMap<String, TonalSystem>,
    tunings: BTreeMap<String, Tuning>,
    cycles: BTreeMap<String, Cycle>,
    grooves: BTreeMap<String, Groove>,
    instruments: BTreeMap<String, Instrument>,
    progressions: BTreeMap<String, Progression>,
    gallery: Vec<GalleryEntry>,
    version: String,
}

impl Knowledge {
    /// Loads the knowledge base embedded in this build.
    pub fn load() -> Result<Knowledge, KnowledgeError> {
        let mut files = Vec::new();
        for kind in EMBEDDED.dirs() {
            for file in kind.files() {
                let path = file.path().to_string_lossy().replace('\\', "/");
                let text = file.contents_utf8().ok_or_else(|| KnowledgeError::Parse {
                    file: path.clone(),
                    message: "not UTF-8 text".to_string(),
                })?;
                files.push(RawFile {
                    path,
                    text: text.to_string(),
                });
            }
        }
        build(files)
    }

    /// Loads a knowledge base from a directory laid out like `data/`.
    pub fn from_dir(root: &Path) -> Result<Knowledge, KnowledgeError> {
        let mut files = Vec::new();
        for kind in visible_entries(root)? {
            let kind_path = root.join(&kind);
            if !kind_path.is_dir() {
                return Err(KnowledgeError::Unexpected { path: kind });
            }
            for name in visible_entries(&kind_path)? {
                let file_path = kind_path.join(&name);
                let text =
                    std::fs::read_to_string(&file_path).map_err(|error| io(&file_path, &error))?;
                files.push(RawFile {
                    path: format!("{kind}/{name}"),
                    text,
                });
            }
        }
        build(files)
    }

    pub fn style(&self, id: &str) -> Option<&Style> {
        self.styles.get(id)
    }

    pub fn tonal_system(&self, id: &str) -> Option<&TonalSystem> {
        self.systems.get(id)
    }

    pub fn tuning(&self, id: &str) -> Option<&Tuning> {
        self.tunings.get(id)
    }

    pub fn cycle(&self, id: &str) -> Option<&Cycle> {
        self.cycles.get(id)
    }

    pub fn groove(&self, id: &str) -> Option<&Groove> {
        self.grooves.get(id)
    }

    pub fn instrument(&self, id: &str) -> Option<&Instrument> {
        self.instruments.get(id)
    }

    pub fn progression(&self, id: &str) -> Option<&Progression> {
        self.progressions.get(id)
    }

    /// The style to fall back on: the general-purpose one, else the first in id order.
    pub fn default_style(&self) -> Option<&Style> {
        self.styles
            .get(DEFAULT_STYLE)
            .or_else(|| self.styles.values().next())
    }

    /// Every style, in id order.
    pub fn styles(&self) -> impl Iterator<Item = &Style> {
        self.styles.values()
    }

    /// Every instrument, in id order.
    pub fn instruments(&self) -> impl Iterator<Item = &Instrument> {
        self.instruments.values()
    }

    /// Every groove, in id order.
    pub fn grooves(&self) -> impl Iterator<Item = &Groove> {
        self.grooves.values()
    }

    /// The starter gallery, in id order.
    pub fn gallery(&self) -> &[GalleryEntry] {
        &self.gallery
    }

    /// A fingerprint of the data files; changes whenever any of them changes.
    pub fn data_version(&self) -> &str {
        &self.version
    }

    /// Styles matching free text, best first.
    ///
    /// An exact id, name or alias beats a tag, which beats one containing the
    /// other, which beats a near spelling. Case, spaces and punctuation are
    /// ignored. Text that resembles nothing gives no results.
    pub fn search_styles(&self, text: &str, limit: usize) -> Vec<&Style> {
        self.ranked(Kind::Style, text)
            .into_iter()
            .take(limit)
            .filter_map(|id| self.styles.get(id))
            .collect()
    }

    /// The id of the entry of `kind` that best matches free text, if any does.
    pub fn nearest(&self, kind: Kind, text: &str) -> Option<&str> {
        self.ranked(kind, text).into_iter().next()
    }

    /// Entries used by the gallery, directly or through a style or system,
    /// that are unverified or have no source. Empty when the gallery is safe to ship.
    pub fn verification_gaps(&self) -> Vec<String> {
        let reach = self.reachable_from_gallery();
        let mut gaps = Vec::new();
        let mut note = |kind: &str, id: &str, entry: Option<(&str, bool)>| {
            let Some((source, verified)) = entry else {
                return;
            };
            if !verified {
                gaps.push(format!("{kind} {id}: not verified"));
            }
            if source.trim().is_empty() {
                gaps.push(format!("{kind} {id}: no source"));
            }
        };
        for id in &reach.styles {
            note(
                "style",
                id,
                self.style(id).map(|e| (e.source.as_str(), e.verified)),
            );
        }
        for id in &reach.systems {
            let entry = self.tonal_system(id);
            note("system", id, entry.map(|e| (e.source.as_str(), e.verified)));
        }
        for id in &reach.tunings {
            note(
                "tuning",
                id,
                self.tuning(id).map(|e| (e.source.as_str(), e.verified)),
            );
        }
        for id in &reach.cycles {
            note(
                "cycle",
                id,
                self.cycle(id).map(|e| (e.source.as_str(), e.verified)),
            );
        }
        for id in &reach.grooves {
            note(
                "groove",
                id,
                self.groove(id).map(|e| (e.source.as_str(), e.verified)),
            );
        }
        for id in &reach.instruments {
            let entry = self.instrument(id);
            note(
                "instrument",
                id,
                entry.map(|e| (e.source.as_str(), e.verified)),
            );
        }
        for id in &reach.progressions {
            let entry = self.progression(id);
            note(
                "progression",
                id,
                entry.map(|e| (e.source.as_str(), e.verified)),
            );
        }
        gaps
    }

    fn reachable_from_gallery(&self) -> Reach {
        let mut reach = Reach::default();
        for entry in &self.gallery {
            let brief = &entry.brief;
            reach.styles.insert(brief.identity.style.clone());
            reach.styles.extend(brief.identity.fusion.clone());
            reach.grooves.insert(brief.time.feel.clone());
            reach.systems.insert(brief.tonality.system.clone());
            reach.tunings.insert(brief.tonality.tuning.clone());
            if let MeterSpec::Cycle { id } = &brief.time.meter {
                reach.cycles.insert(id.clone());
            }
            for track in &brief.tracks {
                reach.instruments.insert(track.instrument.clone());
            }
            for section in &brief.form {
                if let SectionHarmony::Named { id } = &section.harmony {
                    reach.progressions.insert(id.clone());
                }
            }
        }
        for style in reach.styles.iter().filter_map(|id| self.style(id)) {
            reach.grooves.insert(style.feel.clone());
            reach.systems.insert(style.system.clone());
            reach
                .progressions
                .extend(style.progressions.iter().cloned());
            for entry in &style.palette {
                reach.instruments.insert(entry.instrument.clone());
            }
        }
        for system in reach.systems.iter().filter_map(|id| self.tonal_system(id)) {
            reach.tunings.insert(system.default_tuning.clone());
        }
        for groove in reach.grooves.iter().filter_map(|id| self.groove(id)) {
            reach.cycles.extend(groove.cycles.iter().cloned());
        }
        reach
    }

    /// Ids of `kind` that match `text`, best first.
    fn ranked(&self, kind: Kind, text: &str) -> Vec<&str> {
        let query = normalize(text);
        if query.is_empty() {
            return Vec::new();
        }
        let mut scored: Vec<(Score, &str)> = self
            .candidates(kind)
            .into_iter()
            .filter_map(|candidate| Some((candidate.score(&query)?, candidate.id)))
            .collect();
        scored.sort();
        scored.into_iter().map(|(_, id)| id).collect()
    }

    fn candidates(&self, kind: Kind) -> Vec<Candidate<'_>> {
        fn named<'a>(
            id: &'a str,
            name: &str,
            aliases: &[String],
            tags: &[String],
        ) -> Candidate<'a> {
            let mut names = vec![normalize(id), normalize(tail(id)), normalize(name)];
            names.extend(aliases.iter().map(|alias| normalize(alias)));
            names.retain(|name| !name.is_empty());
            Candidate {
                id,
                names,
                tags: tags.iter().map(|tag| normalize(tag)).collect(),
            }
        }
        let none: &[String] = &[];
        match kind {
            Kind::Style => self
                .styles
                .values()
                .map(|e| named(&e.id, &e.name, &e.aliases, &e.tags))
                .collect(),
            Kind::TonalSystem => self
                .systems
                .values()
                .map(|e| named(&e.id, "", &e.aliases, none))
                .collect(),
            Kind::Tuning => self
                .tunings
                .values()
                .map(|e| named(&e.id, "", none, none))
                .collect(),
            Kind::Cycle => self
                .cycles
                .values()
                .map(|e| named(&e.id, "", none, none))
                .collect(),
            Kind::Groove => self
                .grooves
                .values()
                .map(|e| named(&e.id, &e.name, &e.aliases, none))
                .collect(),
            Kind::Instrument => self
                .instruments
                .values()
                .map(|e| named(&e.id, &e.name, &e.aliases, none))
                .collect(),
            Kind::Progression => self
                .progressions
                .values()
                .map(|e| named(&e.id, &e.name, &e.aliases, none))
                .collect(),
        }
    }
}

#[derive(Default)]
struct Reach {
    styles: BTreeSet<String>,
    systems: BTreeSet<String>,
    tunings: BTreeSet<String>,
    cycles: BTreeSet<String>,
    grooves: BTreeSet<String>,
    instruments: BTreeSet<String>,
    progressions: BTreeSet<String>,
}

/// How well a candidate matches: tier first (lower is better), then distance.
type Score = (u8, u32);

struct Candidate<'a> {
    id: &'a str,
    names: Vec<String>,
    tags: Vec<String>,
}

impl Candidate<'_> {
    fn score(&self, query: &str) -> Option<Score> {
        if self.names.iter().any(|name| name == query) {
            return Some((0, 0));
        }
        if self.tags.iter().any(|tag| tag == query) {
            return Some((1, 0));
        }
        let long_enough = |text: &str| text.chars().count() >= MIN_CONTAINS_CHARS;
        let contains = self.names.iter().any(|name| {
            (long_enough(query) && name.contains(query))
                || (long_enough(name) && query.contains(name.as_str()))
        });
        if contains {
            return Some((2, 0));
        }
        let best = self
            .names
            .iter()
            .map(|name| strsim::jaro_winkler(name, query))
            .fold(0.0, f64::max);
        if best < MIN_SIMILARITY {
            return None;
        }
        // Thousandths of dissimilarity; `best` is between 0 and 1.
        let distance = ((1.0 - best) * 1000.0).round() as u32;
        Some((3, distance))
    }
}

/// Lower-case letters and digits only, capped in length.
fn normalize(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .take(MAX_QUERY_CHARS)
        .collect()
}

/// The part of an id after its last dot: `dorian` for `mode.dorian`.
fn tail(id: &str) -> &str {
    id.rsplit('.').next().unwrap_or(id)
}

struct RawFile {
    /// `kind/file.ext`, with forward slashes.
    path: String,
    text: String,
}

fn io(path: &Path, error: &std::io::Error) -> KnowledgeError {
    KnowledgeError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

/// Names in a directory, sorted, without hidden files.
fn visible_entries(dir: &Path) -> Result<Vec<String>, KnowledgeError> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|error| io(dir, &error))? {
        let entry = entry.map_err(|error| io(dir, &error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

fn build(mut files: Vec<RawFile>) -> Result<Knowledge, KnowledgeError> {
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let mut kb = Knowledge {
        styles: BTreeMap::new(),
        systems: BTreeMap::new(),
        tunings: BTreeMap::new(),
        cycles: BTreeMap::new(),
        grooves: BTreeMap::new(),
        instruments: BTreeMap::new(),
        progressions: BTreeMap::new(),
        gallery: Vec::new(),
        version: fingerprint(&files),
    };
    for file in &files {
        let unexpected = || KnowledgeError::Unexpected {
            path: file.path.clone(),
        };
        let (kind, name) = file.path.split_once('/').ok_or_else(unexpected)?;
        let (stem, extension) = name.rsplit_once('.').ok_or_else(unexpected)?;
        let wanted = if kind == "gallery" { "json" } else { "ron" };
        if extension != wanted {
            return Err(unexpected());
        }
        match kind {
            "styles" => add(&mut kb.styles, "style", file, |e: &Style| &e.id)?,
            "tonal" => add(&mut kb.systems, "system", file, |e: &TonalSystem| &e.id)?,
            "tunings" => add(&mut kb.tunings, "tuning", file, |e: &Tuning| &e.id)?,
            "cycles" => add(&mut kb.cycles, "cycle", file, |e: &Cycle| &e.id)?,
            "grooves" => add(&mut kb.grooves, "groove", file, |e: &Groove| &e.id)?,
            "instruments" => add(&mut kb.instruments, "instrument", file, |e: &Instrument| {
                &e.id
            })?,
            "progressions" => add(
                &mut kb.progressions,
                "progression",
                file,
                |e: &Progression| &e.id,
            )?,
            "gallery" => {
                let brief =
                    Brief::from_json(&file.text).map_err(|error| KnowledgeError::Parse {
                        file: file.path.clone(),
                        message: error.to_string(),
                    })?;
                kb.gallery.push(GalleryEntry {
                    id: stem.to_string(),
                    brief,
                });
            }
            _ => return Err(unexpected()),
        }
    }
    cross_check(&kb)?;
    Ok(kb)
}

/// Parses a RON list of entries and adds each under its id.
fn add<T: DeserializeOwned>(
    table: &mut BTreeMap<String, T>,
    kind: &'static str,
    file: &RawFile,
    id_of: impl Fn(&T) -> &String,
) -> Result<(), KnowledgeError> {
    let entries: Vec<T> = ron::from_str(&file.text).map_err(|error| KnowledgeError::Parse {
        file: file.path.clone(),
        message: error.to_string(),
    })?;
    for entry in entries {
        let id = id_of(&entry).clone();
        if table.contains_key(&id) {
            return Err(KnowledgeError::DuplicateId { kind, id });
        }
        table.insert(id, entry);
    }
    Ok(())
}

/// FNV-1a over every path and its contents, as 16 hex digits.
fn fingerprint(files: &[RawFile]) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET;
    let mut feed = |bytes: &[u8]| {
        for byte in bytes.iter().chain(&[0x1f]) {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
        }
    };
    for file in files {
        feed(file.path.as_bytes());
        feed(file.text.as_bytes());
    }
    format!("{hash:016x}")
}

fn invalid(id: &str, reason: impl ToString) -> KnowledgeError {
    KnowledgeError::Invalid {
        id: id.to_string(),
        reason: reason.to_string(),
    }
}

fn require<T>(table: &BTreeMap<String, T>, from: &str, to: &str) -> Result<(), KnowledgeError> {
    if table.contains_key(to) {
        Ok(())
    } else {
        Err(KnowledgeError::MissingRef {
            from: from.to_string(),
            to: to.to_string(),
        })
    }
}

fn tonal(error: TonalError) -> KnowledgeError {
    match error {
        TonalError::DegreeMismatch { system, tuning } => {
            KnowledgeError::DegreeMismatch { system, tuning }
        }
        TonalError::TuningEmpty { ref id }
        | TonalError::TuningFirst { ref id }
        | TonalError::TuningOrder { ref id }
        | TonalError::TuningPeriod { ref id } => invalid(id, &error),
        TonalError::SystemSteps { ref system } | TonalError::DegreeOutside { ref system, .. } => {
            invalid(system, &error)
        }
    }
}

fn cross_check(kb: &Knowledge) -> Result<(), KnowledgeError> {
    // Repairing a brief needs something to fall back on for every part.
    if kb.styles.is_empty() {
        return Err(invalid("styles", "at least one style is needed"));
    }
    let kits = kb.instruments.values().filter(|i| i.is_kit()).count();
    let pitched = kb.instruments.len().saturating_sub(kits);
    if kits.min(pitched) == 0 {
        return Err(invalid(
            "instruments",
            "a drum kit plus a pitched instrument are needed",
        ));
    }
    for tuning in kb.tunings.values() {
        tuning.check().map_err(tonal)?;
    }
    for system in kb.systems.values() {
        check_system(kb, system)?;
    }
    for cycle in kb.cycles.values() {
        cycle.check().map_err(|error| match error {
            TimeError::StrokeLength { expected, got } => KnowledgeError::StrokeLength {
                cycle: cycle.id.clone(),
                expected,
                got,
            },
            other => invalid(&cycle.id, other),
        })?;
    }
    for groove in kb.grooves.values() {
        groove.check().map_err(|why| invalid(&groove.id, why))?;
        for cycle in &groove.cycles {
            require(&kb.cycles, &groove.id, cycle)?;
        }
    }
    for instrument in kb.instruments.values() {
        check_instrument(instrument)?;
    }
    let (major, twelve) = reference_major();
    for progression in kb.progressions.values() {
        if progression.roman.is_empty() {
            return Err(invalid(&progression.id, "no chords"));
        }
        if progression.bars_per_chord == 0 {
            return Err(invalid(
                &progression.id,
                "bars_per_chord must be at least 1",
            ));
        }
        for symbol in &progression.roman {
            parse_roman(symbol, &major, &twelve)
                .map_err(|error| invalid(&progression.id, error))?;
        }
    }
    for style in kb.styles.values() {
        check_style(kb, style)?;
    }
    for entry in &kb.gallery {
        check_gallery(kb, entry)?;
    }
    Ok(())
}

fn check_system(kb: &Knowledge, system: &TonalSystem) -> Result<(), KnowledgeError> {
    require(&kb.tunings, &system.id, &system.default_tuning)?;
    let table_size = |id: &str| kb.tunings.get(id).map(|tuning| tuning.cents.len());
    let default_size = table_size(&system.default_tuning);
    let all = std::iter::once(&system.default_tuning).chain(&system.tunings);
    for tuning_id in all {
        require(&kb.tunings, &system.id, tuning_id)?;
        // Steps index the table, so every tuning of a system must have the same size.
        if table_size(tuning_id) != default_size {
            return Err(KnowledgeError::DegreeMismatch {
                system: system.id.clone(),
                tuning: tuning_id.clone(),
            });
        }
        if let Some(tuning) = kb.tunings.get(tuning_id) {
            system.check(tuning).map_err(tonal)?;
        }
    }
    Ok(())
}

fn check_instrument(instrument: &Instrument) -> Result<(), KnowledgeError> {
    let id = &instrument.id;
    if instrument.gm_program > 127 {
        return Err(invalid(id, "gm_program must be 0 to 127"));
    }
    if instrument.roles.is_empty() {
        return Err(invalid(id, "no roles"));
    }
    if instrument.is_kit() {
        if instrument.kit.iter().any(|piece| piece.note > 127) {
            return Err(invalid(id, "kit notes must be 0 to 127"));
        }
        let names: BTreeSet<&str> = instrument
            .kit
            .iter()
            .map(|piece| piece.name.as_str())
            .collect();
        if names.len() != instrument.kit.len() {
            return Err(invalid(id, "a kit piece is named twice"));
        }
    } else if instrument.low > instrument.high || instrument.high > 127 {
        return Err(invalid(
            id,
            "the range must run from low up to high, at most 127",
        ));
    }
    Ok(())
}

fn check_style(kb: &Knowledge, style: &Style) -> Result<(), KnowledgeError> {
    let id = &style.id;
    require(&kb.grooves, id, &style.feel)?;
    require(&kb.systems, id, &style.system)?;
    for progression in &style.progressions {
        require(&kb.progressions, id, progression)?;
    }
    for entry in &style.palette {
        require(&kb.instruments, id, &entry.instrument)?;
    }
    let tempo = style.tempo;
    let ordered = tempo.min <= tempo.default && tempo.default <= tempo.max;
    if !ordered || tempo.min < 30 || tempo.max > 300 {
        return Err(invalid(
            id,
            "tempo must run from min through default to max, within 30 to 300",
        ));
    }
    Meter::new(style.meter.0, style.meter.1, None).map_err(|error| invalid(id, error))?;
    if style.form.is_empty() || style.palette.is_empty() {
        return Err(invalid(
            id,
            "a style needs a form with sections, plus a palette",
        ));
    }
    let unit = |value: f64| (0.0..=1.0).contains(&value);
    for section in &style.form {
        if section.bars == 0 || !unit(section.energy) || !unit(section.density) {
            return Err(invalid(
                id,
                format!("form section {} is out of range", section.id),
            ));
        }
    }
    Ok(())
}

fn check_gallery(kb: &Knowledge, entry: &GalleryEntry) -> Result<(), KnowledgeError> {
    let from = format!("gallery/{}", entry.id);
    let brief = &entry.brief;
    require(&kb.styles, &from, &brief.identity.style)?;
    if let Some(fusion) = &brief.identity.fusion {
        require(&kb.styles, &from, fusion)?;
    }
    require(&kb.grooves, &from, &brief.time.feel)?;
    require(&kb.systems, &from, &brief.tonality.system)?;
    require(&kb.tunings, &from, &brief.tonality.tuning)?;
    if let MeterSpec::Cycle { id } = &brief.time.meter {
        require(&kb.cycles, &from, id)?;
    }
    for track in &brief.tracks {
        require(&kb.instruments, &from, &track.instrument)?;
    }
    for section in &brief.form {
        if let SectionHarmony::Named { id } = &section.harmony {
            require(&kb.progressions, &from, id)?;
        }
    }
    Ok(())
}

/// The major scale in twelve-tone equal temperament, for checking chord symbols.
fn reference_major() -> (TonalSystem, Tuning) {
    let tuning = Tuning {
        id: "12tet".to_string(),
        cents: (0..12).map(|step| step * 100).collect(),
        period: 1200,
        source: String::new(),
        verified: false,
    };
    let system = TonalSystem {
        id: "mode.major".to_string(),
        family: TonalFamily::Mode,
        steps: vec![0, 2, 4, 5, 7, 9, 11],
        default_tuning: tuning.id.clone(),
        tunings: Vec::new(),
        aliases: Vec::new(),
        moves: MoveGraph::default(),
        emphasis: Vec::new(),
        rest_degrees: Vec::new(),
        phrases: Vec::new(),
        ornaments: Vec::new(),
        chordable: true,
        source: String::new(),
        verified: false,
    };
    (system, tuning)
}
