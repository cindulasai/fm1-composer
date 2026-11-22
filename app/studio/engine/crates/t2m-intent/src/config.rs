//! Where the key and endpoint come from: environment variables, then a private file.

use std::path::Path;

pub const DEFAULT_BASE_URL: &str = "https://api.minimax.io/v1";
pub const DEFAULT_MODEL: &str = "MiniMax-M3.1-Flash";
const MAX_KEY_CHARS: usize = 512;

/// What is needed to call the model. The key never appears in `Debug` output.
#[derive(Clone, PartialEq, Eq)]
pub struct Config {
    pub key: String,
    pub base_url: String,
    pub model: String,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("key", &"<hidden>")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .finish()
    }
}

impl Config {
    /// Reads the settings. Each value comes from, in order: the environment, the private file
    /// `<home>/.config/text2midi/minimax.key` (the key only; first line), the nearest `.env` file from `cwd` upward to `home`,
    /// then `home`'s own. The names are `MINIMAX_API_KEY`, `MINIMAX_BASE_URL` (default
    /// `https://api.minimax.io/v1`) and `MINIMAX_MODEL`. `None` when there is no usable key or the
    /// address is not https.
    pub fn load(
        env: &dyn Fn(&str) -> Option<String>,
        read_file: &dyn Fn(&Path) -> Option<String>,
        home: &Path,
        cwd: &Path,
    ) -> Option<Config> {
        // The nearest `.env` wins: the working folder, then each folder above it up to (not
        // including) home, then home.
        let mut folders: Vec<&Path> = if cwd.starts_with(home) {
            cwd.ancestors().take_while(|dir| *dir != home).collect()
        } else {
            vec![cwd]
        };
        folders.push(home);
        let dotenvs: Vec<String> = folders
            .iter()
            .filter_map(|dir| read_file(&dir.join(".env")))
            .collect();
        let setting = |name: &str| -> Option<String> {
            env(name).or_else(|| dotenvs.iter().find_map(|text| dotenv_value(text, name)))
        };
        let from_file = || read_file(&home.join(".config/text2midi/minimax.key"));
        let key = env("MINIMAX_API_KEY")
            .or_else(from_file)
            .or_else(|| setting("MINIMAX_API_KEY"))?;
        let key = key.lines().next().unwrap_or("").trim().to_string();
        if key.is_empty()
            || key.chars().count() > MAX_KEY_CHARS
            || key.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return None;
        }
        let base_url = setting("MINIMAX_BASE_URL")
            .map(|u| u.trim().trim_end_matches('/').to_string())
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
        if !base_url.starts_with("https://") || base_url.chars().any(|c| c.is_whitespace()) {
            return None;
        }
        let model = setting("MINIMAX_MODEL")
            .map(|m| m.trim().to_string())
            .filter(|m| !m.is_empty() && !m.chars().any(|c| c.is_whitespace()))
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        Some(Config {
            key,
            base_url,
            model,
        })
    }

    /// The chat endpoint.
    pub fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

/// The value of `NAME=value` in `.env` text: comments and blank lines are skipped, an optional
/// `export ` is allowed, matching quotes are removed. The first uncommented line wins.
pub(crate) fn dotenv_value(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if key.trim() != name {
            continue;
        }
        let value = value.trim();
        let value = match (value.chars().next(), value.chars().last()) {
            (Some(q @ ('"' | '\'')), Some(end)) if q == end && value.len() >= 2 => {
                &value[1..value.len() - 1]
            }
            _ => value,
        };
        return Some(value.to_string());
    }
    None
}
