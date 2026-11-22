use super::*;

fn open(id: &str, seed: u64) -> (Session, View) {
    let mut session = Session::new().unwrap();
    let view = session.open_example(id, seed).unwrap();
    (session, view)
}

fn track<'a>(view: &'a View, id: &str) -> &'a crate::TrackView {
    view.tracks.iter().find(|t| t.id == id).unwrap()
}

fn in_section(t: &crate::TrackView, view: &View, section: &str) -> Vec<(i64, i64, f64)> {
    let s = view.sections.iter().find(|s| s.id == section).unwrap();
    t.notes
        .iter()
        .copied()
        .filter(|n| n.0 >= s.start && n.0 < s.end)
        .collect()
}

#[test]
fn t27_1_the_gallery_lists_every_example_with_its_title_and_style() {
    let session = Session::new().unwrap();
    let items = session.gallery();
    assert_eq!(items.len(), 36);
    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
    assert!(
        items
            .iter()
            .all(|i| !i.title.is_empty() && !i.style.is_empty())
    );
    assert!(ids.contains(&"jazz-swing-1") && ids.contains(&"pop-generic-1"));
}

#[test]
fn t27_2_opening_an_example_composes_it_and_nothing_works_before_that() {
    let mut empty = Session::new().unwrap();
    assert_eq!(empty.view(), Err(SessionError::NothingOpen));
    assert_eq!(empty.reroll("lead", None), Err(SessionError::NothingOpen));
    assert_eq!(empty.reseed(3), Err(SessionError::NothingOpen));
    assert_eq!(empty.lock("lead", true), Err(SessionError::NothingOpen));
    assert_eq!(empty.undo(), Err(SessionError::NothingOpen));
    assert_eq!(empty.redo(), Err(SessionError::NothingOpen));
    assert_eq!(
        empty.export(TuningProfile::Mpe, &[]).err(),
        Some(SessionError::NothingOpen)
    );
    assert_eq!(
        empty.open_example("nope", 1),
        Err(SessionError::NoSuchExample("nope".into()))
    );
    let (session, view) = open("jazz-swing-1", 4);
    assert_eq!(session.view().unwrap(), view);
    assert_eq!((view.seed, view.gallery_id.as_str()), (4, "jazz-swing-1"));
    assert!(view.tempo_bpm > 0 && view.end > 0 && !view.sections.is_empty());
    assert!(view.tracks.len() >= 3 && view.tracks.iter().all(|t| !t.notes.is_empty()));
    assert!(!view.can_undo && !view.can_redo && !view.title.is_empty());
    // Same example, same seed: the same view; another seed differs.
    let (_, again) = open("jazz-swing-1", 4);
    assert_eq!(again, view);
    let (_, other) = open("jazz-swing-1", 5);
    assert_ne!(other.tracks, view.tracks);
}

#[test]
fn t27_3_a_reroll_changes_only_that_part_in_that_section_and_undo_redo_restore_it() {
    let (mut session, first) = open("pop-generic-1", 1);
    let bass = track(&first, "bass").id.clone();
    // A section where the bass actually plays (an intro may leave it out).
    let section = first
        .sections
        .iter()
        .find(|s| !in_section(track(&first, &bass), &first, &s.id).is_empty())
        .unwrap()
        .id
        .clone();
    let after = session.reroll(&bass, Some(&section)).unwrap();
    assert!(after.can_undo && !after.can_redo);
    assert_ne!(
        in_section(track(&after, &bass), &after, &section),
        in_section(track(&first, &bass), &first, &section)
    );
    for t in &first.tracks {
        if t.id != bass {
            assert_eq!(
                track(&after, &t.id).notes,
                t.notes,
                "{} should not move",
                t.id
            );
        }
    }
    for s in first.sections.iter().filter(|s| s.id != section) {
        assert_eq!(
            in_section(track(&after, &bass), &after, &s.id),
            in_section(track(&first, &bass), &first, &s.id),
            "section {} should not move",
            s.id
        );
    }
    let undone = session.undo().unwrap();
    assert_eq!(
        (undone.tracks.clone(), undone.can_redo, undone.can_undo),
        (first.tracks.clone(), true, false)
    );
    let redone = session.redo().unwrap();
    assert_eq!(redone.tracks, after.tracks);
    assert!(!redone.can_redo && redone.can_undo);
    // A new change after an undo drops the redo branch.
    session.undo().unwrap();
    session.reroll(&bass, None).unwrap();
    assert!(!session.view().unwrap().can_redo);
}

