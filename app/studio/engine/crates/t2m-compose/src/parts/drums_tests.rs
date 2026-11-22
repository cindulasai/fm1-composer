use t2m_core::brief::{Brief, Length, MeterSpec, Role, Transition};
use t2m_core::groove::Band;

use super::*;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, section, track, with};

const GROOVES: [&str; 12] = [
    "groove.rock_8",
    "groove.pop_four",
    "groove.house",
    "groove.techno",
    "groove.trap",
    "groove.boom_bap",
    "groove.lofi_swing",
    "groove.funk_16",
    "groove.jazz_ride",
    "groove.one_drop",
    "groove.bossa",
    "groove.afrobeats",
];

/// One section of drums only, with the given feel.
fn beat(feel: &str, bars: u16, edit: impl FnOnce(&mut Brief)) -> Brief {
    with(|b| {
        b.time.feel = feel.into();
        b.tracks = vec![
            track("drums", Role::Drums, "kit.standard"),
            track("lead", Role::Lead, "keys.piano"),
        ];
        b.form = vec![section("a", bars, &["drums"])];
        b.time.length = Length::Bars(bars);
        edit(b);
    })
}

fn play(brief: &Brief, seed: u64) -> Vec<Vec<Note>> {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), seed, &rerolls).expect("a valid brief resolves");
    let drums_track = plan.track("drums").unwrap();
    plan.sections
        .iter()
        .map(|section| drums(&plan, drums_track, section))
        .collect()
}

fn piece(name: &str) -> i32 {
    let kit = &kb().instrument("kit.standard").unwrap().kit;
    i32::from(kit.iter().find(|piece| piece.name == name).unwrap().note) * 100
}

fn starts(notes: &[Note], name: &str) -> Vec<Tick> {
    notes
        .iter()
        .filter(|n| n.pitch.cents == piece(name))
        .map(|n| n.start)
        .collect()
}

#[test]
fn t11_4_every_hit_is_a_piece_of_the_kit() {
    let pieces: Vec<i32> = kb()
        .instrument("kit.standard")
        .unwrap()
        .kit
        .iter()
        .map(|p| i32::from(p.note) * 100)
        .collect();
    for feel in GROOVES {
        for energy in [0.1, 0.5, 0.9] {
            let brief = beat(feel, 8, |b| {
                b.form[0].energy = energy;
                b.form[0].transition = Transition::FillHeavy;
            });
            let notes = &play(&brief, 3)[0];
            assert!(notes.len() >= 8, "{feel} at {energy}: {} hits", notes.len());
            for note in notes {
                assert!(
                    pieces.contains(&note.pitch.cents),
                    "{feel}: {}",
                    note.pitch.cents
                );
                assert!(
                    note.dur > 0 && note.start >= 0 && note.end() <= 8 * 3840,
                    "{feel}"
                );
            }
        }
    }
}

#[test]
fn t11_5_the_pattern_follows_the_sections_energy_band() {
    assert_eq!(Band::of(0.0), Band::Low);
    assert_eq!(Band::of(0.33), Band::Low);
    assert_eq!(Band::of(0.34), Band::Mid);
    assert_eq!(Band::of(0.66), Band::Mid);
    assert_eq!(Band::of(0.67), Band::High);
    assert_eq!(Band::of(1.0), Band::High);
    // Rock: the low patterns use the closed hi-hat only; the high ones a ride or open hat.
    for seed in 1..=10 {
        let low = &play(&beat("groove.rock_8", 4, |b| b.form[0].energy = 0.1), seed)[0];
        assert!(!starts(low, "closed_hat").is_empty());
        assert!(starts(low, "ride").is_empty() && starts(low, "open_hat").is_empty());
        let high = &play(&beat("groove.rock_8", 4, |b| b.form[0].energy = 0.9), seed)[0];
        assert!(starts(high, "closed_hat").is_empty());
        assert!(!starts(high, "ride").is_empty() || !starts(high, "open_hat").is_empty());
    }
    // House: four on the floor in every band.
    for energy in [0.1, 0.5, 0.9] {
        let notes = &play(&beat("groove.house", 2, |b| b.form[0].energy = energy), 1)[0];
        assert_eq!(
            starts(notes, "kick"),
            [0, 960, 1920, 2880, 3840, 4800, 5760, 6720]
        );
    }
    // One drop: nothing from the kick on beat one in the low band; kick and stick together on three.
    let notes = &play(&beat("groove.one_drop", 2, |b| b.form[0].energy = 0.1), 1)[0];
    assert_eq!(starts(notes, "kick"), [1920, 5760]);
    assert_eq!(starts(notes, "side_stick")[0], 1920);
}

