mod common;

use std::sync::OnceLock;

use serde_json::{Value, json};
use t2m_core::brief::{Brief, Length, SectionHarmony};
use t2m_core::patch::{BriefPatch, ChangeSet, PatchError, PatchOp, apply, changed_inputs};
use t2m_core::{Knowledge, validate};

fn kb() -> &'static Knowledge {
    static KB: OnceLock<Knowledge> = OnceLock::new();
    KB.get_or_init(common::extended_kb)
}

fn starter() -> Brief {
    kb().gallery()[0].brief.clone()
}

fn replace(path: &str, value: Value) -> PatchOp {
    PatchOp::Replace {
        path: path.into(),
        value,
    }
}

fn add(path: &str, value: Value) -> PatchOp {
    PatchOp::Add {
        path: path.into(),
        value,
    }
}

fn remove(path: &str) -> PatchOp {
    PatchOp::Remove { path: path.into() }
}

fn adjust(path: &str, delta: f64) -> PatchOp {
    PatchOp::Adjust {
        path: path.into(),
        delta,
    }
}

fn patched(ops: Vec<PatchOp>) -> Brief {
    let brief = apply(&starter(), &BriefPatch(ops), kb()).unwrap();
    assert_eq!(
        validate(&brief, kb()),
        Ok(()),
        "a patched brief is always valid"
    );
    brief
}

#[track_caller]
fn refused(ops: Vec<PatchOp>) -> PatchError {
    let before = starter();
    let error = apply(&before, &BriefPatch(ops), kb()).unwrap_err();
    assert_eq!(before, starter(), "a refused patch leaves the brief alone");
    error
}

fn no_path(op: usize, path: &str) -> PatchError {
    PatchError::Path {
        op,
        path: path.into(),
    }
}

fn new_track(id: &str) -> Value {
    json!({ "id": id, "role": "pad", "instrument": "keys.piano", "busy": 0.3 })
}