#[test]
fn t27_4_a_locked_part_never_changes_until_it_is_unlocked() {
    let (mut session, first) = open("pop-generic-1", 1);
    let bass = "bass";
    let locked = session.lock(bass, true).unwrap();
    assert!(track(&locked, bass).locked);
    assert_eq!(
        session.reroll(bass, None),
        Err(SessionError::Locked(bass.into()))
    );
    let reseeded = session.reseed(77).unwrap();
    assert_eq!(track(&reseeded, bass).notes, track(&first, bass).notes);
    let moved = reseeded
        .tracks
        .iter()
        .filter(|t| t.id != bass)
        .any(|t| t.notes != track(&first, &t.id).notes);
    assert!(moved, "unlocked parts should change with a new seed");
    // Locking is undoable like any other change.
    let back = session.undo().unwrap();
    assert_eq!(back.seed, 1);
    session.undo().unwrap();
    let free = session.view().unwrap();
    assert!(!track(&free, bass).locked);
    session.lock(bass, true).unwrap();
    session.lock(bass, false).unwrap();
    assert!(session.reroll(bass, None).is_ok());
}

#[test]
fn t27_5_export_writes_what_is_on_screen_for_the_whole_piece_or_chosen_parts() {
    let (session, view) = open("jazz-swing-1", 2);
    let bundle = session.export(TuningProfile::Approx12, &[]).unwrap();
    assert_eq!(bundle.files[0].0, "jazz-swing-1.mid");
    let read = t2m_midi::read_notes(&bundle.files[0].1).unwrap();
    assert_eq!(read.len(), view.tracks.len() + 1);
    for (shown, written) in view.tracks.iter().zip(&read[1..]) {
        assert_eq!(written.notes.len(), shown.notes.len(), "{}", shown.id);
    }
    let one = session
        .export(TuningProfile::Mpe, &["bass".to_string()])
        .unwrap();
    assert_eq!(t2m_midi::read_notes(&one.files[0].1).unwrap().len(), 2);
    assert!(matches!(
        session.export(TuningProfile::Mpe, &["nope".to_string()]),
        Err(SessionError::Export(m)) if m.contains("no such track")
    ));
}

#[test]
fn t27_6_bad_input_is_a_typed_error_and_leaves_the_session_as_it_was() {
    let (mut session, view) = open("pop-generic-1", 1);
    assert!(matches!(
        session.open_json("not json", 1),
        Err(SessionError::BadBrief(_))
    ));
    assert!(matches!(
        session.open_json("", 1),
        Err(SessionError::BadBrief(_))
    ));
    assert!(matches!(
        session.open_json("[1,2]", 1),
        Err(SessionError::BadBrief(_))
    ));
    assert_eq!(session.view().unwrap(), view);
    assert_eq!(
        session.reroll("nope", None),
        Err(SessionError::NoSuchTrack("nope".into()))
    );
    assert_eq!(
        session.reroll("bass", Some("nope")),
        Err(SessionError::NoSuchSection("nope".into()))
    );
    assert_eq!(
        session.lock("nope", true),
        Err(SessionError::NoSuchTrack("nope".into()))
    );
    assert_eq!(session.view().unwrap(), view);
    // Undo and redo with nothing to do are harmless.
    assert_eq!(session.undo().unwrap(), view);
    assert_eq!(session.redo().unwrap(), view);
    // A brief with a misspelt style is repaired and opens.
    let text =
        include_str!("../../../suite/briefs/pop-generic-1.json").replace("pop.generic", "popp");
    let fixed = session.open_json(&text, 3).unwrap();
    assert_eq!(
        (fixed.gallery_id.as_str(), fixed.style.as_str()),
        ("custom", "pop.generic")
    );
    // History is capped: after 150 changes only 100 steps can be undone.
    for _ in 0..150 {
        session.reseed(9).unwrap();
        session.reroll("bass", None).unwrap();
    }
    let mut steps = 0;
    while session.view().unwrap().can_undo {
        session.undo().unwrap();
        steps += 1;
    }
    assert_eq!(steps, 100);
    for message in [
        SessionError::NothingOpen.to_string(),
        SessionError::Locked("bass".into()).to_string(),
    ] {
        assert!(!message.is_empty());
    }
    assert_eq!(
        SessionError::Locked("bass".into()).to_string(),
        "bass is locked; unlock it first"
    );
}

