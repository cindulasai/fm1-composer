use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use t2m_core::knowledge::{Kind, Knowledge, KnowledgeError};

/// A throwaway copy of the embedded data directory that tests can damage.
struct TempKb {
    root: PathBuf,
}

impl TempKb {
    /// A copy of the whole embedded data directory.
    fn full() -> TempKb {
        let kb = TempKb::new();
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        copy_dir(&source, &kb.root);
        kb
    }

    fn new() -> TempKb {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let unique = format!(
            "t2m-kb-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        );
        let root = std::env::temp_dir().join(unique);
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        copy_dir(&source, &root);
        // These tests were written for a small seed; the Western styles are taken out of the copy.
        for gone in [
            "styles/western.ron",
            "progressions/western.ron",
            "instruments/gm_extra.ron",
            "cycles/tala.ron",
            "tonal/thaat.ron",
            "cycles/timeline.ron",
            "grooves/timeline.ron",
        ] {
            let _ = fs::remove_file(root.join(gone));
        }
        TempKb { root }
    }

    fn write(&self, relative: &str, text: &str) -> &TempKb {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
        self
    }

    fn replace(&self, relative: &str, from: &str, to: &str) -> &TempKb {
        let path = self.root.join(relative);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{relative} lacks {from:?}");
        fs::write(path, text.replacen(from, to, 1)).unwrap();
        self
    }

    fn load(&self) -> Result<Knowledge, KnowledgeError> {
        Knowledge::from_dir(&self.root)
    }

    fn error(&self) -> KnowledgeError {
        self.load()
            .expect_err("the damaged knowledge base should not load")
    }
}

impl Drop for TempKb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

const RUPAK: &str = r#"[(id: "tala.rupak", beats: 7, beat_unit: 4, grouping: [3, 2, 2],
    strokes: [(name: "tin"), (name: "tin"), (name: "na"), (name: "dhin"), (name: "na"), (name: "dhin"), (name: "na")],
    accents: [(beat: 3, weight: 2)], source: "test fixture", verified: false)]"#;

const EXTRA_STYLES: &str = r#"[
    (id: "rock.test", name: "Rock", aliases: ["rock n roll"], tags: ["guitar", "pop"],
     tempo: (min: 100, max: 160, default: 120), meter: (4, 4), feel: "groove.straight_8",
     system: "mode.minor", harmonic_language: functional,
     form: [(id: "a", kind: "verse", bars: 8, energy: 0.5, density: 0.5)],
     palette: [(role: drums, instrument: "kit.standard")], source: "test fixture", verified: false),
    (id: "hiphop.lofi", name: "Lo-fi hip hop", aliases: ["chillhop", "lo-fi beats"], tags: ["mellow"],
     tempo: (min: 70, max: 95, default: 82), meter: (4, 4), feel: "groove.straight_8",
     system: "mode.dorian", harmonic_language: modal,
     form: [(id: "a", kind: "loop", bars: 8, energy: 0.4, density: 0.4)],
     palette: [(role: drums, instrument: "kit.standard")], source: "test fixture", verified: false),
    (id: "electronic.house", name: "House", tags: ["dance", "four on the floor"],
     tempo: (min: 118, max: 130, default: 124), meter: (4, 4), feel: "groove.straight_8",
     system: "mode.minor", harmonic_language: modal,
     form: [(id: "a", kind: "loop", bars: 8, energy: 0.8, density: 0.6)],
     palette: [(role: drums, instrument: "kit.standard")], source: "test fixture", verified: false),
]"#;

fn with_extras() -> (TempKb, Knowledge) {
    let temp = TempKb::new();
    temp.write("styles/extra.ron", EXTRA_STYLES)
        .write("cycles/talas.ron", RUPAK);
    let kb = temp.load().unwrap();
    (temp, kb)
}

fn ids(styles: Vec<&t2m_core::Style>) -> Vec<&str> {
    styles.into_iter().map(|style| style.id.as_str()).collect()
}

