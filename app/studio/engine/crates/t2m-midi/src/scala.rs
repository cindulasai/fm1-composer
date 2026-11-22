//! Scala tuning files: `.scl` (the scale), `.kbm` (the keyboard map) and Ableton's `.ascl`.

use t2m_core::Tuning;

/// The scale: description, number of degrees, then each degree in cents, the last being the period.
pub fn write_scl(tuning: &Tuning, name: &str) -> String {
    let mut text = format!(
        "! {name}.scl\n!\n{}\n {}\n!\n",
        tuning.id,
        tuning.cents.len()
    );
    for cents in tuning.cents.iter().skip(1) {
        text.push_str(&format!(" {:.1}\n", f64::from(*cents)));
    }
    text.push_str(&format!(" {:.1}\n", f64::from(tuning.period)));
    text
}

/// A linear keyboard map: the scale's first degree sits on `tonic_midi`, tuned to `tonic_hz`.
pub fn write_kbm(tonic_midi: u8, tonic_hz: f64, degrees: usize) -> String {
    format!("! {tonic_midi}.kbm\n0\n0\n127\n{tonic_midi}\n{tonic_midi}\n{tonic_hz:.6}\n{degrees}\n")
}

/// Ableton's extension of `.scl`: the same scale plus a reference pitch (octave, degree index, Hz),
/// written as the comment line `! @ABL REFERENCE_PITCH` that the format defines.
pub fn write_ascl(tuning: &Tuning, name: &str, tonic_midi: u8, tonic_hz: f64) -> String {
    let scl = write_scl(tuning, name).replacen(".scl\n", ".ascl\n", 1);
    let octave = i32::from(tonic_midi) / 12 - 1;
    format!("{scl}! @ABL REFERENCE_PITCH {octave} 0 {tonic_hz:.6}\n")
}

#[cfg(test)]
#[path = "scala_tests.rs"]
mod tests;