#[test]
fn t27_7_the_view_serialises_with_the_field_names_the_window_reads() {
    let (_, view) = open("pop-generic-1", 1);
    let json = serde_json::to_value(&view).unwrap();
    let keys: Vec<&str> = json
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    for want in [
        "gallery_id",
        "title",
        "style",
        "tempo_bpm",
        "seed",
        "ticks_per_quarter",
        "end",
        "sections",
        "tracks",
        "failed_gates",
        "can_undo",
        "can_redo",
    ] {
        assert!(keys.contains(&want), "missing {want}");
    }
    let t = &json["tracks"][0];
    assert!(t["id"].is_string() && t["role"].is_string() && t["locked"].is_boolean());
    assert_eq!(t["notes"][0].as_array().unwrap().len(), 3);
    assert!(json["sections"][0]["start"].is_i64());
    // Pitched notes are MIDI numbers (with cents as a fraction); the kit's are its key numbers.
    let kit = view.tracks.iter().find(|t| t.role == "drums").unwrap();
    assert!(kit.notes.iter().all(|n| n.2 >= 20.0 && n.2 <= 100.0));
    let lead = view.tracks.iter().find(|t| t.role == "lead").unwrap();
    assert!(lead.notes.iter().all(|n| n.2 > 20.0 && n.2 < 110.0));
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(24))]
    #[test]
    fn t27_8_undoing_every_change_returns_to_the_opened_piece(
        pick in 0usize..36,
        seed in 0u64..1000,
        ops in proptest::collection::vec((0u8..4, 0usize..6, 0u64..1000), 0..8),
    ) {
        let mut session = Session::new().unwrap();
        let id = session.gallery()[pick].id.clone();
        let opened = session.open_example(&id, seed).unwrap();
        for (kind, which, n) in ops {
            let tracks: Vec<String> = session.view().unwrap().tracks.iter().map(|t| t.id.clone()).collect();
            let name = tracks[which % tracks.len()].clone();
            let _ = match kind {
                0 => session.reroll(&name, None),
                1 => session.reseed(n),
                2 => session.lock(&name, n % 2 == 0),
                _ => session.undo().and_then(|_| session.redo()),
            };
        }
        while session.view().unwrap().can_undo {
            session.undo().unwrap();
        }
        let back = session.view().unwrap();
        proptest::prop_assert_eq!(back.tracks, opened.tracks);
        proptest::prop_assert_eq!(back.seed, opened.seed);
    }
}

