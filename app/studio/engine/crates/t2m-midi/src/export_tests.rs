use super::*;
use crate::smf::read_notes;
use crate::testkit::*;
use t2m_core::brief::Role;
use t2m_core::{Note, Tuning};

fn seven() -> Tuning {
    Tuning {
        id: "seven".into(),
        cents: vec![0, 171, 343, 514, 686, 857, 1029],
        period: 1200,
        source: "test".into(),
        verified: false,
    }
}

fn twelve(kb: &Knowledge) -> Tuning {
    kb.tuning("12tet").unwrap().clone()
}

fn opts(profile: TuningProfile) -> ExportOptions {
    ExportOptions {
        profile,
        ..ExportOptions::default()
    }
}

fn two_part() -> Score {
    score(vec![
        track(
            "lead",
            Role::Lead,
            "keys.piano",
            vec![note(0, 480, 0), note(480, 480, 343)],
        ),
        track(
            "bass",
            Role::Bass,
            "bass.synth",
            vec![note(0, 960, -1200), note(15_360, 960, 0)],
        ),
    ])
}

fn names(bundle: &ExportBundle) -> Vec<&str> {
    bundle.files.iter().map(|f| f.0.as_str()).collect()
}

#[test]
fn t25_4_the_tuning_file_profile_adds_scala_files_only_for_a_non_twelve_tone_scale() {
    let kb = kb();
    let s = two_part();
    let micro = export_with_tuning(&s, &kb, &seven(), &opts(TuningProfile::TuningFile)).unwrap();
    assert_eq!(
        names(&micro),
        ["piece.mid", "piece.scl", "piece.kbm", "piece.ascl"]
    );
    let scl = String::from_utf8(micro.files[1].1.clone()).unwrap();
    assert!(scl.contains(" 7\n"));
    let plain =
        export_with_tuning(&s, &kb, &twelve(&kb), &opts(TuningProfile::TuningFile)).unwrap();
    assert_eq!(names(&plain), ["piece.mid"]);
    // The other profiles never write tuning files.
    for profile in [TuningProfile::Mpe, TuningProfile::Approx12] {
        let b = export_with_tuning(&s, &kb, &seven(), &opts(profile)).unwrap();
        assert_eq!(names(&b), ["piece.mid"]);
    }
}

#[test]
fn t25_5_a_twelve_tone_score_is_the_same_file_under_every_profile() {
    let kb = kb();
    let s = score(vec![
        track(
            "lead",
            Role::Lead,
            "keys.piano",
            vec![note(0, 480, 0), note(480, 480, 700)],
        ),
        track("kit", Role::Drums, "kit.standard", vec![note(0, 100, 3600)]),
    ]);
    let make = |p| export(&s, &kb, &opts(p)).unwrap();
    let (a, b, c) = (
        make(TuningProfile::Mpe),
        make(TuningProfile::TuningFile),
        make(TuningProfile::Approx12),
    );
    assert_eq!(a.files, b.files);
    assert_eq!(b.files, c.files);
    assert!(a.warnings.is_empty());
}

#[test]
fn t25_6_approx12_writes_no_bends_and_reports_how_many_notes_moved() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(0, 100, 50), note(100, 100, 150), note(200, 100, 300)],
    )]);
    let b = export(&s, &kb, &opts(TuningProfile::Approx12)).unwrap();
    assert_eq!(b.warnings, [MidiWarning::Approximated { notes: 2 }]);
    let read = read_notes(&b.files[0].1).unwrap();
    assert!(read[1].bends.is_empty() && read[1].bend_ranges.is_empty());
}

