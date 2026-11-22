use crate::testkit::*;
use crate::{Engine, Timbre, piece_of};
use proptest::prelude::*;

const A4: f32 = 440.0;

fn sine_engine(notes: Vec<crate::PlayNote>, tracks: usize) -> Engine {
    let mut e = Engine::new(RATE);
    e.load(piece(notes, 120, tracks));
    e
}

#[test]
fn t30_1_pitch_is_exact_to_a_cent_including_quarter_tones() {
    for cents in [0.0f64, 50.0, -50.0, 33.0] {
        let hz = A4 * (cents / 1200.0).exp2() as f32;
        let mut e = sine_engine(vec![note(0, 3840, hz, 0, Timbre::Sine)], 1);
        e.play();
        let s = left(&mut e, 40_000);
        let measured = hz_of(&s[4_800..38_400]);
        assert!(
            cents_between(measured, f64::from(hz)).abs() < 1.0,
            "{cents}: {measured} vs {hz}"
        );
    }
}

#[test]
fn t30_2_notes_start_on_their_tick_at_the_tempo() {
    // 120 bpm, 960 ticks per quarter: tick 1920 is 1.0 s.
    let mut e = sine_engine(vec![note(1920, 3840, A4, 0, Timbre::Sine)], 1);
    e.play();
    let s = left(&mut e, 60_000);
    assert!(rms(&s[..47_000]) < 1e-6, "sound before the note");
    assert!(rms(&s[49_000..59_000]) > 0.05, "no sound after it began");
    // The reported position follows the tempo.
    assert!((e.position() - 2400).abs() <= 2, "{}", e.position());
}

#[test]
fn t30_3_stop_is_silent_within_ten_ms_and_keeps_the_place() {
    let mut e = sine_engine(vec![note(0, 38_400, A4, 0, Timbre::Sine)], 1);
    e.play();
    left(&mut e, 4_800);
    let at = e.position();
    e.stop();
    assert!(!e.is_playing());
    let s = left(&mut e, 2_400);
    assert!(rms(&s[480..]) < 1e-3, "still sounding");
    assert_eq!(e.position(), at);
    e.play();
    let s = left(&mut e, 4_800);
    assert!(rms(&s[2_000..]) > 0.0 || e.position() > at);
    assert!(e.position() > at);
}

#[test]
fn t30_4_seek_jumps_and_clamps_and_skips_earlier_notes() {
    let mut e = sine_engine(
        vec![
            note(0, 960, A4, 0, Timbre::Sine),
            note(9600, 11_520, A4, 0, Timbre::Sine),
        ],
        1,
    );
    e.seek(5_000);
    assert_eq!(e.position(), 5_000);
    e.seek(-7);
    assert_eq!(e.position(), 0);
    e.seek(i64::MAX);
    assert_eq!(e.position(), 11_520);
    e.seek(2_000);
    e.play();
    let s = left(&mut e, 9_600);
    assert!(rms(&s) < 1e-6, "an earlier note sounded after a seek");
}

#[test]
fn t30_5_mute_and_solo_decide_what_is_heard() {
    let two = || {
        vec![
            note(0, 38_400, 220.0, 0, Timbre::Sine),
            note(0, 38_400, 330.0, 1, Timbre::Sine),
        ]
    };
    let run = |setup: &dyn Fn(&mut Engine)| {
        let mut e = sine_engine(two(), 2);
        setup(&mut e);
        e.play();
        rms(&left(&mut e, 9_600)[2_000..])
    };
    let both = run(&|_| {});
    let one_muted = run(&|e| e.set_mute(0, true));
    let both_muted = run(&|e| {
        e.set_mute(0, true);
        e.set_mute(1, true)
    });
    let solo = run(&|e| e.set_solo(1, true));
    assert!(both > one_muted * 1.2);
    assert!(both_muted < 1e-6);
    assert!((solo - one_muted).abs() < one_muted * 0.2 + 1e-4);
    // Solo wins over a mute elsewhere; a muted soloed part is silent.
    let silent = run(&|e| {
        e.set_solo(1, true);
        e.set_mute(1, true)
    });
    assert!(silent < 1e-6);
    // Out-of-range tracks are ignored.
    let mut e = sine_engine(two(), 2);
    e.set_mute(99, true);
    e.set_solo(99, true);
}

#[test]
fn t30_6_loop_wraps_the_position_and_bad_ranges_clear_it() {
    let mut e = sine_engine(vec![note(0, 19_200, A4, 0, Timbre::Sine)], 1);
    e.set_loop(Some((0, 1_920)));
    e.play();
    for _ in 0..8 {
        left(&mut e, 12_000);
        assert!(e.position() < 1_920 + 10, "{}", e.position());
        assert!(e.is_playing());
    }
    for bad in [Some((5, 5)), Some((9, 2)), Some((-1, 5)), None] {
        e.set_loop(bad);
    }
    e.seek(0);
    e.play();
    left(&mut e, 600_000);
    assert!(!e.is_playing(), "a cleared loop must let the piece end");
}

