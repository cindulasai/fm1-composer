use super::*;

fn first(rng: &mut Rng, n: usize) -> Vec<u64> {
    (0..n).map(|_| rng.next_u64()).collect()
}

#[test]
fn t10_1_equal_inputs_give_equal_streams_and_any_change_gives_another() {
    let base = first(&mut sub_rng(1, "bass", "a", 0), 8);
    assert_eq!(first(&mut sub_rng(1, "bass", "a", 0), 8), base);
    for other in [
        sub_rng(2, "bass", "a", 0),
        sub_rng(1, "lead", "a", 0),
        sub_rng(1, "bass", "b", 0),
        sub_rng(1, "bass", "a", 1),
        // Moving a character from one id to the other is a different part.
        sub_rng(1, "bas", "sa", 0),
        sub_rng(1, "bassa", "", 0),
    ] {
        let mut other = other;
        assert_ne!(first(&mut other, 8), base);
    }
}

#[test]
fn t10_1_the_derivation_is_pinned() {
    // If this changes, every saved piece changes. Do not update it lightly.
    let got = first(&mut sub_rng(1, "bass", "a", 0), 2);
    const PINNED: [u64; 2] = [14_434_412_297_520_290_214, 10_440_484_612_069_767_818];
    assert_eq!(got, PINNED);
}

#[test]
fn t10_2_whole_number_draws_stay_in_range_and_cover_it() {
    let mut rng = sub_rng(7, "t", "s", 0);
    assert_eq!(rng.below(0), 0);
    assert_eq!(rng.below(1), 0);
    let mut seen = [0u32; 5];
    for _ in 0..5_000 {
        seen[rng.below(5) as usize] += 1;
    }
    // Each of five values should come up about a thousand times.
    assert!(
        seen.iter().all(|count| (850..1150).contains(count)),
        "{seen:?}"
    );

    let mut low_seen = false;
    let mut high_seen = false;
    for _ in 0..2_000 {
        let value = rng.between(-3, 3);
        assert!((-3..=3).contains(&value));
        low_seen |= value == -3;
        high_seen |= value == 3;
    }
    assert!(low_seen && high_seen);
    assert_eq!(rng.between(5, 5), 5);
    assert_eq!(rng.between(9, 2), 9);
    assert_eq!(rng.between(i64::MIN, i64::MIN), i64::MIN);
    let wide = rng.between(0, i64::MAX);
    assert!(wide >= 0);

    assert!((0..200).all(|_| !rng.chance(0)));
    assert!((0..200).all(|_| rng.chance(1000)));
    let hits = (0..10_000).filter(|_| rng.chance(250)).count();
    assert!((2_300..2_700).contains(&hits), "{hits}");

    assert_eq!(rng.pick::<u8>(&[]), None);
    assert_eq!(rng.pick(&[42]), Some(&42));
    let items = [1, 2, 3];
    let mut picked = [false; 3];
    for _ in 0..200 {
        picked[*rng.pick(&items).unwrap() as usize - 1] = true;
    }
    assert_eq!(picked, [true; 3]);
}

#[test]
fn t10_2_rerolls_count_per_track_and_section() {
    let mut rerolls = Rerolls::default();
    assert_eq!(rerolls.count("bass", "a"), 0);
    rerolls.bump("bass", "a");
    rerolls.bump("bass", "a");
    rerolls.bump("lead", "a");
    assert_eq!(rerolls.count("bass", "a"), 2);
    assert_eq!(rerolls.count("lead", "a"), 1);
    assert_eq!(rerolls.count("bass", "b"), 0);
    rerolls.0.insert(("x".into(), "y".into()), u32::MAX);
    rerolls.bump("x", "y");
    assert_eq!(rerolls.count("x", "y"), u32::MAX);
}