#[test]
fn t8_1_each_operation() {
    let brief = patched(vec![replace("/time/tempo_bpm", json!(90))]);
    assert_eq!(brief.time.tempo_bpm, 90);
    let brief = patched(vec![replace("/identity/title", json!("New title"))]);
    assert_eq!(brief.identity.title, "New title");
    let brief = patched(vec![replace(
        "/form/2/harmony",
        json!({ "kind": "named", "id": "prog.axis" }),
    )]);
    assert_eq!(
        brief.form[2].harmony,
        SectionHarmony::Named {
            id: "prog.axis".into()
        }
    );

    // 0.9 + 0.2 clamps at 1.
    let brief = patched(vec![adjust("/form/2/energy", 0.2)]);
    assert_eq!(brief.form[2].energy, 1.0);
    let brief = patched(vec![adjust("/form/1/energy", 0.25)]);
    assert_eq!(brief.form[1].energy, 0.75);
    let brief = patched(vec![adjust("/form/1/energy", -2.0)]);
    assert_eq!(brief.form[1].energy, 0.0);
    // Whole-number fields stay whole.
    let brief = patched(vec![adjust("/time/tempo_bpm", 4.0)]);
    assert_eq!(brief.time.tempo_bpm, 120);
    let brief = patched(vec![adjust("/time/tempo_bpm", -6.4)]);
    assert_eq!(brief.time.tempo_bpm, 110);
    let brief = patched(vec![adjust("/time/tempo_bpm", 0.5)]);
    assert_eq!(brief.time.tempo_bpm, 117);
    let brief = patched(vec![adjust("/form/1/bars", 8.0)]);
    assert_eq!(brief.form[1].bars, 16);

    let brief = patched(vec![add("/tracks/-", new_track("pad"))]);
    assert_eq!(brief.tracks.len(), 5);
    assert_eq!(brief.tracks[4].id, "pad");
    let brief = patched(vec![add("/tracks/0", new_track("pad"))]);
    assert_eq!(
        (brief.tracks[0].id.as_str(), brief.tracks[1].id.as_str()),
        ("pad", "lead")
    );
    let brief = patched(vec![add("/tracks/4", new_track("pad"))]);
    assert_eq!(brief.tracks[4].id, "pad");
    let brief = patched(vec![add("/form/1/tracks/-", json!("chords"))]);
    assert_eq!(
        brief.form[1].tracks.len(),
        4,
        "a repeated track id is dropped by repair"
    );
    // Adding to an object sets the field, new or existing.
    let brief = patched(vec![add("/identity/fusion", json!("hiphop.lofi"))]);
    assert_eq!(brief.identity.fusion.as_deref(), Some("hiphop.lofi"));
    let brief = patched(vec![add("/time/tempo_bpm", json!(100))]);
    assert_eq!(brief.time.tempo_bpm, 100);
    let motif = json!({ "degrees": [0, 2, 4], "durations": [1, 1, 2] });
    let brief = patched(vec![add("/motif", motif)]);
    assert_eq!(brief.motif.unwrap().degrees, [0, 2, 4]);

    // Removing a track removes it from every section.
    assert!(
        starter()
            .form
            .iter()
            .any(|s| s.tracks.contains(&"lead".to_string()))
    );
    let brief = patched(vec![remove("/tracks/0")]);
    let ids: Vec<&str> = brief.tracks.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids, ["chords", "bass", "drums"]);
    assert!(
        brief
            .form
            .iter()
            .all(|s| !s.tracks.contains(&"lead".to_string()))
    );
    assert_eq!(brief.form[3].tracks, ["chords"]);
    let brief = patched(vec![remove("/tracks/3")]);
    assert_eq!(brief.tracks.len(), 3);
    let brief = patched(vec![
        add("/identity/fusion", json!("hiphop.lofi")),
        remove("/identity/fusion"),
    ]);
    assert_eq!(brief.identity.fusion, None);
    let brief = patched(vec![
        remove("/form/0"),
        replace("/time/length", json!({ "bars": 20 })),
    ]);
    assert_eq!(brief.form[0].id, "verse");
    assert_eq!(brief.time.length, Length::Bars(20));

    // Operations apply in order, each seeing the result of the one before.
    let brief = patched(vec![
        add("/tracks/-", new_track("pad")),
        replace("/tracks/4/busy", json!(0.9)),
        adjust("/tracks/4/busy", -0.5),
        add("/form/1/tracks/-", json!("pad")),
    ]);
    assert!((brief.tracks[4].busy - 0.4).abs() < 1e-12);
    assert_eq!(brief.form[1].tracks.last().unwrap(), "pad");
    // An empty patch changes nothing.
    assert_eq!(patched(vec![]), starter());
}

