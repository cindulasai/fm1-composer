use t2m_core::brief::{Length, MeterSpec, Register, Role, SectionHarmony, TonicSpec, Transition};

use super::*;
use crate::testkit::{kb, section, starter, track, with};

fn plan_of(brief: &Brief) -> Plan<'_> {
    static NONE: std::sync::OnceLock<Rerolls> = std::sync::OnceLock::new();
    resolve(brief, kb(), 1, NONE.get_or_init(Rerolls::default)).expect("a valid brief resolves")
}

#[test]
fn t10_3_sections_are_laid_end_to_end_in_ticks() {
    let brief = starter();
    let plan = plan_of(&brief);
    assert_eq!(plan.span, 3840);
    assert_eq!(plan.tempo, 116);
    let spans: Vec<(i64, i64, u16)> = plan
        .sections
        .iter()
        .map(|s| (s.start, s.end, s.bars))
        .collect();
    assert_eq!(
        spans,
        [
            (0, 15_360, 4),
            (15_360, 46_080, 8),
            (46_080, 76_800, 8),
            (76_800, 92_160, 4)
        ]
    );
    assert_eq!(plan.sections[0].id, "intro");
    assert_eq!(plan.sections[1].energy, 0.5);
    assert_eq!(plan.sections[2].density, 0.8);
    let active: Vec<&str> = plan.sections[0].active.iter().map(String::as_str).collect();
    assert_eq!(active, ["chords", "drums"]);
    // Each section knows how it ends and how the one before it ended.
    let hand_overs: Vec<(Transition, Transition)> = plan
        .sections
        .iter()
        .map(|s| (s.entered_by, s.transition))
        .collect();
    assert_eq!(
        hand_overs,
        [
            (Transition::None, Transition::FillLight),
            (Transition::FillLight, Transition::FillLight),
            (Transition::FillLight, Transition::None),
            (Transition::None, Transition::None),
        ]
    );
    // A named progression is looked up; a written one is kept.
    let named = SectionHarmony::Progression {
        roman: vec!["I".into(), "V".into(), "vi".into(), "IV".into()],
        bars_per_chord: 1,
    };
    assert_eq!(plan.sections[0].harmony, named);
    let written = SectionHarmony::Progression {
        roman: vec!["IV".into(), "I".into(), "V".into(), "vi".into()],
        bars_per_chord: 1,
    };
    assert_eq!(plan.sections[2].harmony, written);
    assert_eq!(plan.style.id, "pop.generic");
    assert_eq!(plan.groove.id, "groove.straight_8");
    assert_eq!(plan.system.id, "mode.major");
    assert_eq!(plan.tuning.id, "12tet");
    assert!(plan.motif.is_none());
}

#[test]
fn t10_3_length_is_shared_among_sections_in_proportion() {
    // Sixty seconds at 120 bpm in 4/4 is thirty bars.
    let brief = with(|b| {
        b.time.tempo_bpm = 120;
        b.time.length = Length::Seconds(60);
    });
    let bars: Vec<u16> = plan_of(&brief).sections.iter().map(|s| s.bars).collect();
    assert_eq!(bars, [5, 10, 10, 5]);
    assert_eq!(bars.iter().sum::<u16>(), 30);
    // 61 seconds is 30.5 bars, which rounds up; 59 seconds is 29.5, which also rounds up.
    for (seconds, total) in [(61, 31), (59, 30), (58, 29), (1, 4)] {
        let brief = with(|b| {
            b.time.tempo_bpm = 120;
            b.time.length = Length::Seconds(seconds);
        });
        let sum: u16 = plan_of(&brief).sections.iter().map(|s| s.bars).sum();
        assert_eq!(sum, total, "{seconds} seconds");
    }
    // A stated bar count is shared the same way; cycles count as bars.
    for length in [Length::Bars(48), Length::Cycles(48)] {
        let brief = with(|b| b.time.length = length);
        let plan = plan_of(&brief);
        let bars: Vec<u16> = plan.sections.iter().map(|s| s.bars).collect();
        assert_eq!(bars, [8, 16, 16, 8]);
        assert_eq!(plan.sections[3].end, 48 * 3840);
    }
    // In 7/8 a bar is 3360 ticks.
    let brief = with(|b| {
        b.time.meter = MeterSpec::Meter {
            numerator: 7,
            denominator: 8,
            grouping: None,
        };
    });
    let plan = plan_of(&brief);
    assert_eq!(plan.span, 3360);
    assert_eq!(plan.sections[1].start, 4 * 3360);
}

