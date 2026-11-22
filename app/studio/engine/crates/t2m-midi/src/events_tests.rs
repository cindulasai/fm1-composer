use super::*;
use crate::testkit::*;
use t2m_core::Tuning;

fn kinds(stream: &EventStream, track: usize) -> Vec<(Tick, EventKind)> {
    stream
        .events
        .iter()
        .filter(|e| e.track == track)
        .map(|e| (e.tick, e.kind))
        .collect()
}

fn seven() -> Tuning {
    Tuning {
        id: "seven".into(),
        cents: vec![0, 171, 343, 514, 686, 857, 1029],
        period: 1200,
        source: "test".into(),
        verified: false,
    }
}

#[test]
fn t23_1_a_twelve_tone_score_gives_each_part_one_channel_and_no_bends() {
    let kb = kb();
    let s = score(vec![
        track(
            "lead",
            Role::Lead,
            "keys.piano",
            vec![note(0, 480, 0), note(480, 480, 200)],
        ),
        track("bass", Role::Bass, "bass.synth", vec![note(0, 960, -1200)]),
        track("kit", Role::Drums, "kit.standard", vec![note(0, 100, 3600)]),
    ]);
    let stream = to_events(&s, &kb);
    assert!(stream.warnings.is_empty());
    assert!(
        !stream
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Bend { .. }))
    );
    let channel = |i: usize| {
        let mut set: Vec<u8> = stream
            .events
            .iter()
            .filter(|e| e.track == i)
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { channel, .. } | EventKind::NoteOff { channel, .. } => {
                    Some(channel)
                }
                _ => None,
            })
            .collect();
        set.dedup();
        set
    };
    assert_eq!(channel(0), [0]);
    assert_eq!(channel(1), [1]);
    assert_eq!(channel(2), [9]);
    let program = |i: usize| {
        kinds(&stream, i).into_iter().find_map(|(_, k)| match k {
            EventKind::Program { channel, program } => Some((channel, program)),
            _ => None,
        })
    };
    assert_eq!(
        program(0),
        Some((0, kb.instrument("keys.piano").unwrap().gm_program))
    );
    assert_eq!(
        program(1),
        Some((1, kb.instrument("bass.synth").unwrap().gm_program))
    );
    assert_eq!(program(2), None);
    // The kit's key is its pitch in hundreds of cents; pitched keys are tonic + semitones.
    assert!(stream.events.iter().any(|e| e.track == 2
        && matches!(
            e.kind,
            EventKind::NoteOn {
                key: 36,
                vel: 90,
                ..
            }
        )));
    let t = tonic_key(&s) as u8;
    assert!(
        stream
            .events
            .iter()
            .any(|e| e.track == 1
                && matches!(e.kind, EventKind::NoteOn { key, .. } if key == t - 12))
    );
}

#[test]
fn t23_2_a_quarter_tone_is_a_bend_before_its_note_on_a_channel_set_to_48_semitones() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(0, 480, 150)],
    )]);
    let stream = to_events(&s, &kb);
    let events = kinds(&stream, 0);
    let t = tonic_key(&s) as u8;
    // 150 cents is exactly between two keys; it takes the lower and bends 50 above.
    let on = events
        .iter()
        .position(|e| matches!(e.1, EventKind::NoteOn { .. }))
        .unwrap();
    assert!(matches!(events[on].1, EventKind::NoteOn { key, .. } if key == t + 1));
    assert_eq!(
        events[on - 1].1,
        EventKind::Bend {
            channel: 0,
            value: 8277
        }
    );
    assert!(events.iter().any(|e| e.0 == 0
        && e.1
            == EventKind::BendRange {
                channel: 0,
                semitones: 48
            }));
    let up = to_events(
        &score(vec![track(
            "l",
            Role::Lead,
            "keys.piano",
            vec![note(0, 480, 50)],
        )]),
        &kb,
    );
    assert!(kinds(&up, 0).iter().any(|e| e.1
        == EventKind::Bend {
            channel: 0,
            value: 8277
        }));
    // Unbent (12-tone) notes in a track with microtones still get a centre bend first.
    let mixed = score(vec![track(
        "l",
        Role::Lead,
        "keys.piano",
        vec![note(0, 100, 0), note(960, 100, 50)],
    )]);
    let events = kinds(&to_events(&mixed, &kb), 0);
    assert!(events.iter().any(|e| e.0 == 0
        && e.1
            == EventKind::Bend {
                channel: 0,
                value: 8192
            }));
}