#[test]
fn t8_2_bad_edits_are_refused_and_change_nothing() {
    for path in [
        "/nope",
        "/time/nope",
        "/tracks/4",
        "/tracks/99",
        "/tracks/-",
        "/tracks/-1",
        "/tracks/01",
        "/tracks/+1",
        "/tracks/1.0",
        "/tracks/x",
        "/tracks/0/id/deeper",
        "",
        "time/tempo_bpm",
        "/",
        "/time/",
        "//time",
    ] {
        assert_eq!(
            refused(vec![replace(path, json!(1))]),
            no_path(0, path),
            "replace {path:?}"
        );
        assert_eq!(
            refused(vec![adjust(path, 1.0)]),
            no_path(0, path),
            "adjust {path:?}"
        );
    }
    for path in [
        "/nope",
        "/tracks/4",
        "/tracks/-",
        "/tracks/01",
        "/tracks/0/id/x",
        "",
        "tracks/0",
    ] {
        assert_eq!(
            refused(vec![remove(path)]),
            no_path(0, path),
            "remove {path:?}"
        );
    }
    for path in [
        "/nope/x",
        "/tracks/5",
        "/tracks/99",
        "/tracks/x",
        "/tracks/0/id/x",
        "",
        "tracks/-",
        "/tracks/-/x",
    ] {
        assert_eq!(
            refused(vec![add(path, new_track("pad"))]),
            no_path(0, path),
            "add {path:?}"
        );
    }
    // An optional field that is already empty can be removed again: nothing changes.
    assert_eq!(patched(vec![remove("/motif")]), starter());
    assert_eq!(
        refused(vec![replace("/x", json!(1))]).to_string(),
        "edit 0: nothing at \"/x\""
    );

    // Wrong type for the field: the edited brief no longer reads as a brief.
    for (path, value) in [
        ("/time/tempo_bpm", json!("fast")),
        ("/time/tempo_bpm", json!(-5)),
        ("/time/tempo_bpm", json!(90.5)),
        ("/tracks", json!("none")),
        ("/tracks/0", json!(7)),
        ("/tracks/0/role", json!("kazoo")),
        ("/form/0/harmony", json!({ "kind": "vibes" })),
        ("/tonality", json!(null)),
    ] {
        let error = refused(vec![replace(path, value)]);
        assert!(
            matches!(error, PatchError::Shape { .. }),
            "{path}: {error:?}"
        );
        assert!(
            error
                .to_string()
                .starts_with("the edited brief is not readable: ")
        );
    }
    assert!(matches!(
        refused(vec![remove("/time")]),
        PatchError::Shape { .. }
    ));
    assert!(matches!(
        refused(vec![remove("/tracks/0/id")]),
        PatchError::Shape { .. }
    ));
    assert!(matches!(
        refused(vec![add("/tracks/-", json!("pad"))]),
        PatchError::Shape { .. }
    ));
    // An adjustment that leaves a whole-number field negative cannot be read back.
    assert!(matches!(
        refused(vec![adjust("/time/tempo_bpm", -500.0)]),
        PatchError::Shape { .. }
    ));

    let value = |path: &str, reason: &'static str| PatchError::Value {
        op: 0,
        path: path.into(),
        reason,
    };
    for path in [
        "/identity/style",
        "/tracks",
        "/tracks/0",
        "/identity/fusion",
        "/form/0/tracks/0",
    ] {
        assert_eq!(
            refused(vec![adjust(path, 1.0)]),
            value(path, "only a number can be adjusted"),
            "{path}"
        );
    }
    for delta in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            refused(vec![adjust("/form/0/energy", delta)]),
            value("/form/0/energy", "the amount is not a finite number")
        );
    }
    assert_eq!(
        refused(vec![
            adjust("/form/0/energy", f64::MAX),
            adjust("/form/0/energy", f64::MAX)
        ]),
        PatchError::Value {
            op: 1,
            path: "/form/0/energy".into(),
            reason: "the result is too large"
        }
    );
    assert_eq!(
        refused(vec![adjust("/time/tempo_bpm", 1e300)]),
        value("/time/tempo_bpm", "the result is too large")
    );
    assert_eq!(
        refused(vec![adjust("/time/tempo_bpm", -1e300)]),
        value("/time/tempo_bpm", "the result is too large")
    );
    assert_eq!(
        value("/a", "only a number can be adjusted").to_string(),
        "edit 0 at \"/a\": only a number can be adjusted"
    );
}

#[test]
fn t8_3_a_failing_edit_cancels_the_whole_patch() {
    let error = refused(vec![
        replace("/time/tempo_bpm", json!(90)),
        replace("/nope", json!(1)),
        replace("/identity/title", json!("never")),
    ]);
    assert_eq!(error, no_path(1, "/nope"));
    let error = refused(vec![
        remove("/tracks/0"),
        adjust("/form/0/energy", 0.1),
        adjust("/identity/style", 1.0),
    ]);
    assert_eq!(
        error,
        PatchError::Value {
            op: 2,
            path: "/identity/style".into(),
            reason: "only a number can be adjusted"
        }
    );
    // An edit that only makes sense after an earlier one fails when that one is absent.
    assert_eq!(
        refused(vec![replace("/tracks/4/busy", json!(0.9))]),
        no_path(0, "/tracks/4/busy")
    );
}

