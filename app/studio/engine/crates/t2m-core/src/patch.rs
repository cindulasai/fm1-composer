//! Edits to a brief, and what an edit touches.
//!
//! A patch is a list of operations addressed by JSON pointer, in the style of
//! RFC 6902 plus `adjust` for nudging a number. [`apply`] works on the brief's
//! JSON form, so any field can be edited without a method per field, then
//! repairs the result. [`changed_inputs`] tells the composer which parts of the
//! piece an edit invalidates.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::brief::{Brief, Section, TrackSpec};
use crate::brief_ops::repair;
use crate::knowledge::Knowledge;

/// A list of edits, applied in order, all or none.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BriefPatch(pub Vec<PatchOp>);

/// One edit. Paths are JSON pointers into the brief, for example `/form/1/energy`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PatchOp {
    /// Sets an existing value.
    Replace { path: String, value: Value },
    /// Inserts into a list (`/tracks/2`, or `/tracks/-` to append) or sets an existing field.
    Add { path: String, value: Value },
    /// Removes a list item or an optional field.
    Remove { path: String },
    /// Adds `delta` to a number. Whole-number fields stay whole.
    Adjust { path: String, delta: f64 },
}

/// Why a patch was not applied. The brief is unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PatchError {
    #[error("edit {op}: nothing at {path:?}")]
    Path { op: usize, path: String },
    #[error("edit {op} at {path:?}: {reason}")]
    Value {
        op: usize,
        path: String,
        reason: &'static str,
    },
    #[error("the edited brief is not readable: {message}")]
    Shape { message: String },
}

/// Applies every edit, or none, then repairs the result against the knowledge base.
///
/// Removing a track also removes it from every section, because repair drops
/// references to tracks that no longer exist.
pub fn apply(brief: &Brief, patch: &BriefPatch, kb: &Knowledge) -> Result<Brief, PatchError> {
    let shape = |error: serde_json::Error| PatchError::Shape {
        message: error.to_string(),
    };
    let mut document = serde_json::to_value(brief).map_err(shape)?;
    for (op, edit) in patch.0.iter().enumerate() {
        apply_one(&mut document, op, edit)?;
    }
    let edited: Brief = serde_json::from_value(document).map_err(shape)?;
    Ok(repair(edited, kb).0)
}

fn apply_one(document: &mut Value, op: usize, edit: &PatchOp) -> Result<(), PatchError> {
    match edit {
        PatchOp::Replace { path, value } => {
            *locate(document, op, path)? = value.clone();
        }
        PatchOp::Adjust { path, delta } => {
            let target = locate(document, op, path)?;
            *target = adjusted(target, *delta).map_err(|reason| PatchError::Value {
                op,
                path: path.clone(),
                reason,
            })?;
        }
        PatchOp::Add { path, value } => {
            let (parent, key) = locate_parent(document, op, path)?;
            match parent {
                Value::Object(fields) => {
                    // Empty optional fields are present as null; a name the brief
                    // does not have is a mistake, not a new field.
                    if !fields.contains_key(&key) {
                        return Err(missing(op, path));
                    }
                    fields.insert(key, value.clone());
                }
                Value::Array(items) => {
                    let index = if key == "-" {
                        Some(items.len())
                    } else {
                        list_index(&key).filter(|index| *index <= items.len())
                    };
                    let index = index.ok_or_else(|| missing(op, path))?;
                    items.insert(index, value.clone());
                }
                _ => return Err(missing(op, path)),
            }
        }
        PatchOp::Remove { path } => {
            let (parent, key) = locate_parent(document, op, path)?;
            let removed = match parent {
                Value::Object(fields) => fields.remove(&key).is_some(),
                Value::Array(items) => {
                    match list_index(&key).filter(|index| *index < items.len()) {
                        Some(index) => {
                            items.remove(index);
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            };
            if !removed {
                return Err(missing(op, path));
            }
        }
    }
    Ok(())
}

fn missing(op: usize, path: &str) -> PatchError {
    PatchError::Path {
        op,
        path: path.to_string(),
    }
}

/// Splits a JSON pointer into its unescaped tokens. A pointer starts with `/`.
fn tokens(path: &str) -> Option<Vec<String>> {
    let rest = path.strip_prefix('/')?;
    Some(
        rest.split('/')
            .map(|token| token.replace("~1", "/").replace("~0", "~"))
            .collect(),
    )
}

/// A list position: digits only, no sign, no leading zero.
fn list_index(token: &str) -> Option<usize> {
    let plain = token == "0" || !token.starts_with('0');
    if plain && token.bytes().all(|byte| byte.is_ascii_digit()) {
        token.parse().ok()
    } else {
        None
    }
}

fn step<'a>(value: &'a mut Value, token: &str) -> Option<&'a mut Value> {
    match value {
        Value::Object(fields) => fields.get_mut(token),
        Value::Array(items) => items.get_mut(list_index(token)?),
        _ => None,
    }
}

fn locate<'a>(document: &'a mut Value, op: usize, path: &str) -> Result<&'a mut Value, PatchError> {
    let mut at = document;
    for token in tokens(path).ok_or_else(|| missing(op, path))? {
        at = step(at, &token).ok_or_else(|| missing(op, path))?;
    }
    Ok(at)
}

