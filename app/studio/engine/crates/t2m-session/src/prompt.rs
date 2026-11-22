//! Turns a few words into a brief without any outside service: it finds the style the words
//! point at, starts from that style's example, and applies the tempo, key and mood it can read.
//! It is a plain rule-based reader, not a language model; whatever it guessed is listed in the
//! brief's assumptions.

use t2m_core::brief::{Assumption, Brief, TonicSpec};
use t2m_core::{Knowledge, repair};

/// The longest prompt read.
pub const MAX_PROMPT_CHARS: usize = 2_000;

/// Words that point at a style id, checked before the knowledge base's own search.
const STYLE_WORDS: &[(&str, &str)] = &[
    ("lofi", "hiphop.lofi"),
    ("lo-fi", "hiphop.lofi"),
    ("study", "hiphop.lofi"),
    ("chillhop", "hiphop.lofi"),
    ("trap", "hiphop.trap"),
    ("boombap", "hiphop.boombap"),
    ("boom-bap", "hiphop.boombap"),
    ("hiphop", "hiphop.boombap"),
    ("rap", "hiphop.boombap"),
    ("house", "electronic.house"),
    ("techno", "electronic.techno"),
    ("edm", "electronic.house"),
    ("funk", "funk.classic"),
    ("funky", "funk.classic"),
    ("jazz", "jazz.swing"),
    ("swing", "jazz.swing"),
    ("bossa", "latin.bossa"),
    ("reggae", "reggae.roots"),
    ("rock", "rock.classic"),
    ("cinematic", "cinematic.orchestral"),
    ("orchestral", "cinematic.orchestral"),
    ("epic", "cinematic.orchestral"),
    ("film", "cinematic.orchestral"),
    ("pop", "pop.generic"),
];

const SLOW: &[&str] = &[
    "slow",
    "slower",
    "relaxed",
    "mellow",
    "chill",
    "calm",
    "gentle",
    "laid-back",
];
const FAST: &[&str] = &[
    "fast",
    "faster",
    "upbeat",
    "energetic",
    "driving",
    "lively",
    "quick",
];
const MINOR: &[&str] = &[
    "sad",
    "dark",
    "melancholy",
    "melancholic",
    "moody",
    "minor",
    "somber",
    "haunting",
];
const MAJOR: &[&str] = &[
    "happy",
    "bright",
    "joyful",
    "joyous",
    "major",
    "cheerful",
    "sunny",
    "uplifting",
];

/// Why a prompt could not be used.
pub fn check(text: &str) -> Result<&str, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("describe the music in a few words first".to_string());
    }
    if text.chars().count() > MAX_PROMPT_CHARS {
        return Err(format!(
            "keep the description under {MAX_PROMPT_CHARS} characters"
        ));
    }
    Ok(text)
}

fn tokens(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '#' || c == '-' || c == '.'))
        .map(|w| w.trim_matches(['-', '.']).to_string())
        .filter(|w| !w.is_empty())
        .collect()
}

/// The whole words a style answers to: its id parts, name, aliases and tags.
fn style_names(style: &t2m_core::knowledge::Style) -> Vec<String> {
    let mut names: Vec<String> = style
        .id
        .to_lowercase()
        .split('.')
        .map(String::from)
        .collect();
    for text in std::iter::once(&style.name)
        .chain(&style.aliases)
        .chain(&style.tags)
    {
        names.extend(
            text.to_lowercase()
                .split(|c: char| !c.is_alphanumeric())
                .filter(|w| !w.is_empty())
                .map(String::from),
        );
    }
    names
}

/// A note name like `c`, `f#`, `eb`, normalised to `C`, `F#`, `Eb`.
fn note_name(word: &str) -> Option<String> {
    let mut chars = word.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    if !('A'..='G').contains(&letter) {
        return None;
    }
    let rest: String = chars.collect();
    match rest.as_str() {
        "" => Some(letter.to_string()),
        "#" | "b" => Some(format!("{letter}{rest}")),
        _ => None,
    }
}

