use serde_json::{Value, json};
use t2m_core::brief::{
    Articulation, Brief, BriefParseError, HarmonicLanguage, Length, MAX_BRIEF_BYTES, MeterSpec,
    Register, Role, SectionHarmony, TonicSpec, Transition, brief_schema,
};

const EXAMPLE: &str = include_str!("fixtures/brief_example.json");

fn example() -> Value {
    serde_json::from_str(EXAMPLE).unwrap()
}

/// Reads a brief from a JSON value by way of its text, as the product does.
fn read(value: &Value) -> Result<Brief, BriefParseError> {
    Brief::from_json(&value.to_string())
}

/// Equality that treats `1` and `1.0` as the same number.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(key, p)| y.get(key).is_some_and(|q| same(p, q)))
        }
        _ => a == b,
    }
}

#[test]
fn t5_1_spec_example_round_trips() {
    let brief = Brief::from_json(EXAMPLE).unwrap();
    let again: Value = serde_json::from_str(&brief.to_json()).unwrap();
    assert!(same(&again, &example()), "re-serialized:\n{again:#}");
    assert_eq!(Brief::from_json(&brief.to_json()).unwrap(), brief);

    assert_eq!(brief.version, 1);
    assert_eq!(brief.identity.style, "hiphop.lofi");
    assert_eq!(brief.identity.references[0].interpreted_as.len(), 3);
    assert_eq!(brief.time.tempo_bpm, 82);
    assert_eq!(brief.time.length, Length::Bars(32));
    assert_eq!(brief.tonality.tonic, TonicSpec::Name("D".into()));
    assert_eq!(
        brief.tonality.harmonic_language,
        HarmonicLanguage::Functional
    );
    assert_eq!(brief.form.len(), 3);
    assert_eq!(brief.form[0].transition, Transition::FillLight);
    assert_eq!(brief.tracks[0].role, Role::Lead);
    assert_eq!(brief.tracks[0].register, Register::MidHigh);
    assert_eq!(brief.tracks[0].articulation, Articulation::Legato);
    assert_eq!(brief.motif.as_ref().unwrap().degrees, [5, 3, 2, 1, 0]);
    assert_eq!(brief.assumptions[0].confidence, 0.7);
}

#[test]
fn t5_2_every_variant_round_trips() {
    let mut value = example();
    value["time"]["meter"] = json!({ "kind": "cycle", "id": "tala.rupak" });
    value["time"]["length"] = json!({ "cycles": 24 });
    value["tonality"]["tonic"] = json!({ "hz": 146.83 });
    value["tonality"]["harmonic_language"] = json!("drone");
    value["form"][0]["harmony"] = json!({ "kind": "drone", "degrees": [1, 5] });
    value["form"][1]["harmony"] = json!({ "kind": "none" });
    let brief = read(&value).unwrap();
    assert_eq!(
        brief.time.meter,
        MeterSpec::Cycle {
            id: "tala.rupak".into()
        }
    );
    assert_eq!(brief.time.length, Length::Cycles(24));
    assert_eq!(brief.tonality.tonic, TonicSpec::Hz { hz: 146.83 });
    assert_eq!(brief.tonality.harmonic_language, HarmonicLanguage::Drone);
    assert_eq!(
        brief.form[0].harmony,
        SectionHarmony::Drone {
            degrees: vec![1, 5]
        }
    );
    assert_eq!(brief.form[1].harmony, SectionHarmony::None);
    assert_eq!(
        brief.form[2].harmony,
        SectionHarmony::Named {
            id: "prog.axis".into()
        }
    );
    let again: Value = serde_json::from_str(&brief.to_json()).unwrap();
    assert!(same(&again, &value), "re-serialized:\n{again:#}");

    value["time"]["length"] = json!({ "seconds": 90 });
    value["time"]["meter"] =
        json!({ "kind": "meter", "numerator": 7, "denominator": 8, "grouping": [2, 2, 3] });
    let brief = read(&value).unwrap();
    assert_eq!(brief.time.length, Length::Seconds(90));
    assert_eq!(
        brief.time.meter,
        MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: Some(vec![2, 2, 3])
        }
    );
    for language in ["functional", "modal", "drone", "none"] {
        value["tonality"]["harmonic_language"] = json!(language);
        let again: Value = serde_json::from_str(&read(&value).unwrap().to_json()).unwrap();
        assert_eq!(again["tonality"]["harmonic_language"], json!(language));
    }
    for role in [
        "lead",
        "counter",
        "chords",
        "pad",
        "bass",
        "drums",
        "percussion",
        "arp",
        "drone",
        "fx",
    ] {
        value["tracks"][0]["role"] = json!(role);
        let again: Value = serde_json::from_str(&read(&value).unwrap().to_json()).unwrap();
        assert_eq!(again["tracks"][0]["role"], json!(role));
    }
}