/// The container holding the pointer's last token, and that token.
fn locate_parent<'a>(
    document: &'a mut Value,
    op: usize,
    path: &str,
) -> Result<(&'a mut Value, String), PatchError> {
    let mut all = tokens(path).ok_or_else(|| missing(op, path))?;
    let last = all.pop().ok_or_else(|| missing(op, path))?;
    let mut at = document;
    for token in all {
        at = step(at, &token).ok_or_else(|| missing(op, path))?;
    }
    Ok((at, last))
}

fn adjusted(target: &Value, delta: f64) -> Result<Value, &'static str> {
    if !delta.is_finite() {
        return Err("the amount is not a finite number");
    }
    let Value::Number(number) = target else {
        return Err("only a number can be adjusted");
    };
    if let Some(whole) = number.as_i64() {
        // Whole-number fields stay whole; the result is rounded to the nearest.
        let moved = (whole as f64 + delta).round();
        let fits = moved >= i64::MIN as f64 && moved <= i64::MAX as f64;
        return fits
            .then(|| Value::from(moved as i64))
            .ok_or("the result is too large");
    }
    let moved = number.as_f64().unwrap_or(0.0) + delta;
    serde_json::Number::from_f64(moved)
        .map(Value::Number)
        .ok_or("the result is too large")
}

/// What differs between two briefs, as far as the music is concerned.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ChangeSet {
    /// Something every part depends on changed: tempo, meter, feel, length,
    /// tonality, style, motif, or the sequence and lengths of the sections.
    pub global: bool,
    /// Tracks added, removed, or changed in themselves. Each is affected in every section.
    pub tracks: BTreeSet<String>,
    /// Sections changed in themselves. Each is affected for every track.
    pub sections: BTreeSet<String>,
}

impl ChangeSet {
    /// True when nothing musical differs.
    pub fn is_empty(&self) -> bool {
        !self.global && self.tracks.is_empty() && self.sections.is_empty()
    }

    /// Whether one track's part in one section has to be composed again.
    pub fn affects(&self, track: &str, section: &str) -> bool {
        self.global || self.tracks.contains(track) || self.sections.contains(section)
    }
}

/// Compares two briefs. Titles, summaries, references and assumptions are not music
/// and never count.
pub fn changed_inputs(old: &Brief, new: &Brief) -> ChangeSet {
    let timeline = |brief: &Brief| -> Vec<(String, u16)> {
        brief
            .form
            .iter()
            .map(|section| (section.id.clone(), section.bars))
            .collect()
    };
    let global = old.identity.style != new.identity.style
        || old.identity.fusion != new.identity.fusion
        || old.time != new.time
        || old.tonality != new.tonality
        || old.motif != new.motif
        || timeline(old) != timeline(new);

    let mut tracks = BTreeSet::new();
    let find_track = |brief: &'_ Brief, id: &str| -> Option<TrackSpec> {
        brief.tracks.iter().find(|track| track.id == id).cloned()
    };
    for track in old.tracks.iter().chain(&new.tracks) {
        if find_track(old, &track.id) != find_track(new, &track.id) {
            tracks.insert(track.id.clone());
        }
    }

    let mut sections = BTreeSet::new();
    let find_section = |brief: &'_ Brief, id: &str| -> Option<Section> {
        brief.form.iter().find(|section| section.id == id).cloned()
    };
    for section in old.form.iter().chain(&new.form) {
        if find_section(old, &section.id) != find_section(new, &section.id) {
            sections.insert(section.id.clone());
        }
    }
    ChangeSet {
        global,
        tracks,
        sections,
    }
}