#[test]
fn t23_3_a_bend_curve_becomes_a_bend_per_point_inside_the_note() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![bent(
            1000,
            960,
            0,
            &[(0, 0), (480, 100), (5000, 4800), (-5, -4800)],
        )],
    )]);
    let bends: Vec<(i64, u16)> = kinds(&to_events(&s, &kb), 0)
        .into_iter()
        .filter_map(|(t, k)| match k {
            EventKind::Bend { value, .. } => Some((t, value)),
            _ => None,
        })
        .collect();
    // The first point is the key's own bend, then one per curve point; late and early
    // points are pulled inside the note and the value is clamped to the 14-bit range.
    assert_eq!(
        bends,
        [
            (1000, 8192),
            (1000, 8192),
            (1000, 0),
            (1480, 8363),
            (1959, 16383)
        ]
    );
}

#[test]
fn t23_4_overlapping_microtonal_notes_use_different_channels_oldest_released_first() {
    let kb = kb();
    // Thirteen plain parts leave a microtonal part exactly two channels: 14 and 15.
    let mut tracks: Vec<_> = (0..13)
        .map(|i| {
            track(
                &format!("x{i}"),
                Role::Pad,
                "pad.warm",
                vec![note(0, 100, 0)],
            )
        })
        .collect();
    tracks.push(track(
        "pad",
        Role::Pad,
        "pad.warm",
        vec![
            note(0, 1000, 50),
            note(100, 1000, 150),
            note(1200, 100, 250),
            note(1300, 100, 350),
        ],
    ));
    let stream = to_events(&score(tracks), &kb);
    let ons: Vec<u8> = stream
        .events
        .iter()
        .filter(|e| e.track == 13)
        .filter_map(|e| match e.kind {
            EventKind::NoteOn { channel, .. } => Some(channel),
            _ => None,
        })
        .collect();
    // After both ended (at 1000 and 1100) the channel released first is taken first.
    assert_eq!(ons, [14, 15, 14, 15]);
}

fn ons_and_offs(stream: &EventStream, track: usize) -> Vec<(Tick, bool, u8, u8)> {
    stream
        .events
        .iter()
        .filter(|e| e.track == track)
        .filter_map(|e| match e.kind {
            EventKind::NoteOn { channel, key, .. } => Some((e.tick, true, channel, key)),
            EventKind::NoteOff { channel, key } => Some((e.tick, false, channel, key)),
            _ => None,
        })
        .collect()
}

#[test]
fn t23_5_more_overlapping_microtones_than_channels_cut_the_oldest_and_warn() {
    let kb = kb();
    // One part owns all 15 pitched channels; sixteen long notes overlap.
    let notes: Vec<Note> = (0..16)
        .map(|i| note(i * 10, 1000, 50 + 100 * i as i32))
        .collect();
    let s = score(vec![track("pad", Role::Pad, "pad.warm", notes)]);
    let stream = to_events(&s, &kb);
    assert_eq!(
        stream.warnings,
        [MidiWarning::VoiceLimit {
            track: "pad".into(),
            notes: 1
        }]
    );
    let list = ons_and_offs(&stream, 0);
    assert_eq!(list.iter().filter(|e| e.1).count(), 16);
    assert_eq!(list.iter().filter(|e| !e.1).count(), 16);
    // Each note ends on its own channel and key, never before it began.
    for on in list.iter().filter(|e| e.1) {
        let off = list
            .iter()
            .find(|e| !e.1 && e.2 == on.2 && e.3 == on.3 && e.0 > on.0);
        assert!(off.is_some(), "{on:?} is never switched off");
    }
    // The oldest note was cut where the sixteenth began (tick 150).
    let cut = list.iter().filter(|e| !e.1 && e.0 == 150).count();
    assert_eq!(cut, 1);
}

