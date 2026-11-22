use std::collections::BTreeSet;

use include_dir::{Dir, include_dir};
use t2m_audio::{Piece, piece_of};
use t2m_compose::{Rerolls, critique, recompose};
use t2m_core::{Brief, Knowledge, Score, repair};
use t2m_midi::{ExportBundle, ExportOptions, MidiError, TuningProfile, export};

use crate::view::{SectionView, TrackView, View, tracks_of};

/// The example briefs offered on the start screen.
static BRIEFS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/../../suite/briefs");
const TICKS_PER_QUARTER: i64 = 960;
/// How many steps back undo can go.
const HISTORY: usize = 100;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("nothing is open yet")]
    NothingOpen,
    #[error("no such example: {0}")]
    NoSuchExample(String),
    #[error("no such part: {0}")]
    NoSuchTrack(String),
    #[error("no such section: {0}")]
    NoSuchSection(String),
    #[error("{0} is locked; unlock it first")]
    Locked(String),
    #[error("the brief is not valid JSON for a brief: {0}")]
    BadBrief(String),
    #[error("{0}")]
    Export(String),
    #[error("{0}")]
    BadPrompt(String),
    #[error("ask for 1 to {MAX_TAKES} takes, not {0}")]
    BadTakes(u8),
    #[error("cannot use this project file: {0}")]
    BadProject(String),
    #[error("the built-in knowledge base is damaged: {0}")]
    Knowledge(String),
}

/// One example on the start screen.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct GalleryItem {
    pub id: String,
    pub title: String,
    pub style: String,
}

/// What is written to a project file.
#[derive(serde::Serialize, serde::Deserialize)]
struct Project {
    format: u32,
    gallery_id: String,
    brief: Brief,
    seed: u64,
    rerolls: Vec<(String, String, u32)>,
    locked: Vec<String>,
    score: Score,
}

const PROJECT_FORMAT: u32 = 1;
const MAX_REROLLS: u32 = 10_000;
/// Largest pitch distance (cents) a note in a project file may have: twelve octaves.
const MAX_CENTS: u32 = 14_400;

/// One more reroll of a part of a section. The count wraps back to 1 at [`MAX_REROLLS`], so a
/// piece that has been rerolled very many times still saves and opens (an old take can then come
/// back, which is harmless).
fn bump(rerolls: &mut Rerolls, track: &str, section: &str) {
    let count = rerolls
        .0
        .entry((track.to_string(), section.to_string()))
        .or_insert(0);
    *count = if *count >= MAX_REROLLS { 1 } else { *count + 1 };
}

/// A name made only of letters, digits, `-` and `_` (at most 64), so it can sit in a file name.
fn safe_id(id: &str) -> String {
    let clean: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    if clean.is_empty() {
        "custom".to_string()
    } else {
        clean
    }
}
/// The largest project file accepted.
/// The most alternative takes asked for at once.
pub const MAX_TAKES: u8 = 8;
const MAX_PROJECT_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone)]
struct State {
    gallery_id: String,
    brief: Brief,
    seed: u64,
    rerolls: Rerolls,
    locked: BTreeSet<String>,
    score: Score,
}

pub struct Session {
    kb: Knowledge,
    examples: Vec<(String, Brief)>,
    state: Option<State>,
    undo: Vec<State>,
    redo: Vec<State>,
}

impl Session {
    pub fn new() -> Result<Session, SessionError> {
        let kb = Knowledge::load().map_err(|e| SessionError::Knowledge(e.to_string()))?;
        let mut examples: Vec<(String, Brief)> = Vec::new();
        for file in BRIEFS.files() {
            let path = file.path();
            let (Some(stem), Some(text)) = (path.file_stem(), file.contents_utf8()) else {
                continue;
            };
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(brief) = Brief::from_json(text) {
                examples.push((stem.to_string_lossy().into_owned(), repair(brief, &kb).0));
            }
        }
        examples.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(Session {
            kb,
            examples,
            state: None,
            undo: Vec::new(),
            redo: Vec::new(),
        })
    }

    /// The example briefs, in id order.
    pub fn gallery(&self) -> Vec<GalleryItem> {
        self.examples
            .iter()
            .map(|(id, brief)| GalleryItem {
                id: id.clone(),
                title: brief.identity.title.clone(),
                style: brief.identity.style.clone(),
            })
            .collect()
    }

