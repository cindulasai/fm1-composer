use t2m_core::brief::{Brief, Length, SectionHarmony};

use super::*;
use crate::plan::resolve;
use crate::seed::Rerolls;
use crate::testkit::{kb, progression, section, starter, with};

fn timeline(brief: &Brief) -> Vec<Vec<ChordSpan>> {
    let rerolls = Rerolls::default();
    let plan = resolve(brief, kb(), 1, &rerolls).expect("a valid brief resolves");
    chord_timeline(&plan)
}

/// One eight-bar section with the given harmony.
fn one_section(harmony: SectionHarmony) -> Brief {
    with(|b| {
        b.form = vec![section("a", 8, &["lead", "chords", "bass", "drums"])];
        b.form[0].harmony = harmony;
        b.time.length = Length::Bars(8);
    })
}

fn cents(span: &ChordSpan) -> Vec<i32> {
    span.voicing.iter().map(|pitch| pitch.cents).collect()
}

fn classes(span: &ChordSpan) -> Vec<i32> {
    let mut classes: Vec<i32> = cents(span).iter().map(|c| c.rem_euclid(1200)).collect();
    classes.sort_unstable();
    classes
}

#[test]
fn t10_8_a_progression_cycles_through_the_section() {
    let spans = &timeline(&one_section(progression(&["I", "V", "vi", "IV"], 1)))[0];
    assert_eq!(spans.len(), 8);
    for (index, span) in spans.iter().enumerate() {
        assert_eq!(
            (span.start, span.end),
            (index as i64 * 3840, (index as i64 + 1) * 3840)
        );
    }
    let labels: Vec<&str> = spans.iter().map(|span| span.chord.label.as_str()).collect();
    assert_eq!(labels, ["I", "V", "vi", "IV", "I", "V", "vi", "IV"]);
    assert_eq!(classes(&spans[0]), [0, 400, 700]);
    assert_eq!(classes(&spans[1]), [200, 700, 1100]);
    assert_eq!(classes(&spans[2]), [0, 400, 900]);
    assert_eq!(classes(&spans[3]), [0, 500, 900]);

    // A named progression gives the same chords.
    let named = &timeline(&one_section(SectionHarmony::Named {
        id: "prog.axis".into(),
    }))[0];
    assert_eq!(named, spans);

    // Two bars per chord; the last chord is cut at the section's end, never past it.
    let spans = &timeline(&one_section(progression(&["I", "V", "vi"], 3)))[0];
    let bounds: Vec<(i64, i64)> = spans
        .iter()
        .map(|span| (span.start / 3840, span.end / 3840))
        .collect();
    assert_eq!(bounds, [(0, 3), (3, 6), (6, 8)]);

    // Sections without chords have none; each section starts at its own start.
    let mixed = with(|b| {
        b.form[1].harmony = SectionHarmony::None;
        b.form[2].harmony = SectionHarmony::Drone {
            degrees: vec![1, 5],
        };
    });
    let all = timeline(&mixed);
    assert_eq!(all.len(), 4);
    assert_eq!(all[0].len(), 4);
    assert!(all[1].is_empty() && all[2].is_empty());
    assert_eq!(all[3][0].start, 20 * 3840);
    assert_eq!(all[3].last().unwrap().end, 24 * 3840);

    let spans = &timeline(&starter())[1];
    assert_eq!(chord_at(spans, 15_360).unwrap().chord.label, "I");
    assert_eq!(chord_at(spans, 15_360 + 3839).unwrap().chord.label, "I");
    assert_eq!(chord_at(spans, 15_360 + 3840).unwrap().chord.label, "V");
    assert!(chord_at(spans, 15_359).is_none());
    assert!(chord_at(spans, 46_080).is_none());
}