#[test]
fn t23_6_approx12_moves_to_the_nearest_key_with_ties_up_and_counts_the_moves() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![
            note(0, 100, 50),
            note(100, 100, -50),
            note(200, 100, 49),
            note(300, 100, 0),
        ],
    )]);
    let stream = to_events_with(&s, &kb, TuningProfile::Approx12, None);
    let t = tonic_key(&s) as u8;
    let keys: Vec<u8> = stream
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::NoteOn { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(keys, [t + 1, t, t, t]);
    assert!(
        !stream
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Bend { .. } | EventKind::BendRange { .. }))
    );
    assert_eq!(stream.warnings, [MidiWarning::Approximated { notes: 3 }]);
}

#[test]
fn t23_7_tuning_file_maps_notes_to_degree_keys_and_drops_curves() {
    let kb = kb();
    let tuning = seven();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![
            note(0, 100, 0),
            note(100, 100, 171),
            note(200, 100, 343),
            note(300, 100, 1200),
            note(400, 100, 1371),
            note(500, 100, 600),
            note(600, 100, -100),
            note(700, 100, 1190),
            bent(800, 100, 514, &[(0, 0), (50, 20)]),
        ],
    )]);
    let stream = to_events_with(&s, &kb, TuningProfile::TuningFile, Some(&tuning));
    let t = tonic_key(&s) as u8;
    let keys: Vec<u8> = stream
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::NoteOn { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(
        keys,
        [t, t + 1, t + 2, t + 7, t + 8, t + 3, t - 1, t + 7, t + 3]
    );
    assert!(
        !stream
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Bend { .. }))
    );
    assert_eq!(
        stream.warnings,
        [
            MidiWarning::Approximated { notes: 3 },
            MidiWarning::BendsDropped { notes: 1 }
        ]
    );
}

#[test]
fn t23_8_notes_end_before_new_ones_begin_and_everything_is_in_time_order() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(0, 1000, 0), note(1000, 1000, 0), note(1000, 500, 200)],
    )]);
    let stream = to_events(&s, &kb);
    assert!(stream.events.windows(2).all(|w| w[0].tick <= w[1].tick));
    let at_1000: Vec<bool> = stream
        .events
        .iter()
        .filter(|e| e.tick == 1000)
        .filter_map(|e| match e.kind {
            EventKind::NoteOff { .. } => Some(false),
            EventKind::NoteOn { .. } => Some(true),
            _ => None,
        })
        .collect();
    assert_eq!(at_1000, [false, true, true]);
}

fn assert_well_formed(stream: &EventStream) {
    assert!(stream.events.windows(2).all(|w| w[0].tick <= w[1].tick));
    let mut open: Vec<(usize, u8, u8)> = Vec::new();
    for e in &stream.events {
        match e.kind {
            EventKind::NoteOn { channel, key, vel } => {
                assert!(channel < 16 && key < 128 && (1..128).contains(&vel));
                open.push((e.track, channel, key));
            }
            EventKind::NoteOff { channel, key } => {
                let i = open.iter().position(|o| *o == (e.track, channel, key));
                assert!(i.is_some(), "note off without a note on: {e:?}");
                open.remove(i.unwrap());
            }
            EventKind::Bend { channel, value } => assert!(channel < 16 && value < 16384),
            EventKind::Program { channel, program } => assert!(channel < 16 && program < 128),
            EventKind::BendRange { channel, semitones } => assert!(channel < 16 && semitones <= 48),
        }
    }
    assert!(open.is_empty(), "notes left sounding: {open:?}");
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]
    #[test]
    fn t23_9_every_composed_score_gives_well_formed_events_under_every_profile(
        seed in 0u64..100_000,
        pick in 0usize..12,
        cents in proptest::collection::vec(-2400i32..2400, 0..24),
    ) {
        let kb = kb();
        let brief = kb.gallery()[pick % kb.gallery().len()].brief.clone();
        let mut s = t2m_compose::compose(&brief, &kb, seed);
        // Sprinkle microtones in so every profile has something to carry.
        if let Some(lead) = s.tracks.iter_mut().find(|t| t.role == Role::Lead) {
            for (note, c) in lead.notes.iter_mut().zip(&cents) {
                note.pitch.cents += c % 100;
            }
        }
        let seven = seven();
        for profile in [TuningProfile::Mpe, TuningProfile::TuningFile, TuningProfile::Approx12] {
            assert_well_formed(&to_events_with(&s, &kb, profile, Some(&seven)));
        }
        assert_well_formed(&to_events(&s, &kb));
    }
}