    /// Opens an example and composes it with `seed`.
    pub fn open_example(&mut self, id: &str, seed: u64) -> Result<View, SessionError> {
        let brief = self
            .examples
            .iter()
            .find(|(name, _)| name == id)
            .map(|(_, brief)| brief.clone())
            .ok_or_else(|| SessionError::NoSuchExample(id.to_string()))?;
        Ok(self.open(id, brief, seed))
    }

    /// Opens the piece a short description asks for ("slow sad lofi in D minor, 80 bpm").
    /// A plain rule-based reading, not a language model; guesses are listed in the brief.
    pub fn open_prompt(&mut self, text: &str, seed: u64) -> Result<View, SessionError> {
        let brief = crate::prompt::understand(&self.kb, &self.examples, text, seed)
            .map_err(SessionError::BadPrompt)?;
        Ok(self.open("prompt", brief, seed))
    }

    /// Opens a brief (repaired if it needs it) and composes it; used for briefs made elsewhere,
    /// such as by a language model.
    pub fn open_brief(&mut self, brief: Brief, seed: u64) -> View {
        let brief = repair(brief, &self.kb).0;
        self.open("prompt", brief, seed)
    }

    /// Opens a brief given as JSON (repaired if it has mistakes) and composes it.
    pub fn open_json(&mut self, json: &str, seed: u64) -> Result<View, SessionError> {
        let brief = Brief::from_json(json).map_err(|e| SessionError::BadBrief(e.to_string()))?;
        let brief = repair(brief, &self.kb).0;
        Ok(self.open("custom", brief, seed))
    }

    fn open(&mut self, id: &str, brief: Brief, seed: u64) -> View {
        let rerolls = Rerolls::default();
        let score = recompose(&brief, &self.kb, seed, &rerolls);
        self.undo.clear();
        self.redo.clear();
        self.state = Some(State {
            gallery_id: id.to_string(),
            brief,
            seed,
            rerolls,
            locked: BTreeSet::new(),
            score,
        });
        self.view_unchecked()
    }

    pub fn view(&self) -> Result<View, SessionError> {
        self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        Ok(self.view_unchecked())
    }

    /// Checks that `track` can be rerolled in `section` (or everywhere) and lists the sections.
    fn reroll_targets(
        &self,
        track: &str,
        section: Option<&str>,
    ) -> Result<Vec<String>, SessionError> {
        let current = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        if !current.score.tracks.iter().any(|t| t.id == track) {
            return Err(SessionError::NoSuchTrack(track.to_string()));
        }
        if current.locked.contains(track) {
            return Err(SessionError::Locked(track.to_string()));
        }
        if let Some(id) = section
            && !current.score.sections.iter().any(|s| s.id == id)
        {
            return Err(SessionError::NoSuchSection(id.to_string()));
        }
        Ok(match section {
            Some(id) => vec![id.to_string()],
            None => current
                .score
                .sections
                .iter()
                .map(|s| s.id.clone())
                .collect(),
        })
    }

    /// Asks for a fresh take of one part, in one section or in all of them.
    pub fn reroll(&mut self, track: &str, section: Option<&str>) -> Result<View, SessionError> {
        let ids = self.reroll_targets(track, section)?;
        self.change(|state| {
            for id in &ids {
                bump(&mut state.rerolls, track, id);
            }
        })
    }

    /// `count` (1 to [`MAX_TAKES`]) alternative takes of one part, as that part would look after
    /// one, two, … rerolls. Nothing changes: no undo step, no new state. Take `k` is what
    /// [`Session::keep_take`] with `k` gives.
    pub fn takes(
        &self,
        track: &str,
        section: Option<&str>,
        count: u8,
    ) -> Result<Vec<TrackView>, SessionError> {
        let ids = self.reroll_targets(track, section)?;
        if count == 0 || count > MAX_TAKES {
            return Err(SessionError::BadTakes(count));
        }
        let state = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        let mut rerolls = state.rerolls.clone();
        let mut out = Vec::new();
        for _ in 0..count {
            for id in &ids {
                bump(&mut rerolls, track, id);
            }
            let score = recompose(&state.brief, &self.kb, state.seed, &rerolls);
            let one = tracks_of(&score, &state.locked)
                .into_iter()
                .find(|t| t.id == track)
                .ok_or_else(|| SessionError::NoSuchTrack(track.to_string()))?;
            out.push(one);
        }
        Ok(out)
    }

