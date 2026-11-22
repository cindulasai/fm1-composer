use super::*;

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
fn t25_1_scl_lists_the_degrees_after_the_tonic_and_ends_on_the_period() {
    let text = write_scl(&seven(), "piece");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "! piece.scl");
    // The first line that is not a comment is the description, then the count.
    let body: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|l| !l.starts_with('!'))
        .collect();
    assert_eq!(body[0], "seven");
    assert_eq!(body[1].trim(), "7");
    assert_eq!(body.len(), 2 + 7);
    assert_eq!(body[2].trim(), "171.0");
    assert_eq!(body[7].trim(), "1029.0");
    assert_eq!(body[8].trim(), "1200.0");
}

#[test]
fn t25_2_kbm_maps_the_tonic_to_its_frequency_over_the_whole_keyboard() {
    let text = write_kbm(60, 261.625565, 7);
    let body: Vec<&str> = text.lines().filter(|l| !l.starts_with('!')).collect();
    assert_eq!(body, ["0", "0", "127", "60", "60", "261.625565", "7"]);
}

#[test]
fn t25_3_ascl_is_the_scale_plus_a_reference_pitch_comment() {
    let text = write_ascl(&seven(), "piece", 60, 261.625565);
    assert!(text.starts_with("! piece.ascl\n"));
    assert!(text.contains("\nseven\n 7\n"));
    assert!(text.ends_with("! @ABL REFERENCE_PITCH 4 0 261.625565\n"));
    // The scale lines are the .scl ones.
    let scl = write_scl(&seven(), "piece");
    let tail = |s: &str| {
        s.lines()
            .skip(1)
            .filter(|l| !l.starts_with('!'))
            .collect::<Vec<_>>()
            .join("|")
    };
    assert_eq!(tail(&text), tail(&scl));
}