#[test]
fn t25_7_the_mpe_profile_sets_the_bend_range_on_every_channel_it_uses_at_tick_zero() {
    let kb = kb();
    let s = score(vec![
        track("lead", Role::Lead, "keys.piano", vec![note(0, 480, 50)]),
        track("pad", Role::Pad, "pad.warm", vec![note(0, 480, 0)]),
    ]);
    let b = export(&s, &kb, &opts(TuningProfile::Mpe)).unwrap();
    let read = read_notes(&b.files[0].1).unwrap();
    // The microtonal part owns its own channel plus the spare ones; the plain part has no bends.
    assert!(read[1].bend_ranges.iter().all(|r| r.1 == 48));
    assert!(read[1].bend_ranges.len() > 1);
    assert!(read[2].bend_ranges.is_empty());
    assert!(read[1].bends.iter().all(|b| b.0 == 0));
}

#[test]
fn t25_8_selecting_tracks_a_section_or_one_file_per_track() {
    let kb = kb();
    let s = two_part();
    let only = ExportOptions {
        tracks: TrackSelection::Only(vec!["bass".into()]),
        ..ExportOptions::default()
    };
    let read = read_notes(&export(&s, &kb, &only).unwrap().files[0].1).unwrap();
    assert_eq!(read.len(), 2);
    assert!(read[1].name.starts_with("bass"));
    let b_section = ExportOptions {
        sections: SectionSelection::Only("b".into()),
        ..ExportOptions::default()
    };
    let read = read_notes(&export(&s, &kb, &b_section).unwrap().files[0].1).unwrap();
    assert_eq!(read[0].markers, [(0, "b".to_string())]);
    assert_eq!(read[0].tempo_micros, Some(500_000));
    assert_eq!(read[0].time_signature, Some((4, 4)));
    assert_eq!(read[2].notes.len(), 1);
    assert_eq!(read[2].notes[0].start, 0);
    assert!(read[1].notes.is_empty());
    let each = ExportOptions {
        one_file_per_track: true,
        name: "song".into(),
        ..ExportOptions::default()
    };
    let b = export(&s, &kb, &each).unwrap();
    assert_eq!(names(&b), ["song-lead.mid", "song-bass.mid"]);
    assert_eq!(read_notes(&b.files[1].1).unwrap().len(), 2);
}

#[test]
fn t25_9_unknown_names_are_clear_errors_and_an_empty_selection_still_writes() {
    let kb = kb();
    let s = two_part();
    let bad_track = ExportOptions {
        tracks: TrackSelection::Only(vec!["lead".into(), "nope".into()]),
        ..ExportOptions::default()
    };
    assert_eq!(
        export(&s, &kb, &bad_track),
        Err(MidiError::NoTrack { id: "nope".into() })
    );
    let bad_section = ExportOptions {
        sections: SectionSelection::Only("z".into()),
        ..ExportOptions::default()
    };
    assert_eq!(
        export(&s, &kb, &bad_section),
        Err(MidiError::NoSection { id: "z".into() })
    );
    let mut odd = s.clone();
    odd.meta.tuning_id = "made-up".into();
    assert_eq!(
        export(&odd, &kb, &ExportOptions::default()),
        Err(MidiError::Tuning {
            id: "made-up".into()
        })
    );
    let none = ExportOptions {
        tracks: TrackSelection::Only(vec![]),
        ..ExportOptions::default()
    };
    let read = read_notes(&export(&s, &kb, &none).unwrap().files[0].1).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(
        MidiError::NoTrack { id: "x".into() }.to_string(),
        "no such track: x"
    );
}

#[test]
fn t25_10_a_note_crossing_the_end_of_the_chosen_section_is_clipped_to_it() {
    let kb = kb();
    let long: Note = note(15_000, 2_000, 0);
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![long, note(15_400, 100, 0)],
    )]);
    let only = ExportOptions {
        sections: SectionSelection::Only("a".into()),
        ..ExportOptions::default()
    };
    let read = read_notes(&export(&s, &kb, &only).unwrap().files[0].1).unwrap();
    let ends: Vec<(i64, i64)> = read[1].notes.iter().map(|n| (n.start, n.end)).collect();
    assert_eq!(ends, [(15_000, 15_360)]);
}