#[test]
fn t10_4_allocation_keeps_weights_when_it_can_and_never_leaves_a_section_empty() {
    assert_eq!(allocate(&[4, 8, 8, 4], 24), [4, 8, 8, 4]);
    assert_eq!(allocate(&[1, 9], 10), [1, 9]);
    assert_eq!(allocate(&[4, 8, 8, 4], 30), [5, 10, 10, 5]);
    assert_eq!(allocate(&[4, 8, 8, 4], 12), [2, 4, 4, 2]);
    // Left-over bars go to the largest remainders, the earliest on a tie.
    assert_eq!(allocate(&[1, 1, 1], 4), [2, 1, 1]);
    assert_eq!(allocate(&[1, 1, 1], 5), [2, 2, 1]);
    assert_eq!(allocate(&[3, 1], 6), [5, 1]);
    assert_eq!(allocate(&[2, 3], 6), [2, 4]);
    // A section that would get nothing takes a bar from the longest.
    assert_eq!(allocate(&[1, 100], 3), [1, 2]);
    assert_eq!(allocate(&[100, 1, 1], 3), [1, 1, 1]);
    assert_eq!(allocate(&[100, 1, 100], 4), [1, 1, 2]);
    assert_eq!(allocate(&[50, 1, 1, 50], 5), [1, 1, 1, 2]);
    // Fewer bars than sections cannot be helped: nothing is taken from a one-bar section.
    assert_eq!(allocate(&[1, 1, 1], 2), [1, 1, 0]);
    assert_eq!(allocate(&[5], 512), [512]);
    assert_eq!(allocate(&[], 10), Vec::<u16>::new());
    assert_eq!(allocate(&[0, 0], 10), [0, 0]);
    assert_eq!(allocate(&[u16::MAX, u16::MAX], 512), [256, 256]);
    for total in 3..=512u32 {
        let bars = allocate(&[7, 1, 13], total);
        assert_eq!(bars.iter().map(|b| u32::from(*b)).sum::<u32>(), total);
        assert!(bars.iter().all(|b| *b >= 1), "{total}: {bars:?}");
    }
}

#[test]
fn t10_5_registers_follow_role_setting_and_instrument() {
    let brief = with(|b| {
        b.tracks = vec![
            track("lead", Role::Lead, "keys.piano"),
            track("high", Role::Lead, "keys.piano"),
            track("bass", Role::Bass, "bass.electric"),
            track("deep", Role::Bass, "bass.electric"),
            track("odd", Role::Lead, "bass.electric"),
            track("drums", Role::Drums, "kit.standard"),
        ];
        b.tracks[1].register = Register::High;
        b.tracks[3].register = Register::Low;
        b.tracks[0].busy = 0.25;
        b.tracks[0].pattern = "melody.lyrical".into();
        b.form = vec![section("a", 4, &["lead"])];
    });
    let plan = plan_of(&brief);
    // C major: the tonic at octave 0 is MIDI 12, so MIDI 60 is 4800 cents.
    assert_eq!(plan.tonic_midi, 12);
    assert_eq!(plan.cents_of_midi(60), 4800);
    let range = |id: &str| {
        let track = plan.track(id).unwrap();
        (track.low / 100 + 12, track.high / 100 + 12)
    };
    assert_eq!(range("lead"), (60, 88));
    assert_eq!(range("high"), (72, 100));
    // The bass role wants 28 to 52; the instrument reaches 28 to 63.
    assert_eq!(range("bass"), (28, 52));
    // An octave lower would be 16 to 40; the instrument stops at 28.
    assert_eq!(range("deep"), (28, 40));
    // A lead on a bass guitar: 60 to 88 meets 28 to 63 at 60 to 63.
    assert_eq!(range("odd"), (60, 63));
    let lead = plan.track("lead").unwrap();
    assert_eq!(
        (lead.busy, lead.pattern, lead.role),
        (0.25, "melody.lyrical", Role::Lead)
    );
    assert_eq!(lead.instrument.id, "keys.piano");
    assert_eq!(lead.center(), 4800 + 1400);
    assert!(plan.track("nope").is_none());

    // Another tonic moves the reference: D at octave 0 is MIDI 14.
    let in_d = with(|b| b.tonality.tonic = TonicSpec::Name("D".into()));
    let plan = plan_of(&in_d);
    assert_eq!(plan.tonic_midi, 14);
    assert_eq!(plan.cents_of_midi(60), 4600);
    let in_hz = with(|b| b.tonality.tonic = TonicSpec::Hz { hz: 440.0 });
    assert_eq!(plan_of(&in_hz).tonic_midi, 21);
}

