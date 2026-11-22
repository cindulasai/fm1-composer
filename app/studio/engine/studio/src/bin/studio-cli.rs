//! The engine from a terminal (tests compare it with fm1c):
//!   studio-cli make BRIEF.json --seed N --name NAME -o OUT.f1p [--mid OUT.mid]
//!   studio-cli convert FILE.mid --name NAME [--poly N] -o OUT.f1p
//!   studio-cli prompt | studio-cli brief ANSWER.txt | studio-cli reply BODY.json
use std::process::ExitCode;

fn opt(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn read(path: &str) -> Result<Vec<u8>, String> { std::fs::read(path).map_err(|e| format!("{path}: {e}")) }
fn text(path: &str) -> Result<String, String> { String::from_utf8(read(path)?).map_err(|_| format!("{path}: not UTF-8 text")) }

fn run(args: &[String]) -> Result<(), String> {
    let need = |f: &str| opt(args, f).ok_or(format!("{f} is required"));
    let file = || args.get(1).cloned().ok_or("a file is required".to_string());
    match args.first().map(String::as_str) {
        Some("prompt") => println!("{}", studio::prompt()?),
        Some("brief") => println!("{}", studio::brief_from_reply(&text(&file()?)?)?),
        Some("reply") => println!("{}", studio::brief_from_reply(&studio::reply_text(&text(&file()?)?)?)?),
        Some(cmd @ ("make" | "convert")) => {
            let (out, name) = (need("-o")?, need("--name")?);
            let poly = match opt(args, "--poly") {
                None => studio::POLY,
                Some(p) => p.parse().ok().filter(|p| (1..=16).contains(p)).ok_or("--poly must be 1 to 16")?,
            };
            let fitted = if cmd == "make" {
                let seed = need("--seed")?.parse().map_err(|_| "--seed must be a whole number")?;
                let (midi, f) = studio::make(&text(&file()?)?, seed, &name, poly)?;
                if let Some(m) = opt(args, "--mid") { std::fs::write(&m, midi).map_err(|e| format!("{m}: {e}"))?; }
                f
            } else {
                studio::fit::fit(&read(&file()?)?, &name, poly)?
            };
            std::fs::write(&out, &fitted.bytes).map_err(|e| format!("{out}: {e}"))?;
            println!("{}", fitted.summary(&out));
            for line in &fitted.report { println!("  - {line}"); }
        }
        _ => return Err("usage: studio-cli make|convert|prompt|brief|reply …".into()),
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => { eprintln!("studio: {e}"); ExitCode::from(2) }
    }
}