#[test]
fn t11_6_fills_come_only_where_a_section_asks_for_one() {
    for feel in ["groove.rock_8", "groove.funk_16", "groove.house"] {
        for seed in 1..=6 {
            let plain = beat(feel, 4, |b| b.form[0].energy = 0.1);
            let filled = beat(feel, 4, |b| {
                b.form[0].energy = 0.1;
                b.form[0].transition = Transition::FillLight;
            });
            let (plain, filled) = (&play(&plain, seed)[0], &play(&filled, seed)[0]);
            // The first three bars are the same with or without the fill.
            let early = |notes: &[Note]| -> Vec<Note> {
                notes
                    .iter()
                    .filter(|n| n.start < 3 * 3840)
                    .cloned()
                    .collect()
            };
            assert_eq!(early(plain), early(filled), "{feel} seed {seed}");
            // The last bar differs, and only from the fill's first hit: its first half is the groove.
            let half = 3 * 3840 + 1920;
            let before = |notes: &[Note]| -> Vec<Note> {
                notes
                    .iter()
                    .filter(|n| n.start >= 3 * 3840 && n.start < half)
                    .cloned()
                    .collect()
            };
            assert_eq!(before(plain), before(filled), "{feel} seed {seed}");
            let late = |notes: &[Note]| -> Vec<(Tick, i32)> {
                notes
                    .iter()
                    .filter(|n| n.start >= half)
                    .map(|n| (n.start, n.pitch.cents))
                    .collect()
            };
            assert_ne!(late(plain), late(filled), "{feel} seed {seed}");
        }
    }
    // A heavy fill starts earlier in the bar than a light one.
    let first_fill_hit = |transition: Transition, seed: u64| -> Tick {
        let plain = &play(&beat("groove.rock_8", 2, |b| b.form[0].energy = 0.1), seed)[0];
        let filled = &play(
            &beat("groove.rock_8", 2, |b| {
                b.form[0].energy = 0.1;
                b.form[0].transition = transition;
            }),
            seed,
        )[0];
        let plain: Vec<(Tick, i32)> = plain.iter().map(|n| (n.start, n.pitch.cents)).collect();
        filled
            .iter()
            .map(|n| (n.start, n.pitch.cents))
            .find(|hit| !plain.contains(hit))
            .unwrap()
            .0
    };
    for seed in 1..=6 {
        assert!(first_fill_hit(Transition::FillHeavy, seed) - 3840 < 2880);
        assert!(first_fill_hit(Transition::FillLight, seed) - 3840 >= 2880);
    }
}

#[test]
fn t11_7_crash_and_break_transitions() {
    let two = |first: Transition| {
        beat("groove.rock_8", 4, |b| {
            b.form = vec![section("a", 2, &["drums"]), section("b", 2, &["drums"])];
            b.form[0].transition = first;
        })
    };
    let crash_at = |transition: Transition| starts(&play(&two(transition), 1)[1], "crash");
    assert_eq!(crash_at(Transition::Crash), [2 * 3840]);
    assert_eq!(crash_at(Transition::FillHeavy), [2 * 3840]);
    assert!(crash_at(Transition::None).is_empty());
    assert!(crash_at(Transition::FillLight).is_empty());
    assert!(
        starts(&play(&two(Transition::Crash), 1)[0], "crash").is_empty(),
        "the first section is entered by nothing"
    );

    // A break leaves the last beat of the section empty, and nothing else.
    let sections = play(&two(Transition::Break), 1);
    let plain = play(&two(Transition::None), 1);
    let last_beat = 2 * 3840 - 960;
    assert!(sections[0].iter().all(|n| n.start < last_beat));
    assert!(plain[0].iter().any(|n| n.start >= last_beat));
    let before: Vec<&Note> = plain[0].iter().filter(|n| n.start < last_beat).collect();
    assert_eq!(sections[0].iter().collect::<Vec<_>>(), before);
}