#[test]
fn t27_9_after_unlocking_a_part_the_screen_matches_what_later_changes_keep() {
    let (mut session, first) = open("pop-generic-1", 1);
    session.lock("bass", true).unwrap();
    session.reseed(777).unwrap();
    let unlocked = session.lock("bass", false).unwrap();
    // The part now shows what the piece composes with the new seed.
    let (_, fresh) = open("pop-generic-1", 777);
    assert_eq!(track(&unlocked, "bass").notes, track(&fresh, "bass").notes);
    assert_ne!(track(&unlocked, "bass").notes, track(&first, "bass").notes);
    // A reroll of another part leaves it, and every other part, as shown.
    let other = unlocked
        .tracks
        .iter()
        .find(|t| t.id != "bass" && t.id != "drums")
        .unwrap()
        .id
        .clone();
    let after = session.reroll(&other, None).unwrap();
    for t in &unlocked.tracks {
        if t.id != other {
            assert_eq!(track(&after, &t.id).notes, t.notes, "{} moved", t.id);
        }
    }
    // Another locked part stays locked through an unlock.
    session.lock("drums", true).unwrap();
    let drums = track(&session.view().unwrap(), "drums").notes.clone();
    session.lock("bass", true).unwrap();
    session.reseed(5).unwrap();
    let view = session.lock("bass", false).unwrap();
    assert_eq!(track(&view, "drums").notes, drums);
    assert!(track(&view, "drums").locked && !track(&view, "bass").locked);
}

#[test]
fn t27_10_locking_what_is_already_locked_or_unlocking_what_is_not_changes_nothing() {
    let (mut session, view) = open("pop-generic-1", 1);
    assert_eq!(session.lock("bass", false).unwrap(), view);
    assert!(!session.view().unwrap().can_undo);
    session.lock("bass", true).unwrap();
    let once = session.view().unwrap();
    assert_eq!(session.lock("bass", true).unwrap(), once);
    // One undo undoes the one lock.
    let back = session.undo().unwrap();
    assert!(!track(&back, "bass").locked && !back.can_undo);
}

#[test]
fn t31_1_the_piece_to_play_matches_what_is_on_screen() {
    let mut session = Session::new().unwrap();
    assert_eq!(session.piece().unwrap_err(), SessionError::NothingOpen);
    let view = session.open_example("pop-generic-1", 1).unwrap();
    let count = |v: &View| v.tracks.iter().map(|t| t.notes.len()).sum::<usize>();
    let piece = session.piece().unwrap();
    assert_eq!(piece.notes.len(), count(&view));
    assert_eq!(piece.tracks, view.tracks.len());
    assert_eq!(piece.end, view.end);
    assert_eq!(piece.tempo_bpm, view.tempo_bpm);
}

#[test]
fn t31_2_the_piece_follows_rerolls_locks_and_undo() {
    let mut session = Session::new().unwrap();
    let first = session.open_example("pop-generic-1", 1).unwrap();
    let before = session.piece().unwrap();
    let lead = first
        .tracks
        .iter()
        .find(|t| t.id != "drums")
        .unwrap()
        .id
        .clone();
    session.reroll(&lead, None).unwrap();
    let after = session.piece().unwrap();
    assert_ne!(before, after, "a reroll must change what plays");
    session.undo().unwrap();
    assert_eq!(session.piece().unwrap(), before);
    session.open_example("pop-generic-1", 2).unwrap();
    assert_ne!(session.piece().unwrap(), before);
}

#[test]
fn t33_1_a_saved_project_opens_to_the_same_piece_and_carries_on_the_same() {
    let (mut a, _) = open("pop-generic-1", 1);
    let first = a.view().unwrap().tracks[1].id.clone();
    let second = a.view().unwrap().tracks[2].id.clone();
    a.reroll(&first, None).unwrap();
    a.lock(&second, true).unwrap();
    a.reseed(9).unwrap();
    let shown = a.view().unwrap();
    let file = a.save().unwrap();
    let mut b = Session::new().unwrap();
    let opened = b.open_project(&file).unwrap();
    assert_eq!(opened.tracks, shown.tracks);
    assert_eq!(
        (opened.seed, opened.end, &opened.title),
        (shown.seed, shown.end, &shown.title)
    );
    assert!(!opened.can_undo && !opened.can_redo);
    // The same next step gives the same result in both.
    assert_eq!(
        b.reroll(&first, None).unwrap().tracks,
        a.reroll(&first, None).unwrap().tracks
    );
    assert_eq!(b.reseed(77).unwrap().tracks, a.reseed(77).unwrap().tracks);
    assert!(track(&b.view().unwrap(), &second).locked);
    // Everything downstream agrees too: the piece to play, export, and undo.
    let (mut c, _) = open("pop-generic-1", 1);
    c.reroll(&first, None).unwrap();
    c.lock(&second, true).unwrap();
    c.reseed(9).unwrap();
    let mut d = Session::new().unwrap();
    d.open_project(&c.save().unwrap()).unwrap();
    assert_eq!(d.piece().unwrap(), c.piece().unwrap());
    let all = t2m_midi::TuningProfile::Mpe;
    assert_eq!(d.export(all, &[]).unwrap(), c.export(all, &[]).unwrap());
    assert!(!d.undo().unwrap().can_redo);
}

