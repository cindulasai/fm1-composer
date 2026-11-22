//! Reads a brief (a file path, or the built-in starter with mistakes added),
//! repairs it, and prints what changed.
//!
//! Run: `cargo run -p t2m-core --example repair_brief [brief.json]`

use std::process::ExitCode;

use t2m_core::{Brief, Knowledge, repair, validate};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<(), String> {
    let kb = Knowledge::load().map_err(|error| error.to_string())?;
    let brief = match std::env::args().nth(1) {
        Some(path) => {
            let text =
                std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
            Brief::from_json(&text).map_err(|error| format!("{path}: {error}"))?
        }
        None => {
            let mut brief = kb
                .gallery()
                .first()
                .map(|entry| entry.brief.clone())
                .ok_or("the gallery is empty")?;
            brief.identity.style = "popp".to_string();
            brief.time.tempo_bpm = 400;
            if let Some(track) = brief.tracks.first_mut() {
                track.instrument = "grand piano".to_string();
            }
            brief
        }
    };

    let (repaired, issues) = repair(brief, &kb);
    if issues.is_empty() {
        println!("The brief is valid as written.");
    }
    for issue in &issues {
        match &issue.now {
            Some(now) => println!("{}: was {:?}, now {:?}", issue.path, issue.was, now),
            None => println!("{}: removed {:?}", issue.path, issue.was),
        }
    }
    validate(&repaired, &kb)
        .map_err(|left| format!("{} issues remain after repair", left.len()))?;
    println!(
        "Valid: {} at {} bpm, {} tracks, {} sections.",
        repaired.identity.style,
        repaired.time.tempo_bpm,
        repaired.tracks.len(),
        repaired.form.len()
    );
    Ok(())
}