#[test]
fn t8_4_the_result_is_repaired() {
    let brief = patched(vec![replace("/time/tempo_bpm", json!(999))]);
    assert_eq!(brief.time.tempo_bpm, 300);
    let brief = patched(vec![replace("/identity/style", json!("lofi hiphop"))]);
    assert_eq!(brief.identity.style, "hiphop.lofi");
    let brief = patched(vec![replace("/tracks/0/instrument", json!("grand piano"))]);
    assert_eq!(brief.tracks[0].instrument, "keys.piano");
    let brief = patched(vec![replace("/tracks", json!([]))]);
    assert_eq!(
        brief.tracks.len(),
        4,
        "an emptied track list is refilled from the style"
    );
    let brief = patched(vec![adjust("/tracks/0/busy", 5.0)]);
    assert_eq!(brief.tracks[0].busy, 1.0);
    let brief = patched(vec![add("/tracks/-", new_track("lead"))]);
    assert_eq!(brief.tracks[4].id, "lead_2");
}

fn change(edit: impl FnOnce(&mut Brief)) -> ChangeSet {
    let old = starter();
    let mut new = old.clone();
    edit(&mut new);
    changed_inputs(&old, &new)
}

fn set(ids: &[&str]) -> std::collections::BTreeSet<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

#[test]
fn t8_5_change_sets_say_what_an_edit_touches() {
    let nothing = change(|_| {});
    assert_eq!(nothing, ChangeSet::default());
    assert!(nothing.is_empty());
    assert!(!nothing.affects("lead", "verse"));

    let global: [fn(&mut Brief); 9] = [
        |b| b.time.tempo_bpm = 100,
        |b| b.time.feel = "groove.lofi_swing".into(),
        |b| b.time.length = Length::Bars(48),
        |b| b.tonality.system = "mode.minor".into(),
        |b| b.identity.style = "hiphop.lofi".into(),
        |b| b.identity.fusion = Some("hiphop.lofi".into()),
        |b| {
            b.motif = Some(t2m_core::brief::Motif {
                degrees: vec![0],
                durations: vec![1.0],
                development: String::new(),
            })
        },
        |b| b.form.swap(1, 2),
        |b| {
            b.form.pop();
        },
    ];
    for (index, edit) in global.into_iter().enumerate() {
        let changed = change(edit);
        assert!(changed.global, "edit {index}");
        assert!(!changed.is_empty());
        assert!(changed.affects("lead", "verse"), "edit {index}");
    }
    // Only the tempo changed: nothing is singled out.
    let tempo = change(|b| b.time.tempo_bpm = 100);
    assert!(tempo.tracks.is_empty() && tempo.sections.is_empty());

    // One track: that track, in every section.
    let busy = change(|b| b.tracks[2].busy = 0.9);
    assert_eq!(
        busy,
        ChangeSet {
            global: false,
            tracks: set(&["bass"]),
            sections: set(&[])
        }
    );
    assert!(!busy.is_empty());
    assert!(busy.affects("bass", "intro") && busy.affects("bass", "chorus"));
    assert!(!busy.affects("lead", "chorus"));
    let instrument = change(|b| b.tracks[0].instrument = "keys.rhodes".into());
    assert_eq!(instrument.tracks, set(&["lead"]));
    let added = change(|b| {
        let mut pad = b.tracks[1].clone();
        pad.id = "pad".into();
        b.tracks.push(pad);
    });
    assert_eq!(
        added,
        ChangeSet {
            global: false,
            tracks: set(&["pad"]),
            sections: set(&[])
        }
    );
    let removed = change(|b| {
        b.tracks.remove(1);
    });
    assert_eq!(removed.tracks, set(&["chords"]));
    assert!(!removed.global);
    let renamed = change(|b| b.tracks[0].id = "melody".into());
    assert_eq!(renamed.tracks, set(&["lead", "melody"]));
    // Reordering tracks is not a musical change.
    assert!(change(|b| b.tracks.swap(0, 1)).is_empty());

    // One section: every track, in that section only.
    let energy = change(|b| b.form[1].energy = 0.9);
    assert_eq!(
        energy,
        ChangeSet {
            global: false,
            tracks: set(&[]),
            sections: set(&["verse"])
        }
    );
    assert!(!energy.is_empty());
    assert!(energy.affects("lead", "verse") && energy.affects("drums", "verse"));
    assert!(!energy.affects("lead", "chorus"));
    let harmony = change(|b| b.form[2].harmony = SectionHarmony::None);
    assert_eq!(harmony.sections, set(&["chorus"]));
    let lineup = change(|b| {
        b.form[0].tracks.pop();
    });
    assert_eq!(lineup.sections, set(&["intro"]));
    // A section's length moves everything after it, so it is global as well.
    let bars = change(|b| b.form[1].bars = 16);
    assert!(bars.global);
    assert_eq!(bars.sections, set(&["verse"]));
    let renamed = change(|b| b.form[3].id = "ending".into());
    assert!(renamed.global);
    assert_eq!(renamed.sections, set(&["ending", "outro"]));

    // Words about the piece are not music.
    let words = change(|b| {
        b.identity.title = "Another".into();
        b.identity.summary = "different words".into();
        b.assumptions.clear();
        b.identity.references.clear();
    });
    assert!(words.is_empty());

    let both = change(|b| {
        b.tracks[2].busy = 0.9;
        b.form[1].energy = 0.9;
    });
    assert!(both.affects("bass", "intro") && both.affects("lead", "verse"));
    assert!(!both.affects("lead", "intro"));
}