#[test]
fn t6_1_embedded_seed_loads_and_lookups_work() {
    let kb = Knowledge::load().unwrap();
    let pop = kb.style("pop.generic").unwrap();
    assert_eq!(pop.name, "Pop");
    assert_eq!(pop.tempo.default, 116);
    assert_eq!(pop.form.len(), 8);
    assert!(kb.style("nope").is_none());
    for id in ["mode.major", "mode.minor", "mode.dorian"] {
        assert_eq!(kb.tonal_system(id).unwrap().degrees(), 7, "{id}");
    }
    assert!(kb.tonal_system("nope").is_none());
    assert_eq!(kb.tuning("12tet").unwrap().cents.len(), 12);
    assert!(kb.tuning("nope").is_none());
    assert_eq!(
        kb.groove("groove.straight_8").unwrap().name,
        "Straight eighths"
    );
    assert!(kb.groove("nope").is_none());
    let piano = kb.instrument("keys.piano").unwrap();
    assert_eq!((piano.gm_program, piano.low, piano.high), (0, 21, 108));
    assert!(!piano.is_kit());
    let bass = kb.instrument("bass.electric").unwrap();
    assert_eq!((bass.gm_program, bass.low, bass.high), (33, 28, 63));
    let kit = kb.instrument("kit.standard").unwrap();
    assert!(kit.is_kit());
    let note = |name: &str| {
        kit.kit
            .iter()
            .find(|piece| piece.name == name)
            .unwrap()
            .note
    };
    assert_eq!(
        (note("kick"), note("snare"), note("closed_hat")),
        (36, 38, 42)
    );
    assert!(kb.instrument("nope").is_none());
    assert_eq!(
        kb.progression("prog.axis").unwrap().roman,
        ["I", "V", "vi", "IV"]
    );
    assert!(kb.progression("nope").is_none());
    assert!(kb.cycle("nope").is_none());
    assert_eq!(kb.gallery().len(), 1);
    assert_eq!(kb.gallery()[0].id, "pop-starter");
    assert_eq!(kb.gallery()[0].brief.identity.style, "pop.generic");
    assert_eq!(kb.styles().count(), 12);
    assert_eq!(kb.instruments().count(), 15);
    assert_eq!(kb.grooves().count(), 14);

    // The embedded data equals the data directory read from disk.
    let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
    assert_eq!(Knowledge::from_dir(&data).unwrap(), kb);

    let (_temp, extended) = with_extras();
    let rupak = extended.cycle("tala.rupak").unwrap();
    assert_eq!((rupak.beats, rupak.strokes.len()), (7, 7));
    assert_eq!(
        ids(extended.styles().collect()),
        [
            "electronic.house",
            "hiphop.lofi",
            "pop.generic",
            "rock.test"
        ]
    );
}

#[test]
fn t6_2_search_ranks_exact_then_tag_then_contained_then_near() {
    let (_temp, kb) = with_extras();
    let search = |text: &str, limit: usize| ids(kb.search_styles(text, limit));

    // Exact name first, then the style tagged "pop".
    assert_eq!(search("Pop", 5), ["pop.generic", "rock.test"]);
    assert_eq!(search("pop.generic", 5)[0], "pop.generic");
    assert_eq!(search("generic", 5), ["pop.generic"]);
    // Case, spaces and punctuation are ignored; aliases count as names.
    assert_eq!(search("LOFI hiphop", 5), ["hiphop.lofi"]);
    assert_eq!(search("  chill-hop ", 5), ["hiphop.lofi"]);
    assert_eq!(search("Mainstream Pop!", 5)[0], "pop.generic");
    // A tag alone finds the style.
    assert_eq!(search("dance", 5), ["electronic.house"]);
    // One containing the other.
    assert_eq!(search("hip hop", 5), ["hiphop.lofi"]);
    assert_eq!(search("deep house music", 5), ["electronic.house"]);
    // A near spelling.
    assert_eq!(search("huose", 5), ["electronic.house"]);
    // An exact match outranks a contained one, which outranks a tag-free near one.
    assert_eq!(search("rock", 5)[0], "rock.test");
    assert_eq!(search("roc", 5), ["rock.test"]);
    // Limits.
    assert_eq!(search("pop", 1), ["pop.generic"]);
    assert!(search("pop", 0).is_empty());
    // Nothing to match on, or nothing alike.
    for text in ["", "   ", "!!!", "zzzzqqqq", "x"] {
        assert!(search(text, 5).is_empty(), "query {text:?}");
    }

    assert_eq!(kb.nearest(Kind::Style, "lo-fi beats"), Some("hiphop.lofi"));
    assert_eq!(kb.nearest(Kind::Instrument, "Piano"), Some("keys.piano"));
    assert_eq!(
        kb.nearest(Kind::Instrument, "bass guitar"),
        Some("bass.electric")
    );
    assert_eq!(
        kb.nearest(Kind::Instrument, "drum kit"),
        Some("kit.standard")
    );
    assert_eq!(kb.nearest(Kind::TonalSystem, "Dorian"), Some("mode.dorian"));
    assert_eq!(
        kb.nearest(Kind::TonalSystem, "natural minor"),
        Some("mode.minor")
    );
    assert_eq!(
        kb.nearest(Kind::TonalSystem, "mode.majr"),
        Some("mode.major")
    );
    assert_eq!(kb.nearest(Kind::Tuning, "12-TET"), Some("12tet"));
    assert_eq!(
        kb.nearest(Kind::Groove, "straight"),
        Some("groove.straight_8")
    );
    assert_eq!(kb.nearest(Kind::Progression, "axis"), Some("prog.axis"));
    assert_eq!(kb.nearest(Kind::Cycle, "rupak"), Some("tala.rupak"));
    assert_eq!(kb.nearest(Kind::Cycle, "tala.rupak"), Some("tala.rupak"));
    assert_eq!(kb.nearest(Kind::Instrument, "theremin"), None);
    assert_eq!(kb.nearest(Kind::Instrument, ""), None);
}

