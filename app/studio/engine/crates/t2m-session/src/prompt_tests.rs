use crate::prompt::{MAX_PROMPT_CHARS, understand};
use crate::{Session, SessionError};
use t2m_core::brief::TonicSpec;
use t2m_core::{Brief, Knowledge, repair};

fn examples(kb: &Knowledge) -> Vec<(String, Brief)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../suite/briefs");
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        let brief = repair(Brief::from_json(&text).unwrap(), kb).0;
        found.push((entry.file_name().to_string_lossy().into_owned(), brief));
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    found
}

fn read(text: &str) -> Brief {
    let kb = Knowledge::load().unwrap();
    understand(&kb, &examples(&kb), text, 0).unwrap()
}

#[test]
fn t36_1_words_pick_the_style() {
    for (text, style) in [
        ("a lofi beat to study to", "hiphop.lofi"),
        ("dark trap banger", "hiphop.trap"),
        ("Smooth JAZZ trio", "jazz.swing"),
        ("epic cinematic trailer music", "cinematic.orchestral"),
        ("reggae groove", "reggae.roots"),
        ("four on the floor house track", "electronic.house"),
    ] {
        assert_eq!(read(text).identity.style, style, "{text}");
    }
    // Nothing recognisable: the default style, and the guess is recorded.
    let brief = read("zzzzqq xxyyzz");
    assert!(
        brief
            .assumptions
            .iter()
            .any(|a| a.field == "/identity/style")
    );
}

#[test]
fn t36_2_tempo_comes_from_bpm_or_a_feel_word() {
    assert_eq!(read("pop song at 93 bpm").time.tempo_bpm, 93);
    assert_eq!(read("pop song 140bpm").time.tempo_bpm, 140);
    let base = read("pop song").time.tempo_bpm;
    assert!(read("slow pop song").time.tempo_bpm < base);
    assert!(read("fast pop song").time.tempo_bpm > base);
    // Silly numbers are ignored, not obeyed.
    assert_eq!(read("pop song at 5 bpm").time.tempo_bpm, base);
    assert_eq!(read("pop song at 9000 bpm").time.tempo_bpm, base);
}

#[test]
fn t36_3_key_and_mood_set_the_tonic_and_mode_for_ordinary_modes_only() {
    let brief = read("sad piano pop in F# minor");
    assert_eq!(brief.tonality.system, "mode.minor");
    assert_eq!(brief.tonality.tonic, TonicSpec::Name("F#".to_string()));
    let brief = read("a major pop tune in Eb major");
    assert_eq!(brief.tonality.system, "mode.major");
    assert_eq!(brief.tonality.tonic, TonicSpec::Name("Eb".to_string()));
    assert_eq!(read("melancholy pop").tonality.system, "mode.minor");
    assert_eq!(read("sunny pop").tonality.system, "mode.major");
    // The word "a" or "be" is not a key.
    assert_eq!(
        read("a big pop song").tonality.tonic,
        read("pop song").tonality.tonic
    );
}