#[test]
fn t10_5_a_role_range_that_misses_the_instrument_uses_the_instrument() {
    let brief = with(|b| {
        b.tracks = vec![
            track("arp", Role::Arp, "bass.electric"),
            track("drums", Role::Drums, "kit.standard"),
        ];
        b.tracks[0].register = Register::High;
        b.form = vec![section("a", 4, &["arp"])];
    });
    let plan = plan_of(&brief);
    let arp = plan.track("arp").unwrap();
    // 72 to 108 lies above the bass guitar's 28 to 63.
    assert_eq!((arp.low / 100 + 12, arp.high / 100 + 12), (28, 63));
}

#[test]
fn t10_6_place_finds_the_nearest_octave_in_range_or_none() {
    let brief = starter();
    let plan = plan_of(&brief);
    let lead = plan.track("lead").unwrap();
    // Lead with a mid-high register: MIDI 65 to 93, in cents above MIDI 12.
    assert_eq!((lead.low, lead.high), (5300, 8100));
    // E (400) nearest to 6000 is 6400; nearest to 7500 is 7600; nearest to 100 is the lowest, 6400.
    assert_eq!(lead.place(400, 6000), Some(6400));
    assert_eq!(lead.place(400, 7500), Some(7600));
    assert_eq!(lead.place(400, 100), Some(6400));
    assert_eq!(lead.place(400, 99_999), Some(7600));
    // Any octave of the pitch is accepted as input, including negative ones.
    assert_eq!(lead.place(-800, 6000), Some(6400));
    assert_eq!(lead.place(6400 + 2400, 6000), Some(6400));
    // The edges of the range are playable.
    assert_eq!(lead.place(5300, 0), Some(5300));
    assert_eq!(lead.place(8100, 9000), Some(8100));
    // Midway between two octaves, the lower one wins.
    assert_eq!(lead.place(400, 7000), Some(6400));
    // A microtonal pitch keeps its cents.
    assert_eq!(lead.place(350, 6000), Some(6350));

    // A part whose range is narrower than an octave cannot play every pitch class.
    let narrow = with(|b| {
        b.tracks[0] = track("lead", Role::Lead, "bass.electric");
    });
    let plan = plan_of(&narrow);
    let lead = plan.track("lead").unwrap();
    assert_eq!((lead.low, lead.high), (4800, 5100));
    assert_eq!(lead.place(0, 0), Some(4800));
    assert_eq!(lead.place(300, 0), Some(5100));
    assert_eq!(lead.place(400, 0), None);
    assert_eq!(lead.place(1100, 0), None);
}

#[test]
fn t10_7_an_invalid_brief_does_not_resolve() {
    let rerolls = Rerolls::default();
    let broken: [fn(&mut Brief); 7] = [
        |b| b.identity.style = "nope".into(),
        |b| b.time.feel = "nope".into(),
        |b| b.tonality.system = "nope".into(),
        |b| b.tonality.tuning = "nope".into(),
        |b| b.tonality.tonic = TonicSpec::Name("H".into()),
        |b| b.tracks[0].instrument = "nope".into(),
        |b| b.time.meter = MeterSpec::Cycle { id: "nope".into() },
    ];
    for (index, edit) in broken.into_iter().enumerate() {
        let mut brief = starter();
        edit(&mut brief);
        assert!(resolve(&brief, kb(), 1, &rerolls).is_none(), "edit {index}");
    }
    let mut brief = starter();
    brief.time.meter = MeterSpec::Meter {
        numerator: 0,
        denominator: 4,
        grouping: None,
    };
    assert!(resolve(&brief, kb(), 1, &rerolls).is_none());
    let mut brief = starter();
    brief.form[0].harmony = SectionHarmony::Named { id: "nope".into() };
    assert!(resolve(&brief, kb(), 1, &rerolls).is_none());
}