    /// Keeps take number `take` (1 to [`MAX_TAKES`]) from [`Session::takes`]: one undo step.
    pub fn keep_take(
        &mut self,
        track: &str,
        section: Option<&str>,
        take: u8,
    ) -> Result<View, SessionError> {
        let ids = self.reroll_targets(track, section)?;
        if take == 0 || take > MAX_TAKES {
            return Err(SessionError::BadTakes(take));
        }
        self.change(|state| {
            for _ in 0..take {
                for id in &ids {
                    bump(&mut state.rerolls, track, id);
                }
            }
        })
    }

    /// A different take of the whole piece; locked parts stay as they are.
    pub fn reseed(&mut self, seed: u64) -> Result<View, SessionError> {
        self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        self.change(|state| state.seed = seed)
    }

    /// Locks or unlocks a part. Unlocking shows the part as the piece now composes it,
    /// so what is on screen always matches what a later change will keep.
    pub fn lock(&mut self, track: &str, on: bool) -> Result<View, SessionError> {
        let current = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        if !current.score.tracks.iter().any(|t| t.id == track) {
            return Err(SessionError::NoSuchTrack(track.to_string()));
        }
        if current.locked.contains(track) == on {
            return Ok(self.view_unchecked());
        }
        if on {
            self.push_undo();
            if let Some(state) = self.state.as_mut() {
                state.locked.insert(track.to_string());
            }
            return Ok(self.view_unchecked());
        }
        self.change(|state| {
            state.locked.remove(track);
        })
    }

    pub fn undo(&mut self) -> Result<View, SessionError> {
        self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        if let Some(previous) = self.undo.pop()
            && let Some(now) = self.state.replace(previous)
        {
            self.redo.push(now);
        }
        Ok(self.view_unchecked())
    }

    pub fn redo(&mut self) -> Result<View, SessionError> {
        self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        if let Some(next) = self.redo.pop()
            && let Some(now) = self.state.replace(next)
        {
            self.undo.push(now);
        }
        Ok(self.view_unchecked())
    }

    /// The open piece as files; `tracks` limits it to those parts (all when empty).
    pub fn export(
        &self,
        profile: TuningProfile,
        tracks: &[String],
    ) -> Result<ExportBundle, SessionError> {
        let state = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        let options = ExportOptions {
            profile,
            tracks: if tracks.is_empty() {
                t2m_midi::TrackSelection::All
            } else {
                t2m_midi::TrackSelection::Only(tracks.to_vec())
            },
            name: state.gallery_id.clone(),
            ..ExportOptions::default()
        };
        export(&state.score, &self.kb, &options)
            .map_err(|e: MidiError| SessionError::Export(e.to_string()))
    }

    /// The open piece as it plays now (locks, rerolls and undo included).
    pub fn piece(&self) -> Result<Piece, SessionError> {
        let state = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        Ok(piece_of(&state.score))
    }

    /// The open piece as a project file (JSON): the brief, seed, rerolls, locks and the exact notes.
    pub fn save(&self) -> Result<String, SessionError> {
        let state = self.state.as_ref().ok_or(SessionError::NothingOpen)?;
        let project = Project {
            format: PROJECT_FORMAT,
            gallery_id: state.gallery_id.clone(),
            brief: state.brief.clone(),
            seed: state.seed,
            rerolls: state
                .rerolls
                .0
                .iter()
                .map(|((track, section), count)| (track.clone(), section.clone(), *count))
                .collect(),
            locked: state.locked.iter().cloned().collect(),
            score: state.score.clone(),
        };
        serde_json::to_string(&project).map_err(|e| SessionError::BadProject(e.to_string()))
    }