#[test]
fn t11_8_loudness_varies_and_ghost_notes_stay_soft() {
    for feel in GROOVES {
        for seed in 1..=3 {
            let notes = &play(&beat(feel, 4, |b| b.form[0].energy = 0.5), seed)[0];
            for bar in 0..4 {
                let vels: Vec<u8> = notes
                    .iter()
                    .filter(|n| n.start / 3840 == bar)
                    .map(|n| n.vel)
                    .collect();
                assert!(
                    vels.iter().any(|v| *v != vels[0]),
                    "{feel} bar {bar}: all {}",
                    vels[0]
                );
                assert!(vels.iter().all(|v| (1..=127).contains(v)));
            }
        }
    }
    // Funk's snare ghosts: every snare hit off the backbeat is a ghost, at 50 or below.
    for seed in 1..=10 {
        let notes = &play(&beat("groove.funk_16", 4, |b| b.form[0].energy = 0.5), seed)[0];
        let snare: Vec<&Note> = notes
            .iter()
            .filter(|n| n.pitch.cents == piece("snare"))
            .collect();
        let (backbeat, ghosts): (Vec<&Note>, Vec<&Note>) =
            snare.into_iter().partition(|n| n.start % 1920 == 960);
        assert!(!ghosts.is_empty() && !backbeat.is_empty());
        assert!(ghosts.iter().all(|n| n.vel <= 50), "seed {seed}");
        assert!(backbeat.iter().all(|n| n.vel > 90), "seed {seed}");
    }
    // A part that is not busy leaves the ghosts out.
    let calm = beat("groove.funk_16", 4, |b| {
        b.form[0].energy = 0.5;
        b.tracks[0].busy = 0.34;
    });
    assert!(play(&calm, 1)[0].iter().all(|n| n.vel > 50));
    let busy = beat("groove.funk_16", 4, |b| {
        b.form[0].energy = 0.5;
        b.tracks[0].busy = 0.35;
    });
    assert!(play(&busy, 1)[0].iter().any(|n| n.vel <= 50));
}

#[test]
fn t11_9_swing_depends_on_tempo() {
    let jazz = kb().groove("groove.jazz_ride").unwrap();
    assert_eq!(jazz.swing_ratio(40), 3500);
    assert_eq!(jazz.swing_ratio(60), 3500);
    assert_eq!(jazz.swing_ratio(90), 3350);
    assert_eq!(jazz.swing_ratio(120), 3200);
    assert_eq!(jazz.swing_ratio(175), 2500);
    assert_eq!(jazz.swing_ratio(240), 1500);
    assert_eq!(jazz.swing_ratio(300), 1000);
    assert_eq!(jazz.swing_ratio(u16::MAX), 1000);
    assert!(jazz.swing_ratio(120) > jazz.swing_ratio(240));
    assert_eq!(kb().groove("groove.rock_8").unwrap().swing_ratio(120), 1000);

    // The ride's "and of two": late by the swing, and at 240 bpm its short note still lasts 100 ms.
    let offbeat = |tempo: u16| -> Tick {
        let brief = beat("groove.jazz_ride", 1, |b| {
            b.time.tempo_bpm = tempo;
            b.form[0].energy = 0.1;
        });
        let ride = starts(&play(&brief, 1)[0], "ride");
        // How far into its beat the first off-beat ride note falls.
        ride.iter()
            .map(|start| start % 960)
            .find(|within| *within != 0)
            .unwrap()
    };
    // At 240 bpm a beat is 960 ticks and 250 ms. Ratio 1.5:1 puts the off-beat at 960 * 1.5 / 2.5 = 576.
    assert_eq!(offbeat(240), 576);
    let short_ms = (960 - offbeat(240)) * 250 / 960;
    assert_eq!(short_ms, 100);
    assert!(short_ms >= 90);
    // At 120 bpm the ratio is 3.2:1: 960 * 3.2 / 4.2 = 731.
    assert_eq!(offbeat(120), 731);
    // At 300 bpm it is straight.
    assert_eq!(offbeat(300), 480);
    // A straight groove never moves its off-beats.
    let rock = &play(&beat("groove.rock_8", 1, |b| b.form[0].energy = 0.1), 1)[0];
    assert_eq!(
        starts(rock, "closed_hat"),
        [0, 480, 960, 1440, 1920, 2400, 2880, 3360]
    );
}

