//! Reading what the model sent back.

use crate::IntentError;
use serde_json::Value;

/// The text of the first choice in a chat reply, or why there is none.
pub fn parse_response(body: &str) -> Result<String, IntentError> {
    let value: Value = serde_json::from_str(body)
        .map_err(|_| IntentError::BadAnswer("the reply was not JSON".to_string()))?;
    if let Some(message) = value.pointer("/error/message").and_then(Value::as_str) {
        return Err(IntentError::Service(message.chars().take(300).collect()));
    }
    if let Some(code) = value
        .pointer("/base_resp/status_code")
        .and_then(Value::as_i64)
        && code != 0
    {
        let msg = value
            .pointer("/base_resp/status_msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown error");
        return Err(IntentError::Service(format!(
            "{code}: {}",
            msg.chars().take(300).collect::<String>()
        )));
    }
    value
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| IntentError::BadAnswer("the reply had no text".to_string()))
}

/// The JSON object inside a model's text: `<think>` blocks and code fences are dropped,
/// then the text from the first `{` to the matching `}`.
pub fn extract_json(text: &str) -> Option<&str> {
    let mut text = text;
    // Drop a leading <think>…</think> block (a model that shows its reasoning inline).
    if let Some(end) = text.rfind("</think>") {
        text = &text[end + "</think>".len()..];
    }
    let start = text.find('{')?;
    let bytes = text.as_bytes();
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for (offset, &b) in bytes[start..].iter().enumerate() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}