#[test]
fn t33_2_a_bad_project_is_refused_and_leaves_the_open_piece_alone() {
    let (mut s, view) = open("pop-generic-1", 1);
    let good = s.save().unwrap();
    let wrong_format = good.replacen("\"format\":1", "\"format\":99", 1);
    let ghost_lock = good.replacen("\"locked\":[]", "\"locked\":[\"nobody\"]", 1);
    let ghost_reroll = good.replacen("\"rerolls\":[]", "\"rerolls\":[[\"nobody\",\"x\",1]]", 1);
    let truncated = good[..good.len() / 2].to_string();
    let long = " ".repeat(33 * 1024 * 1024);
    for bad in [
        "",
        " ",
        "{}",
        "[]",
        "null",
        "not json",
        "\u{0}",
        "{\"format\":1}",
        &wrong_format,
        &ghost_lock,
        &ghost_reroll,
        &truncated,
        &long,
    ] {
        assert!(
            matches!(s.open_project(bad), Err(SessionError::BadProject(_))),
            "{:?}",
            &bad[..bad.len().min(40)]
        );
        assert_eq!(
            s.view().unwrap(),
            view,
            "a refused file changed the open piece"
        );
    }
    assert_ne!(good, wrong_format);
    assert_ne!(good, ghost_lock);
    assert_ne!(good, ghost_reroll);
}

#[test]
fn t33_3_saving_with_nothing_open_is_an_error_and_a_broken_score_is_refused() {
    let s = Session::new().unwrap();
    assert_eq!(s.save().unwrap_err(), SessionError::NothingOpen);
    let (mut s, _) = open("pop-generic-1", 1);
    let locked = s.view().unwrap().tracks[1].id.clone();
    s.lock(&locked, true).unwrap();
    let view = s.view().unwrap();
    let good = s.save().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&good).unwrap();
    for tr in value["score"]["tracks"].as_array_mut().unwrap() {
        if tr["id"] == locked.as_str() {
            tr["notes"][0]["dur"] = serde_json::json!(-5);
        }
    }
    let broken = value.to_string();
    assert!(matches!(
        s.open_project(&broken),
        Err(SessionError::BadProject(_))
    ));
    assert_eq!(s.view().unwrap(), view);
}

