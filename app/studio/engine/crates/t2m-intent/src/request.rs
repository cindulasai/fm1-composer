//! What is sent to the model.

use serde_json::{Value, json};
use t2m_core::{Knowledge, brief_schema};

/// The instructions: what a brief is, the schema, and the names the model may use.
pub fn system_prompt(kb: &Knowledge) -> String {
    let mut styles = String::new();
    for style in kb.styles() {
        styles.push_str(&format!(
            "- {} ({}): tempo {}-{} default {}, meter {}/{}, feel {}, system {}\n",
            style.id,
            style.name,
            style.tempo.min,
            style.tempo.max,
            style.tempo.default,
            style.meter.0,
            style.meter.1,
            style.feel,
            style.system
        ));
    }
    let grooves: Vec<String> = kb.grooves().map(|g| g.id.clone()).collect();
    format!(
        "You turn a short description of a piece of music into a JSON brief for a composition engine.\n\
Reply with ONLY one JSON object that follows this JSON Schema, with no commentary and no code fences.\n\
Rules: use only style ids and groove ids from the lists below; keep tempo inside the style's range \
unless the user asked for another; every section's `tracks` must name tracks that exist in `tracks`; \
put anything you guessed rather than were told in `assumptions` with a reason; do not invent facts about \
real artists, and record what you took from any reference in `identity.references`.\n\n\
Styles:\n{styles}\nGrooves: {}\n\nSchema:\n{}",
        grooves.join(", "),
        brief_schema()
    )
}

/// The chat request body.
pub fn build_request(model: &str, system: &str, user: &str) -> Value {
    json!({
        "model": model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "max_completion_tokens": 8000,
        "reasoning_split": true,
    })
}