#[test]
fn t10_9_voicings_stay_in_range_and_move_less_than_block_chords() {
    let spans = &timeline(&one_section(progression(&["I", "V", "vi", "IV"], 1)))[0];
    // MIDI 48 to 76 with the tonic C at MIDI 12: 3600 to 6400 cents.
    for span in spans {
        assert!(
            cents(span).iter().all(|c| (3600..=6400).contains(c)),
            "{:?}",
            cents(span)
        );
        assert!(
            cents(span).windows(2).all(|pair| pair[0] < pair[1]),
            "ascending"
        );
        assert_eq!(span.voicing.len(), 3);
    }
    // The first chord sits around the middle of the range (5000).
    assert_eq!(cents(&spans[0]), [4800, 5200, 5500]);

    let movement =
        |a: &[i32], b: &[i32]| -> i32 { a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum() };
    let led: i32 = spans[..4]
        .windows(2)
        .map(|pair| movement(&cents(&pair[0]), &cents(&pair[1])))
        .sum();
    let blocks = [
        [4800, 5200, 5500],
        [5500, 5900, 6200],
        [5700, 6000, 6400],
        [5300, 5700, 6000],
    ];
    let block: i32 = blocks
        .windows(2)
        .map(|pair| movement(&pair[0], &pair[1]))
        .sum();
    assert_eq!(block, 3700);
    assert!(
        led <= block / 2,
        "voice-led movement {led} should be far below {block}"
    );

    // I to vi keeps two notes where they are.
    let (one, six) = (cents(&spans[0]), cents(&spans[2]));
    let _ = six;
    let two_chords = &timeline(&one_section(progression(&["I", "vi"], 1)))[0];
    let (one_again, six) = (cents(&two_chords[0]), cents(&two_chords[1]));
    assert_eq!(one, one_again);
    let held = one.iter().filter(|pitch| six.contains(pitch)).count();
    assert_eq!(held, 2);
    assert_eq!(six, [4800, 5200, 5700]);
}

#[test]
fn t10_9_rich_chords_keep_the_tones_that_define_them() {
    let spans = &timeline(&one_section(progression(
        &["V7", "V9", "V13", "ii11", "Iadd9", "I6"],
        1,
    )))[0];
    assert_eq!(classes(&spans[0]), [200, 500, 700, 1100]);
    // A ninth chord drops its fifth: root, third, seventh, ninth.
    assert_eq!(classes(&spans[1]), [500, 700, 900, 1100]);
    // A thirteenth drops fifth and root: third, seventh, ninth, thirteenth.
    assert_eq!(classes(&spans[2]), [400, 500, 900, 1100]);
    // An eleventh likewise: third (F), seventh (C), ninth (E), eleventh (G).
    assert_eq!(classes(&spans[3]), [0, 400, 500, 700]);
    assert_eq!(classes(&spans[4]), [0, 200, 400, 700]);
    assert_eq!(classes(&spans[5]), [0, 400, 700, 900]);
    for span in spans {
        assert_eq!(span.voicing.len(), 4);
        assert!(cents(span).iter().all(|c| (3600..=6400).contains(c)));
    }
}

#[test]
fn t10_10_each_section_is_voiced_by_itself() {
    let base = timeline(&starter());
    // Changing the first section's chords leaves every other section's voicings alone.
    let edited = timeline(&with(|b| b.form[0].harmony = progression(&["vi", "IV"], 2)));
    assert_ne!(edited[0], base[0]);
    assert_eq!(edited[1..], base[1..]);
    // Every section's first chord is voiced around the middle, whatever came before.
    assert_eq!(cents(&base[1][0]), [4800, 5200, 5500]);
    assert_eq!(cents(&base[3][0]), [4800, 5200, 5500]);
}

#[test]
fn t10_11_odd_chords_and_ranges_never_panic() {
    // A tonic high in the octave shifts the range in cents; voicings still fit it.
    for tonic in ["B", "F#", "C"] {
        let brief = with(|b| b.tonality.tonic = t2m_core::brief::TonicSpec::Name(tonic.into()));
        for spans in timeline(&brief) {
            for span in spans {
                let rerolls = Rerolls::default();
                let plan = resolve(&brief, kb(), 1, &rerolls).unwrap();
                let (low, high) = (plan.cents_of_midi(48), plan.cents_of_midi(76));
                assert!(
                    cents(&span).iter().all(|c| (low..=high).contains(c)),
                    "{tonic}"
                );
            }
        }
    }
    // Sixty-four chords, one bar each, in an eight-bar section: only eight are used.
    let many: Vec<&str> = std::iter::repeat_n(["I", "bVII", "V7/ii", "viiø7"], 16)
        .flatten()
        .collect();
    let spans = &timeline(&one_section(progression(&many, 1)))[0];
    assert_eq!(spans.len(), 8);
    // A chord held longer than the section is cut to it.
    let spans = &timeline(&one_section(progression(&["I"], 200)))[0];
    assert_eq!((spans.len(), spans[0].end), (1, 8 * 3840));
}