#[test]
fn t11_10_offsets_never_leave_the_section_at_any_tempo() {
    for tempo in [30, 300] {
        for feel in ["groove.jazz_ride", "groove.lofi_swing", "groove.boom_bap"] {
            let brief = beat(feel, 3, |b| {
                b.time.tempo_bpm = tempo;
                b.form = vec![section("a", 1, &["drums"]), section("b", 2, &["drums"])];
                b.form[0].transition = Transition::FillHeavy;
                b.form[1].transition = Transition::FillLight;
            });
            let rerolls = Rerolls::default();
            let plan = resolve(&brief, kb(), 5, &rerolls).unwrap();
            for (section, notes) in plan.sections.iter().zip(play(&brief, 5)) {
                assert!(!notes.is_empty());
                for note in &notes {
                    assert!(
                        note.start >= section.start && note.start >= 0,
                        "{feel} at {tempo}"
                    );
                    assert!(note.dur > 0, "{feel} at {tempo}: zero-length hit");
                    assert!(note.end() <= section.end, "{feel} at {tempo}");
                }
                assert!(notes.windows(2).all(|pair| pair[0].start <= pair[1].start));
            }
        }
    }
    // The lo-fi snare is played late: 14 ticks behind beat two.
    let lofi = &play(&beat("groove.lofi_swing", 1, |b| b.form[0].energy = 0.1), 1)[0];
    let mut backbeat = starts(lofi, "snare");
    backbeat.extend(starts(lofi, "side_stick"));
    assert_eq!(backbeat, [960 + 14, 2880 + 14]);
}

#[test]
fn t11_11_any_meter_gets_a_plain_beat() {
    // A feel with no patterns of its own.
    let plain = &play(&beat("groove.straight_8", 2, |_| {}), 1)[0];
    assert_eq!(starts(plain, "kick"), [0, 1920, 3840, 5760]);
    assert_eq!(starts(plain, "snare"), [960, 2880, 4800, 6720]);
    assert_eq!(starts(plain, "closed_hat").len(), 8);
    // A meter the groove was not written for: 7/8 grouped 2+2+3.
    let seven = beat("groove.rock_8", 2, |b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None,
        };
    });
    let notes = &play(&seven, 1)[0];
    assert_eq!(starts(notes, "kick"), [0, 960, 1920, 3360, 4320, 5280]);
    assert_eq!(starts(notes, "closed_hat").len(), 14);
    assert!(notes.iter().all(|n| n.end() <= 2 * 3360));
    // Three-four: kick on one, snare on two and three.
    let waltz = beat("groove.rock_8", 1, |b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 3,
            denominator: 4,
            grouping: None,
        };
    });
    let notes = &play(&waltz, 1)[0];
    assert_eq!(
        (starts(notes, "kick"), starts(notes, "snare")),
        (vec![0], vec![960, 1920])
    );
    // With a fill, the last group of the last bar is all snare.
    let filled = beat("groove.rock_8", 2, |b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None,
        };
        b.form[0].transition = Transition::FillLight;
    });
    let notes = &play(&filled, 1)[0];
    assert_eq!(
        starts(notes, "snare"),
        [3360 + 1920, 3360 + 2400, 3360 + 2880]
    );
    assert_eq!(starts(notes, "kick"), [0, 960, 1920, 3360, 4320]);
}

#[test]
fn t11_12_same_seed_same_drums_and_kick_onsets() {
    let brief = beat("groove.funk_16", 8, |b| {
        b.form[0].transition = Transition::FillHeavy
    });
    assert_eq!(play(&brief, 9), play(&brief, 9));
    assert_ne!(play(&brief, 9), play(&brief, 10));
    let rerolls = Rerolls::default();
    let plan = resolve(&brief, kb(), 9, &rerolls).unwrap();
    let drums_track = plan.track("drums").unwrap();
    let notes = &play(&brief, 9)[0];
    let kicks = kick_onsets(notes, drums_track);
    assert_eq!(kicks, starts(notes, "kick"));
    assert!(kicks.len() >= 8 && kicks.windows(2).all(|pair| pair[0] < pair[1]));
    // A pitched track has no kick.
    assert!(kick_onsets(notes, plan.track("lead").unwrap()).is_empty());
}

fn timeline_brief(id: &str, bars: u16) -> Brief {
    beat("groove.timeline", bars, |b| {
        b.time.meter = MeterSpec::Cycle { id: id.into() };
        b.time.length = Length::Cycles(bars);
    })
}