#[test]
fn t5_3_unknown_fields_ignored_and_optional_fields_default() {
    let mut value = example();
    value["surprise"] = json!({ "anything": [1, 2, 3] });
    value["identity"]["mood_ring"] = json!("teal");
    value["tracks"][0]["future_field"] = json!(true);
    assert_eq!(read(&value).unwrap(), Brief::from_json(EXAMPLE).unwrap());

    let top = value.as_object_mut().unwrap();
    top.remove("motif");
    top.remove("assumptions");
    let identity = value["identity"].as_object_mut().unwrap();
    identity.remove("fusion");
    identity.remove("references");
    identity.remove("title");
    identity.remove("summary");
    let track = value["tracks"][0].as_object_mut().unwrap();
    track.remove("register");
    track.remove("pattern");
    track.remove("articulation");
    let section = value["form"][0].as_object_mut().unwrap();
    section.remove("transition");
    section.remove("kind");
    value["time"]["meter"]
        .as_object_mut()
        .unwrap()
        .remove("grouping");
    value["assumptions"] = json!([{ "field": "/x", "because": "y" }]);
    value["identity"]["references"] = json!([{ "said": "someone" }]);

    let brief = read(&value).unwrap();
    assert_eq!(brief.motif, None);
    assert_eq!(brief.identity.fusion, None);
    assert_eq!(brief.identity.title, "");
    assert_eq!(brief.identity.summary, "");
    assert_eq!(brief.tracks[0].register, Register::Mid);
    assert_eq!(brief.tracks[0].pattern, "");
    assert_eq!(brief.tracks[0].articulation, Articulation::Normal);
    assert_eq!(brief.form[0].transition, Transition::None);
    assert_eq!(brief.form[0].kind, "");
    assert_eq!(brief.assumptions[0].confidence, 0.0);
    assert!(brief.identity.references[0].interpreted_as.is_empty());
    assert_eq!(
        brief.time.meter,
        MeterSpec::Meter {
            numerator: 4,
            denominator: 4,
            grouping: None
        }
    );

    value.as_object_mut().unwrap().remove("assumptions");
    value["identity"]
        .as_object_mut()
        .unwrap()
        .remove("references");
    let brief = read(&value).unwrap();
    assert!(brief.assumptions.is_empty());
    assert!(brief.identity.references.is_empty());
}

#[test]
fn t5_4_schema_matches_snapshot() {
    let schema = brief_schema();
    assert_eq!(schema["type"], json!("object"));
    for field in ["identity", "time", "tonality", "form", "tracks"] {
        assert!(
            schema["properties"][field].is_object(),
            "schema lacks {field}"
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!(field)),
            "{field} should be required"
        );
    }
    let text = schema.to_string();
    for word in [
        "progression",
        "fill_light",
        "percussion",
        "mid_high",
        "bars_per_chord",
    ] {
        assert!(text.contains(word), "schema lacks {word}");
    }
    insta::assert_json_snapshot!("brief_schema", schema);
}

#[test]
fn t5_5_hostile_text_is_a_clean_error() {
    let syntax = |text: &str| match Brief::from_json(text) {
        Err(BriefParseError::Syntax {
            line,
            column,
            message,
        }) => (line, column, message),
        other => panic!("expected a syntax error for {text:.40?}, got {other:?}"),
    };
    for text in [
        "",
        " ",
        "null",
        "[]",
        "42",
        "\"brief\"",
        "{",
        "{}",
        "{\"version\":1}",
    ] {
        syntax(text);
    }
    let (line, column, _) = syntax("{\n  \"version\": oops\n}");
    assert_eq!((line, column), (2, 14));
    let message = Brief::from_json("{\n  \"version\": oops\n}")
        .unwrap_err()
        .to_string();
    assert!(message.starts_with("line 2, column 14: "), "{message}");

    // Truncated.
    syntax(&EXAMPLE[..EXAMPLE.len() / 2]);
    // Wrong types and out-of-range numbers.
    let broken = |pointer: &str, replacement: Value| {
        let mut value = example();
        *value.pointer_mut(pointer).unwrap() = replacement;
        syntax(&value.to_string());
    };
    broken("/time/tempo_bpm", json!("fast"));
    broken("/time/tempo_bpm", json!(-5));
    broken("/time/tempo_bpm", json!(70_000));
    broken("/time/tempo_bpm", json!(82.5));
    broken("/version", json!(-1));
    broken("/form", json!("verse chorus"));
    broken("/tracks/0/role", json!("kazoo"));
    broken("/tonality/harmonic_language", json!("spicy"));
    broken("/form/0/transition", json!("explode"));
    broken("/form/0/harmony", json!({ "kind": "vibes" }));
    broken("/time/length", json!({ "parsecs": 12 }));
    broken("/tonality/tonic", json!(440));
    broken("/form/0/energy", json!("high"));
    syntax(&EXAMPLE.replace("82", "1e400"));
    // A raw NUL byte inside the text.
    syntax(&EXAMPLE.replace("Rain", "Ra\0in"));
    // Nesting far deeper than any brief.
    syntax(&format!("{}{}", "[".repeat(100_000), "]".repeat(100_000)));
    syntax(&format!("{{\"version\":{}", "{\"a\":".repeat(5_000)));

    // Oversized text is refused before parsing, at exactly one byte over the limit.
    let padding = " ".repeat(MAX_BRIEF_BYTES - EXAMPLE.len());
    let at_limit = format!("{EXAMPLE}{padding}");
    assert_eq!(at_limit.len(), MAX_BRIEF_BYTES);
    assert!(Brief::from_json(&at_limit).is_ok());
    let over = format!("{at_limit} ");
    assert_eq!(
        Brief::from_json(&over),
        Err(BriefParseError::TooLarge {
            bytes: MAX_BRIEF_BYTES + 1
        })
    );
    let huge = EXAMPLE.replace("Rain on Shibuya", &"x".repeat(5_000_000));
    let error = Brief::from_json(&huge).unwrap_err();
    assert!(matches!(error, BriefParseError::TooLarge { .. }));
    assert_eq!(
        error.to_string(),
        format!("the brief is {} bytes; the limit is 1000000", huge.len())
    );
    // Escaped control characters and emoji in text fields are data, not errors.
    let odd = EXAMPLE.replace("Rain on Shibuya", "🎹 \\u0000 \\\" ; rm -rf / \\n مرحبا");
    assert!(
        Brief::from_json(&odd)
            .unwrap()
            .identity
            .title
            .contains('🎹')
    );
}
