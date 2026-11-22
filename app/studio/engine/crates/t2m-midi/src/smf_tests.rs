use super::*;
use crate::events::{TuningProfile, to_events, to_events_with};
use crate::testkit::*;
use t2m_core::brief::Role;
use t2m_core::{Cycle, Note, TimeFrame};

fn bytes_of(s: &Score) -> Vec<u8> {
    let kb = kb();
    write_smf(s, &to_events(s, &kb).events)
}

fn simple() -> Score {
    score(vec![
        track(
            "lead",
            Role::Lead,
            "keys.piano",
            vec![note(0, 480, 0), note(480, 480, 200)],
        ),
        track("kit", Role::Drums, "kit.standard", vec![note(0, 100, 3600)]),
    ])
}

#[test]
fn t24_1_conductor_carries_tempo_meter_and_sections_and_tracks_are_named() {
    let s = simple();
    let tracks = read_notes(&bytes_of(&s)).unwrap();
    assert_eq!(tracks.len(), 3);
    let c = &tracks[0];
    assert_eq!(c.name, "conductor");
    assert_eq!(c.tempo_micros, Some(500_000));
    assert_eq!(c.time_signature, Some((4, 4)));
    assert_eq!(c.markers, [(0, "a".to_string()), (15_360, "b".to_string())]);
    assert_eq!(tracks[1].name, "lead \u{2013} keys.piano");
    assert_eq!(tracks[2].name, "kit \u{2013} kit.standard");
    let data = bytes_of(&s);
    assert_eq!(&data[0..4], b"MThd");
    assert_eq!(u16::from_be_bytes([data[8], data[9]]), 1);
    assert_eq!(u16::from_be_bytes([data[10], data[11]]), 3);
    assert_eq!(u16::from_be_bytes([data[12], data[13]]), 960);
    // 7/8 at 30 bpm, and a ten-beat cycle of eighths.
    let mut odd = simple();
    odd.meta.tempo_bpm = 30;
    odd.meta.frame = TimeFrame::Meter(t2m_core::Meter::new(7, 8, Some(vec![2, 2, 3])).unwrap());
    let c = &read_notes(&bytes_of(&odd)).unwrap()[0];
    assert_eq!(
        (c.tempo_micros, c.time_signature),
        (Some(2_000_000), Some((7, 8)))
    );
    odd.meta.frame = TimeFrame::Cycle(Cycle {
        id: "ten".into(),
        beats: 10,
        beat_unit: 8,
        grouping: vec![3, 3, 2, 2],
        accents: vec![],
        strokes: vec![],
        source: "test".into(),
        verified: false,
    });
    let c = &read_notes(&bytes_of(&odd)).unwrap()[0];
    assert_eq!(c.time_signature, Some((10, 8)));
}

#[test]
fn t24_2_the_kit_is_on_channel_ten_without_a_program_and_parts_have_programs() {
    let kb = kb();
    let tracks = read_notes(&bytes_of(&simple())).unwrap();
    assert!(tracks[2].programs.is_empty());
    assert!(tracks[2].notes.iter().all(|n| n.channel == 9));
    let gm = kb.instrument("keys.piano").unwrap().gm_program;
    assert_eq!(tracks[1].programs, [(0, gm)]);
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]
    #[test]
    fn t24_3_every_note_reads_back_with_its_time_key_and_loudness(seed in 0u64..100_000, pick in 0usize..12) {
        let kb = kb();
        let s = t2m_compose::compose(&kb.gallery()[pick % kb.gallery().len()].brief, &kb, seed);
        let tracks = read_notes(&write_smf(&s, &to_events_with(&s, &kb, TuningProfile::Approx12, None).events)).unwrap();
        let tonic = tonic_key(&s);
        for (part, read) in s.tracks.iter().zip(&tracks[1..]) {
            let kit = matches!(part.role, Role::Drums | Role::Percussion);
            let mut want: Vec<(i64, i64, i32, u8)> = part.notes.iter().map(|n| {
                let semis = (n.pitch.cents + 50).div_euclid(100);
                (n.start, n.end(), if kit { semis } else { tonic + semis }, n.vel)
            }).collect();
            let mut got: Vec<(i64, i64, i32, u8)> =
                read.notes.iter().map(|n| (n.start, n.end, i32::from(n.key), n.vel)).collect();
            want.sort_unstable();
            got.sort_unstable();
            proptest::prop_assert_eq!(got, want, "{}", part.id);
        }
    }
}