#[test]
fn t50_1_a_timeline_is_played_exactly_on_its_onsets_in_every_cycle() {
    let kb = kb();
    for (id, voice) in [
        ("timeline.son_3_2", "side_stick"),
        ("timeline.son_2_3", "side_stick"),
        ("timeline.rumba_3_2", "side_stick"),
        ("timeline.rumba_2_3", "side_stick"),
        ("timeline.bell_12_8", "cowbell"),
    ] {
        let cycle = kb.cycle(id).unwrap();
        let rerolls = Rerolls::default();
        let brief = timeline_brief(id, 3);
        let plan = resolve(&brief, kb, 1, &rerolls).expect("a timeline brief resolves");
        let pulse = plan.span / Tick::from(cycle.beats);
        let notes = &play(&brief, 5)[0];
        let mut wanted = Vec::new();
        for c in 0..3 {
            for (i, s) in cycle.strokes.iter().enumerate() {
                if s.name == "x" {
                    wanted.push(c * plan.span + i as Tick * pulse);
                }
            }
        }
        // Timing offsets are zero for a cycle, so the voice sits exactly on the strokes.
        assert_eq!(starts(notes, voice), wanted, "{id}");
        assert_eq!(
            starts(notes, "kick"),
            [0, plan.span, 2 * plan.span],
            "{id}: kick on the first pulse of each cycle"
        );
        assert_eq!(
            starts(notes, "closed_hat").len(),
            3 * usize::from(cycle.beats),
            "{id}"
        );
        assert!(notes.iter().all(|n| n.end() <= 3 * plan.span), "{id}");
    }
}

#[test]
fn t50_2_timelines_are_deterministic_and_other_cycles_keep_the_plain_beat() {
    let a = play(&timeline_brief("timeline.son_3_2", 2), 9);
    let b = play(&timeline_brief("timeline.son_3_2", 2), 9);
    assert_eq!(a, b);
    // A plain 7/8 meter under the timeline groove is not a timeline: it gets the plain beat.
    let tala = beat("groove.timeline", 2, |b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None,
        };
    });
    let notes = &play(&tala, 1)[0];
    assert!(starts(notes, "side_stick").is_empty() && starts(notes, "cowbell").is_empty());
}

#[test]
fn t51_1_clave_and_bell_are_loud_and_a_pulse_half_long_and_the_hat_is_soft() {
    for id in ["timeline.son_3_2", "timeline.bell_12_8"] {
        let cycle = kb().cycle(id).unwrap();
        let brief = timeline_brief(id, 2);
        let rerolls = Rerolls::default();
        let plan = resolve(&brief, kb(), 1, &rerolls).unwrap();
        let pulse = plan.span / Tick::from(cycle.beats);
        let notes = &play(&brief, 4)[0];
        let voice = piece(if id.contains("bell") {
            "cowbell"
        } else {
            "side_stick"
        });
        let hat = piece("closed_hat");
        let kick = piece("kick");
        for n in notes {
            assert_eq!(n.dur, pulse / 2, "{id}: every hit lasts half a pulse");
            let wanted = if n.pitch.cents == voice || n.pitch.cents == kick {
                109..=115
            } else if n.pitch.cents == hat {
                65..=71
            } else {
                panic!("{id}: unexpected piece {}", n.pitch.cents)
            };
            assert!(
                wanted.contains(&i64::from(n.vel)),
                "{id}: velocity {} for {}",
                n.vel,
                n.pitch.cents
            );
        }
    }
}

#[test]
fn t51_2_a_tala_under_the_timeline_groove_is_repaired_to_a_plain_meter_beat() {
    let brief = beat("groove.timeline", 2, |b| {
        b.time.meter = MeterSpec::Cycle {
            id: "tala.rupak".into(),
        };
        b.time.length = Length::Cycles(2);
    });
    // The test helper already repairs the brief: the groove cannot play a tala, so the meter is
    // no longer that cycle.
    let (fixed, _) = t2m_core::repair(brief, kb());
    assert_ne!(
        fixed.time.meter,
        MeterSpec::Cycle {
            id: "tala.rupak".into()
        },
        "a tala the groove does not play must be repaired away"
    );
    let notes = &play(&fixed, 1)[0];
    assert!(starts(notes, "side_stick").is_empty() && starts(notes, "cowbell").is_empty());
    assert!(!starts(notes, "kick").is_empty());
}