#[test]
fn t33_4_a_crafted_project_cannot_make_an_inconsistent_piece_or_a_path_out_of_the_folder() {
    let (mut s, _) = open("pop-generic-1", 1);
    let second = s.view().unwrap().tracks[2].id.clone();
    s.lock(&second, true).unwrap();
    let good = s.save().unwrap();
    let edit = |f: &dyn Fn(&mut serde_json::Value)| {
        let mut v: serde_json::Value = serde_json::from_str(&good).unwrap();
        f(&mut v);
        v.to_string()
    };
    // Names are cleaned: nothing that could leave a folder or hide a file survives.
    for (given, expected) in [
        ("../../escape", "escape"),
        (".hidden", "hidden"),
        ("a/b\\c", "abc"),
        ("", "custom"),
        ("\u{202e}", "custom"),
    ] {
        let text = edit(&|v| v["gallery_id"] = serde_json::json!(given));
        let mut t = Session::new().unwrap();
        assert_eq!(
            t.open_project(&text).unwrap().gallery_id,
            expected,
            "{given:?}"
        );
        let exported = t.export(t2m_midi::TuningProfile::Mpe, &[]).unwrap();
        assert!(
            exported
                .files
                .iter()
                .all(|(name, _)| !name.contains(['/', '\\']) && !name.starts_with('.'))
        );
    }
    // The piece comes from the brief and seed, so a lying score cannot change it.
    let (u, _) = open("pop-generic-1", 1);
    let unlocked = u.save().unwrap();
    let swapped = {
        let mut v: serde_json::Value = serde_json::from_str(&unlocked).unwrap();
        v["score"]["tracks"] = serde_json::json!([]);
        v["score"]["meta"]["tempo_bpm"] = serde_json::json!(0);
        v["score"]["sections"] = serde_json::json!([]);
        v.to_string()
    };
    let mut t = Session::new().unwrap();
    let view = t.open_project(&swapped).unwrap();
    let (_, plain) = open("pop-generic-1", 1);
    assert_eq!(view.tempo_bpm, plain.tempo_bpm);
    assert_eq!(view.sections.len(), plain.sections.len());
    // Locked notes that are out of range, a missing locked part, and silly counts are refused.
    let far = edit(&|v| {
        for tr in v["score"]["tracks"].as_array_mut().unwrap() {
            if tr["id"] == second.as_str() {
                tr["notes"][0]["pitch"]["cents"] = serde_json::json!(i32::MAX);
            }
        }
    });
    let hole = edit(&|v| v["score"]["tracks"] = serde_json::json!([]));
    let section = s.view().unwrap().sections[0].id.clone();
    let many =
        edit(&|v| v["rerolls"] = serde_json::json!([[second.as_str(), section, 4_000_000_000u64]]));
    let fine = edit(&|v| v["rerolls"] = serde_json::json!([[second.as_str(), section, 10_000u64]]));
    Session::new().unwrap().open_project(&fine).unwrap();
    let on_locked = |f: &dyn Fn(&mut serde_json::Value)| {
        edit(&|v| {
            for tr in v["score"]["tracks"].as_array_mut().unwrap() {
                if tr["id"] == second.as_str() {
                    f(&mut tr["notes"][0]);
                }
            }
        })
    };
    let early = on_locked(&|n| n["start"] = serde_json::json!(-5));
    let negative = on_locked(&|n| n["dur"] = serde_json::json!(-5));
    let bent = on_locked(&|n| n["bend"] = serde_json::json!([[0, 2_000_000_000i64]]));
    for bad in [far, hole, many, early, negative, bent] {
        let mut t = Session::new().unwrap();
        assert!(matches!(
            t.open_project(&bad),
            Err(SessionError::BadProject(_))
        ));
        assert!(t.view().is_err(), "a refused file opened something");
    }
}

#[test]
fn t39_4_a_brief_made_elsewhere_is_repaired_and_opened_like_any_other() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../suite/briefs/jazz-swing-1.json"
    ))
    .unwrap();
    let mut brief = t2m_core::Brief::from_json(&text).unwrap();
    brief.time.tempo_bpm = 9_999;
    let mut s = Session::new().unwrap();
    let view = s.open_brief(brief, 4);
    assert!(
        view.tempo_bpm <= 300 && view.tempo_bpm >= 30,
        "{}",
        view.tempo_bpm
    );
    assert_eq!((view.seed, view.style.as_str()), (4, "jazz.swing"));
    assert!(!view.tracks.is_empty() && !view.can_undo);
    s.reroll(&view.tracks[1].id, None).unwrap();
}