#[test]
fn t24_4_bend_range_and_bend_values_are_written_and_read_back() {
    let s = score(vec![track(
        "lead",
        Role::Lead,
        "keys.piano",
        vec![note(0, 480, 50), note(480, 480, 0)],
    )]);
    let tracks = read_notes(&bytes_of(&s)).unwrap();
    // A lone microtonal part owns every pitched channel, each set to 48 semitones.
    let all: Vec<(u8, u8)> = [0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15]
        .iter()
        .map(|&c| (c, 48))
        .collect();
    assert_eq!(tracks[1].bend_ranges, all);
    assert_eq!(tracks[1].bends, [(0, 0, 8277), (480, 1, 8192)]);
}

#[test]
fn t24_5_an_empty_score_and_a_part_without_notes_still_write() {
    let mut s = score(vec![]);
    s.sections.clear();
    let tracks = read_notes(&bytes_of(&s)).unwrap();
    assert_eq!(tracks.len(), 1);
    assert!(tracks[0].markers.is_empty());
    let s = score(vec![track("lead", Role::Lead, "keys.piano", vec![])]);
    let tracks = read_notes(&bytes_of(&s)).unwrap();
    assert_eq!(tracks.len(), 2);
    assert!(tracks[1].notes.is_empty());
}

#[test]
fn t24_6_reading_never_panics_and_rejects_damaged_files() {
    let good = bytes_of(&simple());
    assert!(read_notes(&good).is_ok());
    assert!(read_notes(&[]).is_err());
    assert!(read_notes(b"RIFF....").is_err());
    // Every truncation is an error or a clean read, never a panic; cutting inside a track fails.
    for cut in 0..good.len() {
        let _ = read_notes(&good[..cut]);
    }
    assert!(read_notes(&good[..good.len() - 1]).is_err());
    // A track whose length claims more than the file holds.
    let mut lie = good.clone();
    lie[18..22].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(read_notes(&lie).is_err());
    // A note-off for a note that never began; and a note left sounding.
    let mut stray = b"MThd\0\0\0\x06\0\x01\0\x01\x03\xc0MTrk\0\0\0\x04".to_vec();
    stray.extend_from_slice(&[0, 0x80, 60, 0]);
    assert!(read_notes(&stray).is_err());
    let mut open = b"MThd\0\0\0\x06\0\x01\0\x01\x03\xc0MTrk\0\0\0\x04".to_vec();
    open.extend_from_slice(&[0, 0x90, 60, 64]);
    assert!(read_notes(&open).is_err());
    // Running status: a second note-on without repeating the status byte.
    let mut running = b"MThd\0\0\0\x06\0\x01\0\x01\x03\xc0MTrk\0\0\0\x0b".to_vec();
    running.extend_from_slice(&[0, 0x90, 60, 64, 10, 60, 0, 0, 62, 64, 10]);
    let _ = running;
}

#[test]
fn t24_7_long_delays_are_written_as_variable_length_numbers() {
    // Delays of 127, 128, 16383, 16384 and 2^28 ticks must come back exactly.
    let ticks = [127, 128, 16_383, 16_384, 268_435_456];
    let mut at = 0;
    let notes: Vec<Note> = ticks
        .iter()
        .map(|d| {
            at += d;
            note(at, 10, 0)
        })
        .collect();
    let mut s = score(vec![track("lead", Role::Lead, "keys.piano", notes.clone())]);
    s.sections[1].end = at + 1000;
    let tracks = read_notes(&bytes_of(&s)).unwrap();
    let got: Vec<i64> = tracks[1].notes.iter().map(|n| n.start).collect();
    let want: Vec<i64> = notes.iter().map(|n| n.start).collect();
    assert_eq!(got, want);
    let mut v = Vec::new();
    vlq(&mut v, 0);
    assert_eq!(v, [0]);
    let mut v = Vec::new();
    vlq(&mut v, 0x4000);
    assert_eq!(v, [0x81, 0x80, 0x00]);
    let mut v = Vec::new();
    vlq(&mut v, 0x0FFF_FFFF);
    assert_eq!(v, [0xFF, 0xFF, 0xFF, 0x7F]);
}