#[test]
fn t6_2_short_text_only_matches_whole_names() {
    let temp = TempKb::new();
    // "ab" is two characters: too short to match by being contained in a query.
    temp.replace("styles/pop.ron", "aliases: [", "aliases: [\"ab\", ");
    let kb = temp.load().unwrap();
    assert_eq!(ids(kb.search_styles("ab", 5)), ["pop.generic"]);
    assert!(kb.search_styles("xxabxx", 5).is_empty());
    // Three characters are enough.
    temp.replace("styles/pop.ron", "\"ab\", ", "\"abc\", ");
    let kb = temp.load().unwrap();
    assert_eq!(ids(kb.search_styles("xxabcxx", 5)), ["pop.generic"]);
    // A two-character query is not matched by being inside a name.
    assert!(kb.search_styles("bc", 5).is_empty());
    assert_eq!(ids(kb.search_styles("mai", 5)), ["pop.generic"]);
}

#[test]
fn t6_3_damaged_data_gives_specific_errors() {
    let missing = |from: &str, to: &str| KnowledgeError::MissingRef {
        from: from.into(),
        to: to.into(),
    };
    let damaged = |relative: &str, from: &str, to: &str| {
        let temp = TempKb::new();
        temp.replace(relative, from, to);
        temp.error()
    };

    // Dangling references, one per kind of link.
    assert_eq!(
        damaged(
            "styles/pop.ron",
            "feel: \"groove.straight_8\"",
            "feel: \"groove.nope\""
        ),
        missing("pop.generic", "groove.nope")
    );
    assert_eq!(
        damaged(
            "styles/pop.ron",
            "system: \"mode.major\"",
            "system: \"mode.nope\""
        ),
        missing("pop.generic", "mode.nope")
    );
    assert_eq!(
        damaged("styles/pop.ron", "[\"prog.axis\"]", "[\"prog.nope\"]"),
        missing("pop.generic", "prog.nope")
    );
    assert_eq!(
        damaged(
            "styles/pop.ron",
            "instrument: \"bass.electric\"",
            "instrument: \"bass.nope\""
        ),
        missing("pop.generic", "bass.nope")
    );
    assert_eq!(
        damaged(
            "tonal/modes.ron",
            "default_tuning: \"12tet\"",
            "default_tuning: \"nope\""
        ),
        missing("mode.major", "nope")
    );
    assert_eq!(
        damaged(
            "tonal/modes.ron",
            "aliases: [\"dorian\"]",
            "tunings: [\"nope\"]"
        ),
        missing("mode.dorian", "nope")
    );
    assert_eq!(
        damaged(
            "grooves/basic.ron",
            "aliases:",
            "cycles: [\"tala.nope\"], aliases:"
        ),
        missing("groove.straight_8", "tala.nope")
    );
    let gallery = "gallery/pop-starter.json";
    let from = "gallery/pop-starter";
    assert_eq!(
        damaged(
            gallery,
            "\"style\": \"pop.generic\"",
            "\"style\": \"pop.nope\""
        ),
        missing(from, "pop.nope")
    );
    assert_eq!(
        damaged(
            gallery,
            "\"style\": \"pop.generic\"",
            "\"style\": \"pop.generic\", \"fusion\": \"x\""
        ),
        missing(from, "x")
    );
    assert_eq!(
        damaged(
            gallery,
            "\"feel\": \"groove.straight_8\"",
            "\"feel\": \"groove.nope\""
        ),
        missing(from, "groove.nope")
    );
    assert_eq!(
        damaged(
            gallery,
            "\"system\": \"mode.major\"",
            "\"system\": \"mode.nope\""
        ),
        missing(from, "mode.nope")
    );
    assert_eq!(
        damaged(gallery, "\"tuning\": \"12tet\"", "\"tuning\": \"nope\""),
        missing(from, "nope")
    );
    assert_eq!(
        damaged(
            gallery,
            "\"instrument\": \"bass.electric\"",
            "\"instrument\": \"bass.nope\""
        ),
        missing(from, "bass.nope")
    );
    assert_eq!(
        damaged(
            gallery,
            "{ \"kind\": \"named\", \"id\": \"prog.axis\" }",
            "{ \"kind\": \"named\", \"id\": \"prog.nope\" }"
        ),
        missing(from, "prog.nope")
    );
    assert_eq!(
        damaged(
            gallery,
            "{ \"kind\": \"meter\", \"numerator\": 4, \"denominator\": 4 }",
            "{ \"kind\": \"cycle\", \"id\": \"tala.nope\" }"
        ),
        missing(from, "tala.nope")
    );

    // A system that does not fit its tuning table.
    assert_eq!(
        damaged(
            "tonal/modes.ron",
            "steps: [0, 2, 3, 5, 7, 9, 10]",
            "steps: [0, 2, 3, 5, 7, 9, 12]"
        ),
        KnowledgeError::DegreeMismatch {
            system: "mode.dorian".into(),
            tuning: "12tet".into()
        }
    );
    // An alternative tuning with a table of another size.
    let temp = TempKb::new();
    temp.write(
        "tunings/other.ron",
        r#"[(id: "5tet", cents: [0, 240, 480, 720, 960], period: 1200, source: "s", verified: false)]"#,
    )
    .replace("tonal/modes.ron", "aliases: [\"dorian\"]", "tunings: [\"5tet\"]");
    assert_eq!(
        temp.error(),
        KnowledgeError::DegreeMismatch {
            system: "mode.dorian".into(),
            tuning: "5tet".into()
        }
    );
    // An alternative tuning of the same size is accepted.
    let temp = TempKb::new();
    temp.write(
        "tunings/other.ron",
        r#"[(id: "12well", cents: [0, 90, 196, 294, 390, 498, 588, 698, 792, 893, 996, 1092], period: 1200, source: "s", verified: false)]"#,
    )
    .replace("tonal/modes.ron", "aliases: [\"dorian\"]", "tunings: [\"12well\"]");
    assert_eq!(
        temp.load()
            .unwrap()
            .tonal_system("mode.dorian")
            .unwrap()
            .tunings,
        ["12well"]
    );

    // A stroke pattern of the wrong length.
    let temp = TempKb::new();
    temp.write(
        "cycles/talas.ron",
        &RUPAK.replacen("(name: \"tin\"), ", "", 1),
    );
    assert_eq!(
        temp.error(),
        KnowledgeError::StrokeLength {
            cycle: "tala.rupak".into(),
            expected: 7,
            got: 6
        }
    );

    // The same id twice, in one file or across files.
    let temp = TempKb::new();
    temp.write(
        "tunings/again.ron",
        r#"[(id: "12tet", cents: [0], period: 1200, source: "s", verified: false)]"#,
    );
    assert_eq!(
        temp.error(),
        KnowledgeError::DuplicateId {
            kind: "tuning",
            id: "12tet".into()
        }
    );
    let duplicates = [
        (
            "styles/extra.ron",
            EXTRA_STYLES.replace("rock.test", "hiphop.lofi"),
            "style",
            "hiphop.lofi",
        ),
        (
            "cycles/talas.ron",
            format!(
                "{}, (id: \"tala.rupak\", beats: 1, beat_unit: 4, grouping: [1], source: \"s\", verified: false)]",
                RUPAK.trim_end_matches(']')
            ),
            "cycle",
            "tala.rupak",
        ),
    ];
    for (file, text, kind, id) in duplicates {
        let temp = TempKb::new();
        temp.write(file, &text);
        assert_eq!(
            temp.error(),
            KnowledgeError::DuplicateId {
                kind,
                id: id.into()
            }
        );
    }
    for (file, kind, id) in [
        ("tonal/modes.ron", "system", "mode.major"),
        ("grooves/basic.ron", "groove", "groove.straight_8"),
        ("instruments/gm.ron", "instrument", "keys.piano"),
        ("progressions/pop.ron", "progression", "prog.axis"),
    ] {
        let temp = TempKb::new();
        let text = fs::read_to_string(temp.root.join(file)).unwrap();
        temp.write(&file.replace(".ron", "2.ron"), &text);
        assert_eq!(
            temp.error(),
            KnowledgeError::DuplicateId {
                kind,
                id: id.into()
            },
            "{file}"
        );
    }

    // Syntax errors name the file.
    let temp = TempKb::new();
    temp.write("grooves/broken.ron", "[ (id: ");
    match temp.error() {
        KnowledgeError::Parse { file, message } => {
            assert_eq!(file, "grooves/broken.ron");
            assert!(!message.is_empty());
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
    let temp = TempKb::new();
    temp.write("gallery/broken.json", "{ \"version\": ");
    match temp.error() {
        KnowledgeError::Parse { file, message } => {
            assert_eq!(file, "gallery/broken.json");
            assert!(message.starts_with("line 1, column"), "{message}");
        }
        other => panic!("expected a parse error, got {other:?}"),
    }
    let temp = TempKb::new();
    temp.write("grooves/binary.ron", "");
    fs::write(temp.root.join("grooves/binary.ron"), [0xff, 0xfe, 0x00]).unwrap();
    assert!(matches!(temp.error(), KnowledgeError::Io { .. }));

    // A directory that is not there.
    let nowhere = std::env::temp_dir().join("t2m-kb-does-not-exist");
    match Knowledge::from_dir(&nowhere).unwrap_err() {
        KnowledgeError::Io { path, message } => {
            assert!(path.ends_with("t2m-kb-does-not-exist"));
            assert!(!message.is_empty());
        }
        other => panic!("expected an I/O error, got {other:?}"),
    }
}

#[test]
fn t6_3_unexpected_files_are_refused_and_hidden_files_ignored() {
    let unexpected = |relative: &str| {
        let temp = TempKb::new();
        temp.write(relative, "[]");
        temp.error()
    };
    let refused = |path: &str| KnowledgeError::Unexpected { path: path.into() };
    assert_eq!(unexpected("notes.txt"), refused("notes.txt"));
    assert_eq!(unexpected("stylez/x.ron"), refused("stylez/x.ron"));
    assert_eq!(unexpected("styles/x.json"), refused("styles/x.json"));
    assert_eq!(unexpected("gallery/x.ron"), refused("gallery/x.ron"));
    assert_eq!(unexpected("styles/README"), refused("styles/README"));
    assert_eq!(
        refused("notes.txt").to_string(),
        "notes.txt is not a knowledge file this build understands"
    );

    let temp = TempKb::full();
    temp.write(".DS_Store", "junk")
        .write("styles/.DS_Store", "junk")
        .write(".git/config", "junk");
    assert_eq!(temp.load().unwrap(), Knowledge::load().unwrap());
    // An empty list and an empty kind directory are fine.
    temp.write("cycles/none.ron", "[]");
    fs::create_dir_all(temp.root.join("tunings-empty")).unwrap();
    assert_eq!(temp.load().unwrap().gallery().len(), 1);
}

#[test]
fn t6_3_invalid_entries_are_refused() {
    let invalid_id = |relative: &str, from: &str, to: &str| {
        let temp = TempKb::new();
        temp.replace(relative, from, to);
        match temp.error() {
            KnowledgeError::Invalid { id, reason } => {
                assert!(!reason.is_empty());
                id
            }
            other => panic!("expected an invalid entry for {to:?}, got {other:?}"),
        }
    };
    let ok = |relative: &str, from: &str, to: &str| {
        let temp = TempKb::new();
        temp.replace(relative, from, to);
        temp.load()
            .unwrap_or_else(|error| panic!("{to:?} should load: {error}"));
    };
    let tunings = "tunings/equal.ron";
    assert_eq!(
        invalid_id(tunings, "cents: [0, 100,", "cents: [5, 100,"),
        "12tet"
    );
    assert_eq!(invalid_id(tunings, "period: 1200", "period: 1100"), "12tet");
    assert_eq!(
        invalid_id(tunings, "[0, 100, 200,", "[0, 200, 200,"),
        "12tet"
    );
    let modes = "tonal/modes.ron";
    assert_eq!(
        invalid_id(modes, "steps: [0, 2, 4, 5, 7, 9, 11]", "steps: []"),
        "mode.major"
    );
    assert_eq!(
        invalid_id(modes, "aliases: [\"dorian\"]", "emphasis: [7]"),
        "mode.dorian"
    );

    let gm = "instruments/gm.ron";
    assert_eq!(
        invalid_id(gm, "gm_program: 33", "gm_program: 128"),
        "bass.electric"
    );
    ok(gm, "gm_program: 33", "gm_program: 127");
    assert_eq!(
        invalid_id(gm, "roles: [bass]", "roles: []"),
        "bass.electric"
    );
    assert_eq!(invalid_id(gm, "low: 28,", "low: 64,"), "bass.electric");
    ok(gm, "low: 28,", "low: 63,");
    assert_eq!(invalid_id(gm, "high: 108", "high: 128"), "keys.piano");
    ok(gm, "high: 108", "high: 127");
    assert_eq!(invalid_id(gm, "note: 36", "note: 128"), "kit.standard");
    ok(gm, "note: 36", "note: 127");
    assert_eq!(
        invalid_id(gm, "name: \"snare\"", "name: \"kick\""),
        "kit.standard"
    );
    // A kit's own range fields are not checked.
    ok(
        gm,
        "low: 0,\n        high: 127",
        "low: 90,\n        high: 10",
    );

    let prog = "progressions/pop.ron";
    assert_eq!(
        invalid_id(prog, "[\"I\", \"V\", \"vi\", \"IV\"]", "[]"),
        "prog.axis"
    );
    assert_eq!(invalid_id(prog, "\"vi\"", "\"viii\""), "prog.axis");
    assert_eq!(
        invalid_id(prog, "bars_per_chord: 1", "bars_per_chord: 0"),
        "prog.axis"
    );

    let pop = "styles/pop.ron";
    let tempo = "(min: 90, max: 130, default: 116)";
    for bad in [
        "(min: 29, max: 130, default: 116)",
        "(min: 90, max: 301, default: 116)",
        "(min: 90, max: 130, default: 89)",
        "(min: 90, max: 130, default: 131)",
        "(min: 131, max: 130, default: 131)",
    ] {
        assert_eq!(invalid_id(pop, tempo, bad), "pop.generic", "{bad}");
    }
    for good in [
        "(min: 30, max: 300, default: 30)",
        "(min: 30, max: 300, default: 300)",
        "(min: 116, max: 116, default: 116)",
    ] {
        ok(pop, tempo, good);
    }
    assert_eq!(
        invalid_id(pop, "meter: (4, 4)", "meter: (4, 5)"),
        "pop.generic"
    );
    assert_eq!(
        invalid_id(pop, "bars: 4, energy: 0.3", "bars: 0, energy: 0.3"),
        "pop.generic"
    );
    assert_eq!(
        invalid_id(pop, "energy: 0.3,", "energy: 1.1,"),
        "pop.generic"
    );
    assert_eq!(
        invalid_id(pop, "energy: 0.3,", "energy: -0.1,"),
        "pop.generic"
    );
    assert_eq!(
        invalid_id(pop, "density: 0.3,", "density: 1.5,"),
        "pop.generic"
    );
    ok(
        pop,
        "energy: 0.3, density: 0.3,",
        "energy: 0.0, density: 1.0,",
    );

    let temp = TempKb::new();
    temp.write(
        "styles/extra.ron",
        &EXTRA_STYLES.replace(
            "form: [(id: \"a\", kind: \"verse\", bars: 8, energy: 0.5, density: 0.5)]",
            "form: []",
        ),
    );
    assert!(matches!(temp.error(), KnowledgeError::Invalid { id, .. } if id == "rock.test"));
    let temp = TempKb::new();
    temp.write(
        "styles/extra.ron",
        &EXTRA_STYLES.replacen(
            "palette: [(role: drums, instrument: \"kit.standard\")]",
            "palette: []",
            1,
        ),
    );
    assert!(matches!(temp.error(), KnowledgeError::Invalid { id, .. } if id == "rock.test"));
    let temp = TempKb::new();
    temp.write(
        "cycles/talas.ron",
        &RUPAK.replace("grouping: [3, 2, 2]", "grouping: [3, 2]"),
    );
    assert!(matches!(temp.error(), KnowledgeError::Invalid { id, .. } if id == "tala.rupak"));
}

#[test]
fn t6_4_everything_the_gallery_uses_is_verified_and_sourced() {
    let kb = Knowledge::load().unwrap();
    assert_eq!(kb.verification_gaps(), Vec::<String>::new());

    let gaps_after = |relative: &str, from: &str, to: &str| {
        let temp = TempKb::new();
        temp.replace(relative, from, to);
        temp.load().unwrap().verification_gaps()
    };
    let unverified = "verified: false";
    assert_eq!(
        gaps_after("tunings/equal.ron", "verified: true", unverified),
        ["tuning 12tet: not verified"]
    );
    assert_eq!(
        gaps_after("tonal/modes.ron", "verified: true", unverified),
        ["system mode.major: not verified"]
    );
    assert_eq!(
        gaps_after("grooves/basic.ron", "verified: true", unverified),
        ["groove groove.straight_8: not verified"]
    );
    assert_eq!(
        gaps_after("instruments/gm.ron", "verified: true", unverified),
        ["instrument keys.piano: not verified"]
    );
    assert_eq!(
        gaps_after("progressions/pop.ron", "verified: true", unverified),
        ["progression prog.axis: not verified"]
    );
    assert_eq!(
        gaps_after("styles/pop.ron", "verified: true", unverified),
        ["style pop.generic: not verified"]
    );
    let temp = TempKb::new();
    let groove = r#"[(id: "groove.straight_8", name: "Straight", source: "  ", verified: true)]"#;
    temp.write("grooves/basic.ron", groove);
    assert_eq!(
        temp.load().unwrap().verification_gaps(),
        ["groove groove.straight_8: no source"]
    );

    // Entries the gallery does not reach are not held to the gate...
    let (temp, kb) = with_extras();
    assert_eq!(kb.verification_gaps(), Vec::<String>::new());
    // ...until a gallery brief uses them, directly or through a style, groove or system.
    temp.replace(
        "gallery/pop-starter.json",
        "\"style\": \"pop.generic\"",
        "\"style\": \"pop.generic\", \"fusion\": \"rock.test\"",
    );
    assert_eq!(
        temp.load().unwrap().verification_gaps(),
        ["style rock.test: not verified"]
    );
    temp.replace(
        "grooves/basic.ron",
        "aliases:",
        "cycles: [\"tala.rupak\"], aliases:",
    );
    assert_eq!(
        temp.load().unwrap().verification_gaps(),
        [
            "style rock.test: not verified",
            "cycle tala.rupak: not verified"
        ]
    );
    let temp = TempKb::new();
    temp.write("cycles/talas.ron", RUPAK).replace(
        "gallery/pop-starter.json",
        "{ \"kind\": \"meter\", \"numerator\": 4, \"denominator\": 4 }",
        "{ \"kind\": \"cycle\", \"id\": \"tala.rupak\" }",
    );
    assert_eq!(
        temp.load().unwrap().verification_gaps(),
        ["cycle tala.rupak: not verified"]
    );
    // A system reached only through a style, and a tuning only through that system.
    let temp = TempKb::new();
    temp.write(
        "tunings/other.ron",
        r#"[(id: "12b", cents: [0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100], period: 1200, source: "s", verified: false)]"#,
    )
    .replace("tonal/modes.ron", "default_tuning: \"12tet\",\n        aliases: [\"minor\"", "default_tuning: \"12b\",\n        aliases: [\"minor\"")
    .replace("styles/pop.ron", "system: \"mode.major\"", "system: \"mode.minor\"");
    assert_eq!(
        temp.load().unwrap().verification_gaps(),
        ["tuning 12b: not verified"]
    );
}

#[test]
fn t6_5_data_version_is_stable_and_tracks_every_byte() {
    let first = Knowledge::load().unwrap();
    let second = Knowledge::load().unwrap();
    assert_eq!(first.data_version(), second.data_version());
    assert_eq!(first.data_version().len(), 16);
    assert!(first.data_version().chars().all(|c| c.is_ascii_hexdigit()));

    let temp = TempKb::full();
    assert_eq!(temp.load().unwrap().data_version(), first.data_version());
    // One changed byte in one file.
    temp.replace("grooves/basic.ron", "Straight eighths", "Straight Eighths");
    let changed = temp.load().unwrap();
    assert_ne!(changed.data_version(), first.data_version());
    // A new file, even one that adds no entries.
    let temp = TempKb::full();
    temp.write("cycles/none.ron", "[]");
    assert_ne!(temp.load().unwrap().data_version(), first.data_version());
    // The same bytes under another file name.
    let temp = TempKb::full();
    let text = fs::read_to_string(temp.root.join("grooves/basic.ron")).unwrap();
    fs::remove_file(temp.root.join("grooves/basic.ron")).unwrap();
    temp.write("grooves/renamed.ron", &text);
    assert_ne!(temp.load().unwrap().data_version(), first.data_version());
}

#[test]
fn t6_6_hostile_search_text_is_harmless() {
    let (_temp, kb) = with_extras();
    let long = "pop".repeat(10_000);
    let cases = [
        long.as_str(),
        "🎹🎹🎹",
        "pop\0generic",
        "\0",
        "'; DROP TABLE styles; --",
        "مرحبا",
        "POP\n\n\n",
        "ｐｏｐ",
    ];
    for text in cases {
        for limit in [0, 1, 2, usize::MAX] {
            let found = kb.search_styles(text, limit);
            assert!(found.len() <= limit, "query {text:.20?}");
            assert!(found.len() <= 4);
        }
        for kind in [
            Kind::Style,
            Kind::Instrument,
            Kind::TonalSystem,
            Kind::Tuning,
            Kind::Cycle,
            Kind::Groove,
            Kind::Progression,
        ] {
            let _ = kb.nearest(kind, text);
        }
    }
    // Only the first 64 letters and digits are looked at.
    let padded = format!("{}{}", "pop.generic", "!".repeat(5_000));
    assert_eq!(ids(kb.search_styles(&padded, 1)), ["pop.generic"]);
    let sixty_four = "z".repeat(64);
    let temp = TempKb::new();
    temp.replace(
        "styles/pop.ron",
        "aliases: [",
        &format!("aliases: [\"{sixty_four}\", "),
    );
    let kb = temp.load().unwrap();
    assert_eq!(
        ids(kb.search_styles(&format!("{sixty_four}zzzz"), 1)),
        ["pop.generic"]
    );
    assert_eq!(ids(kb.search_styles("POP\n\n\n", 1)), ["pop.generic"]);
    assert_eq!(ids(kb.search_styles("pop\0generic", 1)), ["pop.generic"]);
}

#[test]
fn a_knowledge_base_needs_a_style_a_kit_and_a_pitched_instrument() {
    let invalid = |temp: &TempKb| match temp.error() {
        KnowledgeError::Invalid { id, .. } => id,
        other => panic!("expected an invalid knowledge base, got {other:?}"),
    };
    // No styles (the gallery goes too, since it needs one).
    let temp = TempKb::new();
    fs::remove_dir_all(temp.root.join("styles")).unwrap();
    fs::remove_dir_all(temp.root.join("gallery")).unwrap();
    assert_eq!(invalid(&temp), "styles");

    let kit_only = r#"[(id: "kit.standard", name: "Kit", gm_program: 0, low: 0, high: 127,
        polyphonic: true, roles: [drums], kit: [(name: "kick", note: 36)], source: "s", verified: false)]"#;
    let pitched_only = r#"[(id: "keys.piano", name: "Piano", gm_program: 0, low: 21, high: 108,
        polyphonic: true, roles: [lead], source: "s", verified: false)]"#;
    let minimal_style = r#"[(id: "only.style", name: "Only", tempo: (min: 60, max: 180, default: 120),
        meter: (4, 4), feel: "groove.straight_8", system: "mode.major", harmonic_language: functional,
        form: [(id: "a", kind: "verse", bars: 8, energy: 0.5, density: 0.5)],
        palette: [(role: lead, instrument: "INSTRUMENT")], source: "s", verified: false)]"#;
    for (instruments, palette) in [(kit_only, "kit.standard"), (pitched_only, "keys.piano")] {
        let temp = TempKb::new();
        fs::remove_dir_all(temp.root.join("gallery")).unwrap();
        temp.write("instruments/gm.ron", instruments).write(
            "styles/pop.ron",
            &minimal_style.replace("INSTRUMENT", palette),
        );
        assert_eq!(invalid(&temp), "instruments");
        // With the other kind of instrument added, the same data loads.
        let other = if palette == "kit.standard" {
            pitched_only
        } else {
            kit_only
        };
        temp.write("instruments/more.ron", other);
        let kb = temp.load().unwrap();
        // With no general-purpose style, the first style is the fallback.
        assert_eq!(kb.default_style().unwrap().id, "only.style");
    }
    assert_eq!(
        Knowledge::load().unwrap().default_style().unwrap().id,
        "pop.generic"
    );
    let (_temp, kb) = with_extras();
    assert_eq!(kb.default_style().unwrap().id, "pop.generic");
}