#[test]
fn t43_2_the_worst_brief_a_model_can_send_still_composes_in_bounded_time() {
    use t2m_core::brief::{Length, MeterSpec};
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../suite/briefs/pop-generic-1.json"
    ))
    .unwrap();
    for (numerator, denominator) in [(255, 32), (255, 1), (4, 4)] {
        let mut brief = t2m_core::Brief::from_json(&text).unwrap();
        brief.time.meter = MeterSpec::Meter {
            numerator,
            denominator,
            grouping: None,
        };
        brief.time.length = Length::Bars(u16::MAX);
        brief.time.tempo_bpm = 30;
        for section in &mut brief.form {
            section.bars = u16::MAX;
        }
        let started = std::time::Instant::now();
        let mut s = Session::new().unwrap();
        let view = s.open_brief(brief, 1);
        let took = started.elapsed();
        assert!(!view.tracks.is_empty());
        assert!(
            took.as_secs() < 10,
            "{numerator}/{denominator} took {took:?}"
        );
    }
}

fn opened() -> (Session, View) {
    let mut s = Session::new().unwrap();
    let id = s.gallery()[0].id.clone();
    let view = s.open_example(&id, 7).unwrap();
    (s, view)
}

#[test]
fn t45_1_takes_change_nothing_and_differ_from_each_other() {
    let (s, view) = opened();
    let track = view
        .tracks
        .iter()
        .find(|t| t.role == "bass")
        .unwrap_or(&view.tracks[1])
        .clone();
    let takes = s.takes(&track.id, None, 3).unwrap();
    assert_eq!(takes.len(), 3);
    assert!(
        takes
            .iter()
            .all(|t| t.id == track.id && !t.notes.is_empty())
    );
    assert_ne!(
        takes[0].notes, track.notes,
        "take 1 equals the current part"
    );
    assert_ne!(takes[0].notes, takes[1].notes);
    assert_ne!(takes[1].notes, takes[2].notes);
    // Nothing happened to the session: same piece, no undo step, same takes again.
    assert_eq!(s.takes(&track.id, None, 3).unwrap(), takes);
    let mut s = s;
    let after = s.undo().unwrap();
    assert_eq!(after.tracks, view.tracks, "takes must not change the piece");
    assert!(
        !after.can_undo && !after.can_redo,
        "takes must not add history"
    );
}

#[test]
fn t45_2_keeping_take_k_gives_exactly_that_take_in_one_undo_step() {
    let (mut s, view) = opened();
    let id = view.tracks[1].id.clone();
    let takes = s.takes(&id, None, 4).unwrap();
    let kept = s.keep_take(&id, None, 3).unwrap();
    let now = kept.tracks.iter().find(|t| t.id == id).unwrap();
    assert_eq!(now.notes, takes[2].notes);
    // Other parts are untouched.
    for (before, after) in view.tracks.iter().zip(&kept.tracks) {
        if before.id != id {
            assert_eq!(before.notes, after.notes, "{} changed", before.id);
        }
    }
    let undone = s.undo().unwrap();
    assert_eq!(
        undone.tracks, view.tracks,
        "one undo must restore the piece"
    );
    assert!(
        !undone.can_undo,
        "keeping a take must be a single undo step"
    );
}