#[test]
fn t23_10_a_tonic_off_the_grid_bends_every_note_or_is_rounded_away() {
    let kb = kb();
    let mut s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(0, 100, 0), note(100, 100, 700)],
    )]);
    s.meta.tonic = t2m_core::Tonic::Hz(440.0 * 2f64.powf(30.0 / 1200.0));
    let mpe = to_events(&s, &kb);
    let bends: Vec<u16> = mpe
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Bend { value, .. } => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(bends, [8243, 8243]);
    let keys = |stream: &EventStream| -> Vec<u8> {
        stream
            .events
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::NoteOn { key, .. } => Some(key),
                _ => None,
            })
            .collect()
    };
    assert_eq!(keys(&mpe), [21, 28]);
    let approx = to_events_with(&s, &kb, TuningProfile::Approx12, None);
    assert_eq!(keys(&approx), [21, 28]);
    assert!(
        !approx
            .events
            .iter()
            .any(|e| matches!(e.kind, EventKind::Bend { .. }))
    );
    assert_eq!(approx.warnings, [MidiWarning::Approximated { notes: 2 }]);
}

#[test]
fn t23_11_a_cut_notes_remaining_curve_does_not_bend_the_note_that_takes_its_channel() {
    let kb = kb();
    // The first note carries bends at 500 and 900; the sixteenth, at tick 150, cuts it.
    let mut notes: Vec<Note> = (0..16)
        .map(|i| note(i * 10, 1000, 50 + 100 * i as i32))
        .collect();
    notes[0] = bent(0, 1000, 50, &[(500, 100), (900, 200)]);
    let s = score(vec![track("pad", Role::Pad, "pad.warm", notes)]);
    let stream = to_events(&s, &kb);
    assert_eq!(
        stream.warnings,
        [MidiWarning::VoiceLimit {
            track: "pad".into(),
            notes: 1
        }]
    );
    let late: Vec<(Tick, u16)> = stream
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Bend { channel: 0, value } if e.tick > 150 => Some((e.tick, value)),
            _ => None,
        })
        .collect();
    assert_eq!(late, []);
    // A curve point before the cut is kept.
    let mut early = vec![bent(0, 1000, 50, &[(100, 100), (900, 200)])];
    early.extend((1..16).map(|i| note(i * 10, 1000, 50 + 100 * i as i32)));
    let stream = to_events(
        &score(vec![track("pad", Role::Pad, "pad.warm", early)]),
        &kb,
    );
    let kept = stream
        .events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::Bend { channel: 0, .. }) && e.tick == 100)
        .count();
    assert_eq!(kept, 1);
}

#[test]
fn t23_12_a_note_with_no_length_still_ends_after_it_begins() {
    let kb = kb();
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(500, 0, 0), note(2000, -5, 50), note(3000, 0, 100)],
    )]);
    for profile in [
        TuningProfile::Mpe,
        TuningProfile::TuningFile,
        TuningProfile::Approx12,
    ] {
        let stream = to_events_with(&s, &kb, profile, None);
        assert_well_formed(&stream);
        let list = ons_and_offs(&stream, 0);
        for on in list.iter().filter(|e| e.1) {
            let off = list
                .iter()
                .find(|e| !e.1 && e.2 == on.2 && e.3 == on.3 && e.0 > on.0);
            assert!(off.is_some(), "{profile:?}: {on:?} has no later off");
        }
    }
}