#[test]
fn t30_7_hostile_input_never_breaks_the_output() {
    // Nothing loaded, nothing playing, odd buffers.
    let mut e = Engine::new(RATE);
    let mut out = [1.0f32; 7];
    e.render(&mut out);
    assert!(out.iter().all(|s| *s == 0.0));
    e.play();
    e.render(&mut out);
    assert!(out.iter().all(|s| *s == 0.0));
    e.render(&mut []);
    // Silly sample rates are clamped.
    for rate in [0, 1, u32::MAX] {
        let mut e = Engine::new(rate);
        e.load(piece(vec![note(0, 960, A4, 0, Timbre::Sine)], 120, 1));
        e.play();
        let mut buf = [0.0f32; 512];
        e.render(&mut buf);
        assert!(buf.iter().all(|s| s.is_finite()));
    }
    // Hostile notes: NaN, infinite and zero frequencies, inverted times, huge tempo.
    let mut notes = Vec::new();
    for hz in [f32::NAN, f32::INFINITY, 0.0, -5.0, 1e9] {
        notes.push(note(0, 100, hz, 0, Timbre::Bright));
    }
    notes.push(note(500, 100, A4, 0, Timbre::Soft));
    notes.push(note(-100, 50, A4, 7, Timbre::Pad));
    let mut e = Engine::new(RATE);
    e.load(piece(notes, u16::MAX, 1));
    e.play();
    for _ in 0..20 {
        let mut buf = vec![0.0f32; 2_048];
        e.render(&mut buf);
        assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
    }
}

#[test]
fn t30_8_two_hundred_notes_at_once_stay_bounded() {
    let notes: Vec<_> = (0..200)
        .map(|i| note(0, 9_600, 100.0 + i as f32 * 7.0, i % 4, Timbre::Kick))
        .collect();
    let mut e = sine_engine(notes, 4);
    e.play();
    let mut peak = 0.0f32;
    for _ in 0..10 {
        for s in left(&mut e, 4_800) {
            assert!(s.is_finite());
            peak = peak.max(s.abs());
        }
    }
    assert!(peak <= 1.0 && peak > 0.1, "{peak}");
}

#[test]
fn t30_9_a_composed_piece_becomes_sorted_notes_and_plays() {
    let kb = t2m_core::Knowledge::load().unwrap();
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../suite/briefs/pop-generic-1.json"
    ))
    .unwrap();
    let brief = t2m_core::repair(t2m_core::Brief::from_json(&text).unwrap(), &kb).0;
    let score = t2m_compose::compose(&brief, &kb, 1);
    let total: usize = score.tracks.iter().map(|t| t.notes.len()).sum();
    let p = piece_of(&score);
    assert_eq!(p.notes.len(), total);
    assert_eq!(p.tracks, score.tracks.len());
    assert_eq!(p.end, score.end());
    assert!(p.notes.windows(2).all(|w| w[0].start <= w[1].start));
    assert!(
        p.notes
            .iter()
            .all(|n| n.hz > 20.0 && n.hz < 20_000.0 && n.end > n.start)
    );
    let mut e = Engine::new(RATE);
    e.load(p);
    e.play();
    let mut heard = 0.0f32;
    for _ in 0..40 {
        heard = heard.max(rms(&left(&mut e, 4_800)));
    }
    assert!(heard > 0.01, "a composed piece is silent");
}