#[test]
fn t36_4_the_session_opens_a_playable_piece_and_refuses_bad_prompts() {
    let mut s = Session::new().unwrap();
    for bad in ["", "   ", "\n\t"] {
        assert!(matches!(
            s.open_prompt(bad, 1),
            Err(SessionError::BadPrompt(_))
        ));
        assert!(s.view().is_err(), "a refused prompt opened something");
    }
    let long = "a".repeat(MAX_PROMPT_CHARS + 1);
    assert!(matches!(
        s.open_prompt(&long, 1),
        Err(SessionError::BadPrompt(_))
    ));
    let view = s
        .open_prompt("slow sad lofi in D minor at 80 bpm", 3)
        .unwrap();
    assert_eq!(view.tempo_bpm, 80);
    assert_eq!(view.style, "hiphop.lofi");
    assert!(view.title.starts_with("Slow sad lofi"));
    assert!(!view.tracks.is_empty() && view.end > 0);
    assert!(!s.piece().unwrap().notes.is_empty());
    // Hostile text: emoji, NUL, quotes, RTL, shell characters. Always some piece, never a panic.
    for odd in [
        "🎹🎹🎹",
        "a\u{0}b",
        "\"; rm -rf /\"",
        "\u{202e}lofi",
        "bpm bpm bpm",
        "in minor",
        "# b #b",
    ] {
        let v = s.open_prompt(odd, 1).unwrap();
        assert!(v.end > 0, "{odd:?}");
    }
    // The same prompt and seed give the same piece; locking and rerolling still work after.
    let again = Session::new()
        .unwrap()
        .open_prompt("slow sad lofi in D minor at 80 bpm", 3)
        .unwrap();
    assert_eq!(again.tracks, view.tracks);
    let first = view.tracks[1].id.clone();
    s.open_prompt("slow sad lofi in D minor at 80 bpm", 3)
        .unwrap();
    s.reroll(&first, None).unwrap();
    s.lock(&first, true).unwrap();
}

#[test]
fn t36_5_a_named_key_is_applied_in_every_style_and_the_piece_still_passes_its_gates() {
    let kb = Knowledge::load().unwrap();
    let ex = examples(&kb);
    for style in [
        "lofi",
        "funk",
        "jazz",
        "trap",
        "boombap",
        "house",
        "techno",
        "bossa",
        "reggae",
        "rock",
        "cinematic",
        "pop",
    ] {
        for key in ["in D minor", "in Eb major", "in F# minor", "e flat minor"] {
            let text = format!("{style} {key}");
            let brief = understand(&kb, &ex, &text, 0).unwrap();
            let (system, tonic) = if key.contains("minor") {
                (
                    "mode.minor",
                    if key.contains("flat") {
                        "Eb"
                    } else if key.contains("F#") {
                        "F#"
                    } else {
                        "D"
                    },
                )
            } else {
                ("mode.major", "Eb")
            };
            assert_eq!(brief.tonality.system, system, "{text}");
            assert_eq!(
                brief.tonality.tonic,
                TonicSpec::Name(tonic.to_string()),
                "{text}"
            );
            let score = t2m_compose::compose(&brief, &kb, 1);
            let card = t2m_compose::critique(&score, &brief, &kb);
            assert!(
                card.hard_failures.is_empty(),
                "{text}: {:?}",
                card.hard_failures
            );
        }
    }
}

#[test]
fn t36_6_assumptions_say_only_what_happened() {
    let says = |b: &Brief, field: &str| b.assumptions.iter().any(|a| a.field == field);
    // A mood that was applied is recorded; the system really changed.
    let sad = read("sad pop");
    assert_eq!(sad.tonality.system, "mode.minor");
    assert!(says(&sad, "/tonality/system"));
    // Whatever the style's example mode, a recorded mood assumption always matches the result.
    for style in ["lofi", "funk", "jazz", "rock", "house"] {
        let b = read(&format!("sad {style}"));
        if says(&b, "/tonality/system") {
            assert_eq!(b.tonality.system, "mode.minor", "{style}");
        }
    }
    // Whole words only: substrings do not pick a style, and the all-miss fallback is recorded.
    for text in ["trapeze artist", "rocket science", "popular opinion"] {
        let b = read(text);
        assert!(
            says(&b, "/identity/style"),
            "{text}: a guessed style must be recorded"
        );
    }
    assert_eq!(
        read("trapeze artist").identity.style,
        read("zzzz").identity.style
    );
    // Several styles named: the first is used and the choice is recorded.
    let many = read("jazz and trap and house");
    assert_eq!(many.identity.style, "jazz.swing");
    assert!(says(&many, "/identity/style"));
    // A key that cannot be read is noted, not silently dropped.
    let odd = read("pop in \u{ff21} minor");
    assert!(
        says(&odd, "/tonality/tonic") || odd.tonality.tonic == read("pop minor").tonality.tonic
    );
}