#[test]
fn t8_6_pointer_escapes_and_serialized_patches() {
    // "~1" is "/" and "~0" is "~" inside a token; neither appears in a brief's
    // own field names, so such paths simply find nothing.
    assert_eq!(
        refused(vec![replace("/time~1tempo_bpm", json!(1))]),
        no_path(0, "/time~1tempo_bpm")
    );
    assert_eq!(
        refused(vec![replace("/a~0b", json!(1))]),
        no_path(0, "/a~0b")
    );
    // "~01" is the two characters "~1", not a slash.
    assert_eq!(refused(vec![remove("/~01")]), no_path(0, "/~01"));

    let patch = BriefPatch(vec![
        replace("/time/tempo_bpm", json!(90)),
        add("/tracks/-", new_track("pad")),
        remove("/tracks/0"),
        adjust("/form/1/energy", 0.2),
    ]);
    let text = serde_json::to_string(&patch).unwrap();
    assert_eq!(
        text,
        concat!(
            r#"[{"op":"replace","path":"/time/tempo_bpm","value":90},"#,
            r#"{"op":"add","path":"/tracks/-","value":{"busy":0.3,"id":"pad","instrument":"keys.piano","role":"pad"}},"#,
            r#"{"op":"remove","path":"/tracks/0"},"#,
            r#"{"op":"adjust","path":"/form/1/energy","delta":0.2}]"#
        )
    );
    assert_eq!(serde_json::from_str::<BriefPatch>(&text).unwrap(), patch);
    let applied = apply(&starter(), &patch, kb()).unwrap();
    assert_eq!(applied.time.tempo_bpm, 90);
    assert_eq!(applied.tracks.len(), 4);
    assert!((applied.form[1].energy - 0.7).abs() < 1e-12);
    assert!(serde_json::from_str::<BriefPatch>(r#"[{"op":"explode","path":"/x"}]"#).is_err());
}

#[test]
fn t8_7_adding_to_a_field_the_brief_does_not_have_is_refused() {
    for path in [
        "/time/tempo",
        "/tracks/0/volume",
        "/form/0/enrgy",
        "/nope",
        "/identity/title2",
    ] {
        assert_eq!(
            refused(vec![add(path, json!(1))]),
            no_path(0, path),
            "{path}"
        );
    }
    // Empty optional fields exist as null, so they can still be filled in.
    let brief = patched(vec![add("/identity/fusion", json!("hiphop.lofi"))]);
    assert_eq!(brief.identity.fusion.as_deref(), Some("hiphop.lofi"));
    let motif = json!({ "degrees": [0, 2], "durations": [1, 1] });
    assert!(patched(vec![add("/motif", motif)]).motif.is_some());
}
