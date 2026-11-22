//! Turns a description into a brief by asking a language model, then checking the answer.
//!
//! No network code lives here: a `Complete` provider does the talking (the app supplies one
//! for MiniMax). This crate builds the request, reads the reply, pulls the brief out of it,
//! repairs and validates it, and asks once more when the first answer was unusable.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod config;
mod reply;
mod request;

pub use config::{Config, DEFAULT_BASE_URL, DEFAULT_MODEL};
pub use reply::{extract_json, parse_response};
pub use request::{build_request, system_prompt};

use t2m_core::{Brief, Knowledge, repair, validate};

/// The longest description sent.
pub const MAX_TEXT_CHARS: usize = 2_000;

/// Something that can answer a chat request: a system message and a user message in, text out.
pub trait Complete {
    fn complete(&self, system: &str, user: &str) -> Result<String, IntentError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IntentError {
    #[error("describe the music in a few words first")]
    Empty,
    #[error("keep the description under {MAX_TEXT_CHARS} characters")]
    TooLong,
    #[error("the language model could not be reached: {0}")]
    Network(String),
    #[error("the language model refused the request: {0}")]
    Service(String),
    #[error("the language model's answer could not be used: {0}")]
    BadAnswer(String),
}

/// Asks `model` for a brief matching `text`. The answer is repaired and validated; if the
/// first answer is unusable the model is told what was wrong and asked once more.
pub fn brief_from_prompt(
    model: &dyn Complete,
    kb: &Knowledge,
    text: &str,
) -> Result<Brief, IntentError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(IntentError::Empty);
    }
    if text.chars().count() > MAX_TEXT_CHARS {
        return Err(IntentError::TooLong);
    }
    let system = system_prompt(kb);
    let first = model.complete(&system, text)?;
    match read_brief(&first, kb) {
        Ok(brief) => Ok(brief),
        Err(problem) => {
            let again = format!(
                "{text}\n\nYour previous answer could not be used ({problem}). Answer again with only the JSON brief."
            );
            let second = model.complete(&system, &again)?;
            read_brief(&second, kb).map_err(IntentError::BadAnswer)
        }
    }
}

fn read_brief(answer: &str, kb: &Knowledge) -> Result<Brief, String> {
    let json = extract_json(answer).ok_or("no JSON object found")?;
    let brief = Brief::from_json(json).map_err(|e| e.to_string())?;
    let (brief, _issues) = repair(brief, kb);
    validate(&brief, kb).map_err(|issues| {
        issues
            .iter()
            .take(3)
            .map(|i| format!("{} {:?}: {}", i.path, i.kind, i.was))
            .collect::<Vec<_>>()
            .join("; ")
    })?;
    Ok(brief)
}

#[cfg(test)]
#[path = "intent_tests.rs"]
mod tests;