/// The brief a prompt describes. `variant` picks among examples of the same style.
pub fn understand(
    kb: &Knowledge,
    examples: &[(String, Brief)],
    text: &str,
    variant: u64,
) -> Result<Brief, String> {
    let text = check(text)?;
    let raw = tokens(text);
    let ws: Vec<String> = raw.iter().map(|w| w.to_lowercase()).collect();
    let mut assumptions: Vec<Assumption> = Vec::new();
    let guess = |field: &str, because: String, confidence: f64| Assumption {
        field: field.to_string(),
        because,
        confidence,
    };

    // Style: a keyword, else a whole-word match in the knowledge base, else the default.
    let mut hits: Vec<String> = Vec::new();
    for w in &ws {
        if let Some((_, id)) = STYLE_WORDS.iter().find(|(k, _)| k == w)
            && kb.style(id).is_some()
            && !hits.iter().any(|h| h == id)
        {
            hits.push((*id).to_string());
        }
    }
    let mut style_id: Option<String> = hits.first().cloned();
    if hits.len() > 1 {
        assumptions.push(guess(
            "/identity/style",
            format!(
                "several styles were named ({}); the first, {}, was used",
                hits.join(", "),
                hits[0]
            ),
            0.4,
        ));
    }
    if style_id.is_none() {
        for w in &ws {
            if w.len() >= 3
                && let Some(style) = kb.search_styles(w, 1).first()
                && style_names(style).iter().any(|n| n == w)
            {
                assumptions.push(guess(
                    "/identity/style",
                    format!("\"{w}\" was read as the style {}", style.id),
                    0.5,
                ));
                style_id = Some(style.id.clone());
                break;
            }
        }
    }
    let style_id = match style_id {
        Some(id) => id,
        None => {
            let fallback = kb
                .default_style()
                .map(|s| s.id.clone())
                .ok_or("the built-in knowledge base has no styles")?;
            assumptions.push(guess(
                "/identity/style",
                format!("no style was recognised in the description, so {fallback} was used"),
                0.2,
            ));
            fallback
        }
    };

    let of_style: Vec<&(String, Brief)> = examples
        .iter()
        .filter(|(_, b)| b.identity.style == style_id)
        .collect();
    let pool: Vec<&(String, Brief)> = if of_style.is_empty() {
        examples.iter().collect()
    } else {
        of_style
    };
    if pool.is_empty() {
        return Err("there are no example pieces to start from".to_string());
    }
    let mut brief = pool[(variant % pool.len() as u64) as usize].1.clone();

    let title: String = text.chars().take(48).collect();
    let mut chars = title.chars();
    brief.identity.title = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => title,
    };
    brief.identity.summary = text.chars().take(200).collect();

    // Tempo: an explicit "N bpm", else a feel word scales the style's tempo.
    let explicit = ws
        .windows(2)
        .find_map(|p| {
            let (a, b) = (&p[0], &p[1]);
            if b == "bpm" {
                a.parse::<u16>().ok()
            } else {
                None
            }
        })
        .or_else(|| {
            ws.iter()
                .find_map(|w| w.strip_suffix("bpm").and_then(|n| n.parse::<u16>().ok()))
        });
    if let Some(bpm) = explicit.filter(|b| (30..=300).contains(b)) {
        brief.time.tempo_bpm = bpm;
    } else if ws.iter().any(|w| SLOW.contains(&w.as_str())) {
        brief.time.tempo_bpm = (u32::from(brief.time.tempo_bpm) * 3 / 4).clamp(30, 300) as u16;
        assumptions.push(guess(
            "/time/tempo_bpm",
            "the description sounds slow".to_string(),
            0.5,
        ));
    } else if ws.iter().any(|w| FAST.contains(&w.as_str())) {
        brief.time.tempo_bpm = (u32::from(brief.time.tempo_bpm) * 6 / 5).clamp(30, 300) as u16;
        assumptions.push(guess(
            "/time/tempo_bpm",
            "the description sounds fast".to_string(),
            0.5,
        ));
    }

    // Key and mode, only for the ordinary major/minor modes (never over a raga or maqam).
    let ordinary = brief.tonality.system.starts_with("mode.");
    let mut tonic: Option<String> = None;
    let mut mode: Option<&str> = None;
    for (i, w) in ws.iter().enumerate() {
        // "e flat minor" / "f sharp major": a letter, an optional flat or sharp, then the mode.
        let (name, after) = match (note_name(w), ws.get(i + 1).map(String::as_str)) {
            (Some(n), Some("flat")) if n.len() == 1 => (Some(format!("{n}b")), i + 2),
            (Some(n), Some("sharp")) if n.len() == 1 => (Some(format!("{n}#")), i + 2),
            (n, _) => (n, i + 1),
        };
        let next = ws.get(after).map(String::as_str);
        if let Some(name) = name
            && matches!(next, Some("major" | "minor"))
            && (w.len() > 1
                || after > i + 1
                || ws.get(i.wrapping_sub(1)).is_some_and(|p| p == "in")
                || raw[i].starts_with(|c: char| c.is_ascii_uppercase()))
        {
            tonic = Some(name);
            mode = Some(if next == Some("minor") {
                "mode.minor"
            } else {
                "mode.major"
            });
            break;
        }
    }
    let key_words = ws.iter().any(|w| w == "major" || w == "minor");
    if key_words && tonic.is_none() && ws.iter().any(|w| note_name(w).is_some() && w.len() > 1) {
        assumptions.push(guess(
            "/tonality/tonic",
            "a key was mentioned but not understood, so the example's key was kept".to_string(),
            0.3,
        ));
    }
    let mut mood_mode: Option<&str> = None;
    if mode.is_none() {
        if ws.iter().any(|w| MINOR.contains(&w.as_str())) {
            mood_mode = Some("mode.minor");
        } else if ws.iter().any(|w| MAJOR.contains(&w.as_str())) {
            mood_mode = Some("mode.major");
        }
    }
    if ordinary {
        if let Some(m) = mode.or(mood_mode)
            && kb.tonal_system(m).is_some()
        {
            if mood_mode.is_some() {
                assumptions.push(guess(
                    "/tonality/system",
                    format!(
                        "the mood words sound like {}",
                        if m == "mode.minor" { "minor" } else { "major" }
                    ),
                    0.5,
                ));
            }
            brief.tonality.system = m.to_string();
        }
        if let Some(t) = tonic {
            brief.tonality.tonic = TonicSpec::Name(t);
        }
    } else if mode.is_some() || mood_mode.is_some() || tonic.is_some() {
        assumptions.push(guess(
            "/tonality/system",
            format!(
                "this style's example uses {}, so the key and mood you named were not applied",
                brief.tonality.system
            ),
            0.9,
        ));
    }

    brief.assumptions.extend(assumptions);
    Ok(repair(brief, kb).0)
}