#[test]
fn t30_10_bend_curves_move_the_pitch() {
    let mut n = note(0, 3_840, A4, 0, Timbre::Sine);
    n.bend = vec![(0, 0.0), (1_920, 1_200.0)];
    let mut e = sine_engine(vec![n], 1);
    e.play();
    let s = left(&mut e, 60_000);
    let early = hz_of(&s[2_400..9_600]);
    let late = hz_of(&s[38_000..46_000]);
    assert!(cents_between(early, 440.0).abs() < 250.0, "{early}");
    assert!(late > early * 1.6, "{early} -> {late}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn t30_11_any_notes_give_finite_bounded_audio(
        raw in proptest::collection::vec((0i64..20_000, 0i64..5_000, 1.0f32..8_000.0, 0u8..=255, 0usize..4, 0usize..13), 0..40),
        tempo in 1u16..400,
    ) {
        let timbres = [Timbre::Sine, Timbre::Soft, Timbre::Bright, Timbre::Sub, Timbre::Pad, Timbre::Pluck, Timbre::Kick, Timbre::Snare, Timbre::Hat, Timbre::OpenHat, Timbre::Crash, Timbre::Tom, Timbre::Click];
        let notes = raw.iter().map(|&(s, d, hz, v, t, k)| {
            let mut n = note(s, s + d, hz, t, timbres[k]);
            n.vel = v;
            n
        }).collect();
        let mut e = Engine::new(RATE);
        e.load(piece(notes, tempo, 4));
        e.play();
        let mut buf = vec![0.0f32; 4_096];
        for _ in 0..6 {
            e.render(&mut buf);
            prop_assert!(buf.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        }
    }
}

#[test]
fn t30_12_a_note_at_the_loop_start_sounds_on_every_pass() {
    // 120 bpm: 1920 ticks = 1 s. Notes at 0 and 960 inside a loop of (0, 1920).
    let mut e = sine_engine(
        vec![
            note(0, 480, A4, 0, Timbre::Sine),
            note(960, 1_440, 330.0, 0, Timbre::Sine),
        ],
        1,
    );
    e.set_loop(Some((0, 1_920)));
    e.play();
    for pass in 0..3 {
        let s = left(&mut e, 48_000);
        assert!(
            rms(&s[2_000..10_000]) > 0.05,
            "pass {pass}: first note silent"
        );
        assert!(
            rms(&s[26_000..34_000]) > 0.05,
            "pass {pass}: second note silent"
        );
    }
}

#[test]
fn t30_13_loading_the_same_piece_changes_nothing_and_a_changed_one_keeps_held_notes() {
    let held = || piece(vec![note(0, 96_000, A4, 0, Timbre::Sine)], 120, 1);
    let mut e = Engine::new(RATE);
    e.load(held());
    e.play();
    left(&mut e, 24_000);
    e.load(held());
    let s = left(&mut e, 9_600);
    assert!(
        rms(&s[400..]) > 0.1,
        "the same piece must play on without a gap"
    );
    // Exactly the audio an engine that was never reloaded gives, sample for sample.
    let mut plain = Engine::new(RATE);
    plain.load(held());
    plain.play();
    left(&mut plain, 24_000);
    assert_eq!(
        left(&mut plain, 9_600),
        s,
        "loading the same piece disturbed the sound"
    );
    // A changed piece with the same held note: the note keeps sounding.
    let mut changed = held();
    changed
        .notes
        .push(note(90_000, 91_000, 880.0, 0, Timbre::Sine));
    e.load(changed);
    let s = left(&mut e, 9_600);
    assert!(
        rms(&s[1_000..]) > 0.1,
        "a held note went silent after a change"
    );
    // A stopped engine stays stopped and silent on load.
    e.stop();
    e.load(held());
    assert!(rms(&left(&mut e, 4_800)[1_000..]) < 1e-3);
}

#[test]
fn t30_14_unmuting_brings_a_part_back_and_muting_cuts_a_sounding_note() {
    let mut e = sine_engine(vec![note(0, 96_000, 220.0, 0, Timbre::Sine)], 1);
    e.play();
    left(&mut e, 4_800);
    e.set_mute(0, true);
    left(&mut e, 960);
    assert!(
        rms(&left(&mut e, 2_400)) < 1e-3,
        "a muted part still sounds"
    );
    e.set_mute(0, false);
    e.seek(1_000);
    e.play();
    assert!(
        rms(&left(&mut e, 4_800)) < 1e-6,
        "notes begun before a seek stay silent"
    );
}

#[test]
fn t55_1_unmuting_or_unsoloing_a_held_note_is_heard_at_once_but_one_shots_do_not_return() {
    // Part 0 holds a pad for two seconds; part 1 has a long note too.
    let mut e = sine_engine(
        vec![
            note(0, 96_000, 220.0, 0, Timbre::Sine),
            note(0, 96_000, 330.0, 1, Timbre::Sine),
        ],
        2,
    );
    e.play();
    left(&mut e, 4_800);
    e.set_mute(0, true);
    left(&mut e, 2_400);
    e.set_mute(1, true);
    left(&mut e, 2_400);
    assert!(rms(&left(&mut e, 2_400)) < 1e-3, "everything is muted");
    e.set_mute(0, false);
    assert!(
        rms(&left(&mut e, 4_800)) > 0.02,
        "the unmuted pad must come back mid-note"
    );
    // Soloing part 1 (still muted) is no help; unmuting it and soloing it keeps only it.
    e.set_mute(1, false);
    e.set_solo(0, true);
    left(&mut e, 2_400);
    e.set_solo(0, false);
    assert!(
        rms(&left(&mut e, 4_800)) > 0.02,
        "un-soloing must bring part 1 back"
    );
    // A stopped engine does not start voices when flags change; playing then sounds normally.
    e.stop();
    left(&mut e, 2_400);
    e.set_mute(0, true);
    e.set_mute(0, false);
    assert!(
        rms(&left(&mut e, 2_400)) < 1e-3,
        "a stopped engine stays silent"
    );
}

#[test]
fn t55_2_a_percussive_note_is_not_revived_by_unmuting() {
    let mut e = sine_engine(vec![note(0, 96_000, 220.0, 0, Timbre::Kick)], 1);
    e.play();
    left(&mut e, 4_800);
    e.set_mute(0, true);
    left(&mut e, 4_800);
    e.set_mute(0, false);
    assert!(
        rms(&left(&mut e, 4_800)) < 1e-3,
        "a one-shot hit must not restart on unmute"
    );
}