    /// Opens a project file. Anything wrong with it leaves the open piece untouched.
    pub fn open_project(&mut self, json: &str) -> Result<View, SessionError> {
        let bad = |why: &str| SessionError::BadProject(why.to_string());
        if json.len() > MAX_PROJECT_BYTES {
            return Err(bad("the file is too large"));
        }
        let project: Project =
            serde_json::from_str(json).map_err(|e| SessionError::BadProject(e.to_string()))?;
        if project.format != PROJECT_FORMAT {
            return Err(bad("it was written by a different version of the app"));
        }
        let brief = repair(project.brief, &self.kb).0;
        if project
            .rerolls
            .iter()
            .any(|(_, _, count)| *count > MAX_REROLLS)
        {
            return Err(bad("it has an impossible number of rerolls"));
        }
        // The piece is rebuilt from the brief, seed and rerolls, so its parts, sections,
        // tempo and tuning are always consistent; only the locked parts' notes are taken
        // from the file, after they are checked.
        let mut rerolls = Rerolls::default();
        for (track, section, count) in &project.rerolls {
            rerolls.0.insert((track.clone(), section.clone()), *count);
        }
        let mut score = recompose(&brief, &self.kb, project.seed, &rerolls);
        let known = |id: &String| score.tracks.iter().any(|t| &t.id == id);
        if !project.locked.iter().all(known) {
            return Err(bad("it locks a part that does not exist"));
        }
        if !project.rerolls.iter().all(|(track, section, _)| {
            known(track) && score.sections.iter().any(|s| &s.id == section)
        }) {
            return Err(bad("it rerolls a part or section that does not exist"));
        }
        for track in &mut score.tracks {
            if project.locked.contains(&track.id) {
                let Some(saved) = project.score.tracks.iter().find(|t| t.id == track.id) else {
                    return Err(bad("a locked part has no notes in the file"));
                };
                if saved.notes.iter().any(|n| {
                    n.pitch.cents.unsigned_abs() > MAX_CENTS
                        || n.bend
                            .as_ref()
                            .is_some_and(|b| b.0.iter().any(|&(_, c)| c.unsigned_abs() > MAX_CENTS))
                }) {
                    return Err(bad("a locked part has notes out of range"));
                }
                track.notes = saved.notes.clone();
            }
        }
        score
            .check()
            .map_err(|_| bad("its notes are not a valid piece"))?;
        let gallery_id = safe_id(&project.gallery_id);
        self.undo.clear();
        self.redo.clear();
        self.state = Some(State {
            gallery_id,
            brief,
            seed: project.seed,
            rerolls,
            locked: project.locked.into_iter().collect(),
            score,
        });
        Ok(self.view_unchecked())
    }

    fn push_undo(&mut self) {
        if let Some(state) = &self.state {
            self.undo.push(state.clone());
            if self.undo.len() > HISTORY {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    /// Applies `edit`, recomposes, and keeps locked parts exactly as they were.
    fn change(&mut self, edit: impl FnOnce(&mut State)) -> Result<View, SessionError> {
        self.push_undo();
        let kb = &self.kb;
        if let Some(state) = self.state.as_mut() {
            let before = state.score.clone();
            edit(state);
            let mut score = recompose(&state.brief, kb, state.seed, &state.rerolls);
            for track in &mut score.tracks {
                if state.locked.contains(&track.id)
                    && let Some(old) = before.tracks.iter().find(|t| t.id == track.id)
                {
                    track.notes = old.notes.clone();
                }
            }
            state.score = score;
        }
        Ok(self.view_unchecked())
    }

    fn view_unchecked(&self) -> View {
        let Some(state) = &self.state else {
            return View {
                gallery_id: String::new(),
                title: String::new(),
                style: String::new(),
                tempo_bpm: 0,
                seed: 0,
                ticks_per_quarter: TICKS_PER_QUARTER,
                end: 0,
                sections: Vec::new(),
                tracks: Vec::new(),
                failed_gates: Vec::new(),
                can_undo: false,
                can_redo: false,
            };
        };
        View {
            gallery_id: state.gallery_id.clone(),
            title: state.brief.identity.title.clone(),
            style: state.brief.identity.style.clone(),
            tempo_bpm: state.score.meta.tempo_bpm,
            seed: state.seed,
            ticks_per_quarter: TICKS_PER_QUARTER,
            end: state.score.end(),
            sections: state
                .score
                .sections
                .iter()
                .map(|s| SectionView {
                    id: s.id.clone(),
                    start: s.start,
                    end: s.end,
                })
                .collect(),
            tracks: tracks_of(&state.score, &state.locked),
            failed_gates: critique(&state.score, &state.brief, &self.kb).hard_failures,
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "prompt_tests.rs"]
mod prompt_tests;