#[test]
fn t45_3_takes_for_one_section_leave_the_other_sections_alone() {
    let (mut s, view) = opened();
    let id = view.tracks[1].id.clone();
    let first = view.sections[0].id.clone();
    let takes = s.takes(&id, Some(&first), 2).unwrap();
    let before = &view.tracks[1].notes;
    let end = view.sections[0].end;
    for take in &takes {
        let outside = |notes: &[(i64, i64, f64)]| {
            notes
                .iter()
                .filter(|n| n.0 >= end)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(
            outside(&take.notes),
            outside(before),
            "a later section changed"
        );
    }
    let kept = s.keep_take(&id, Some(&first), 2).unwrap();
    assert_eq!(kept.tracks[1].notes, takes[1].notes);
}

#[test]
fn t45_4_bad_requests_are_clean_errors_and_change_nothing() {
    let (mut s, view) = opened();
    let id = view.tracks[1].id.clone();
    for count in [0u8, 9, 255] {
        assert_eq!(
            s.takes(&id, None, count).unwrap_err(),
            SessionError::BadTakes(count)
        );
        assert_eq!(
            s.keep_take(&id, None, count).unwrap_err(),
            SessionError::BadTakes(count)
        );
    }
    assert!(matches!(
        s.takes("nope", None, 2),
        Err(SessionError::NoSuchTrack(_))
    ));
    assert!(matches!(
        s.takes(&id, Some("nope"), 2),
        Err(SessionError::NoSuchSection(_))
    ));
    assert!(matches!(
        s.keep_take("nope", None, 1),
        Err(SessionError::NoSuchTrack(_))
    ));
    s.lock(&id, true).unwrap();
    assert!(matches!(
        s.takes(&id, None, 2),
        Err(SessionError::Locked(_))
    ));
    assert!(matches!(
        s.keep_take(&id, None, 1),
        Err(SessionError::Locked(_))
    ));
    let fresh = Session::new().unwrap();
    assert_eq!(
        fresh.takes("a", None, 2).unwrap_err(),
        SessionError::NothingOpen
    );
    // Eight takes is allowed.
    let (s, view) = opened();
    assert_eq!(s.takes(&view.tracks[1].id, None, 8).unwrap().len(), 8);
}

#[test]
fn t46_1_a_piece_rerolled_very_many_times_still_saves_and_opens_the_same() {
    let (mut a, _) = open("pop-generic-1", 1);
    let id = a.view().unwrap().tracks[1].id.clone();
    let section = a.view().unwrap().sections[0].id.clone();
    // As if rerolled 9,997 times already: eight more must wrap, not break saving.
    a.state
        .as_mut()
        .unwrap()
        .rerolls
        .0
        .insert((id.clone(), section.clone()), 9_997);
    a.keep_take(&id, None, 8).unwrap();
    let count = a.state.as_ref().unwrap().rerolls.count(&id, &section);
    assert!((1..=MAX_REROLLS).contains(&count), "count {count}");
    let shown = a.view().unwrap();
    let file = a.save().unwrap();
    let mut b = Session::new().unwrap();
    let opened = b
        .open_project(&file)
        .expect("a heavily rerolled project must open");
    assert_eq!(opened.tracks, shown.tracks);
    // The preview of the next take still equals what keeping it gives, across the wrap.
    let preview = a.takes(&id, None, 3).unwrap();
    let kept = a.keep_take(&id, None, 3).unwrap();
    assert_eq!(kept.tracks[1].notes, preview[2].notes);
}

#[test]
fn t46_2_keeping_a_section_take_survives_undo_redo_and_save_open_and_touches_nothing_else() {
    let (mut s, before) = opened();
    let id = before.tracks[1].id.clone();
    let section = before.sections[1].id.clone();
    let takes = s.takes(&id, Some(&section), 2).unwrap();
    let kept = s.keep_take(&id, Some(&section), 2).unwrap();
    assert_eq!(kept.tracks[1].notes, takes[1].notes);
    assert_ne!(
        kept.tracks[1].notes, before.tracks[1].notes,
        "the take must differ from the part it replaced"
    );
    for (i, (b, a)) in before.tracks.iter().zip(&kept.tracks).enumerate() {
        if i != 1 {
            assert_eq!(b.notes, a.notes, "part {i} changed");
        }
    }
    // Only the chosen section's notes moved.
    let (start, end) = (before.sections[1].start, before.sections[1].end);
    let outside = |notes: &[(i64, i64, f64)]| {
        notes
            .iter()
            .filter(|n| n.0 < start || n.0 >= end)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(
        outside(&kept.tracks[1].notes),
        outside(&before.tracks[1].notes)
    );
    assert_eq!(s.undo().unwrap().tracks, before.tracks);
    assert_eq!(s.redo().unwrap().tracks, kept.tracks);
    let file = s.save().unwrap();
    let mut other = Session::new().unwrap();
    assert_eq!(other.open_project(&file).unwrap().tracks, kept.tracks);
}
