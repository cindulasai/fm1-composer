//! text2midi's window commands, served in the browser: what text2midi_v2's app/src-tauri/src/main.rs does with
//! `t2m-session` and `t2m-audio`, without Tauri, files or a sound device. One open piece and one audio engine.
//! Each command takes and returns JSON shaped as the Tauri command's arguments and result, so text2midi's own
//! window (its Svelte code, unchanged) talks to this through a small `invoke` stand-in.
use serde_json::{Value, json};
use t2m_audio::Engine;
use t2m_core::Brief;
use t2m_midi::TuningProfile;
use t2m_session::Session;

pub struct Desk {
    session: Session,
    engine: Engine,
    loaded: bool,
}

fn s(v: &Value, k: &str) -> Result<String, String> {
    v.get(k).and_then(Value::as_str).map(str::to_string).ok_or_else(|| format!("missing {k}"))
}
fn opt(v: &Value, k: &str) -> Option<String> { v.get(k).and_then(Value::as_str).map(str::to_string) }
fn n(v: &Value, k: &str) -> Result<u64, String> {
    v.get(k).and_then(Value::as_u64).ok_or_else(|| format!("missing {k}"))
}
fn b(v: &Value, k: &str) -> Result<bool, String> {
    v.get(k).and_then(Value::as_bool).ok_or_else(|| format!("missing {k}"))
}
fn bools(v: &Value, k: &str) -> Vec<bool> {
    v.get(k).and_then(Value::as_array).map(|a| a.iter().map(|x| x.as_bool().unwrap_or(false)).collect()).unwrap_or_default()
}
fn to<T: serde::Serialize>(x: T) -> Result<Value, String> { serde_json::to_value(x).map_err(|e| e.to_string()) }
fn brief_of(json: &str) -> Result<Brief, String> { Brief::from_json(json).map_err(|e| format!("not a brief: {e}")) }

/// text2midi's offline note, word for word (main.rs cloud_brief).
fn offline_note(why: Option<&str>) -> String {
    match why {
        None => "No AI model is set up, so the plain keyword reader was used (open Settings to choose one).".to_string(),
        Some(e) => format!("The language model did not help ({e}), so the plain keyword reader was used."),
    }
}

impl Desk {
    pub fn new(sample_rate: u32) -> Result<Desk, String> {
        Ok(Desk { session: Session::new().map_err(|e| e.to_string())?, engine: Engine::new(sample_rate), loaded: false })
    }

    fn reload(&mut self) {
        if let Ok(piece) = self.session.piece() {
            self.engine.load(piece);
            self.loaded = true;
        }
    }

    /// One command: `cmd` with `args` (JSON) → its result (JSON), or the error text the window shows.
    /// AI commands take what the browser got from the model: `brief` (checked brief JSON) or `ai_error`.
    pub fn call(&mut self, cmd: &str, args: &str) -> Result<String, String> {
        let a: Value = if args.trim().is_empty() { json!({}) } else { serde_json::from_str(args).map_err(|e| e.to_string())? };
        let e = |x: t2m_session::SessionError| x.to_string();
        let out = match cmd {
            "gallery" => to(self.session.gallery())?,
            "open_example" => to(self.session.open_example(&s(&a, "id")?, n(&a, "seed")?).map_err(e)?)?,
            "open_prompt" => {
                let (text, seed) = (s(&a, "text")?, n(&a, "seed")?);
                match opt(&a, "brief") {
                    Some(brief) => json!({ "view": to(self.session.open_brief(brief_of(&brief)?, seed))?, "source": "cloud", "note": null }),
                    None => {
                        let view = self.session.open_prompt(&text, seed).map_err(e)?;
                        let note = a.get("asked").and_then(Value::as_bool).unwrap_or(false)
                            .then(|| offline_note(opt(&a, "ai_error").as_deref()));
                        json!({ "view": to(view)?, "source": "offline", "note": note })
                    }
                }
            }
            "make" => {
                let (text, seed) = (s(&a, "text")?, n(&a, "seed")?);
                let mut fresh = Session::new().map_err(|e| e.to_string())?;
                let (view, source) = match opt(&a, "brief") {
                    Some(brief) => (fresh.open_brief(brief_of(&brief)?, seed), "cloud"),
                    None => (fresh.open_prompt(&text, seed).map_err(e)?, "offline"),
                };
                let note = (source == "offline").then(|| offline_note(opt(&a, "ai_error").as_deref()));
                json!({ "json": fresh.save().map_err(e)?, "title": view.title, "gallery_id": view.gallery_id,
                        "seed": view.seed, "source": source, "note": note })
            }
            "view" => to(self.session.view().map_err(e)?)?,
            "takes" => to(self.session.takes(&s(&a, "track")?, opt(&a, "section").as_deref(), n(&a, "count")? as u8).map_err(e)?)?,
            "keep_take" => to(self.session.keep_take(&s(&a, "track")?, opt(&a, "section").as_deref(), n(&a, "take")? as u8).map_err(e)?)?,
            "reroll" => to(self.session.reroll(&s(&a, "track")?, opt(&a, "section").as_deref()).map_err(e)?)?,
            "reseed" => to(self.session.reseed(n(&a, "seed")?).map_err(e)?)?,
            "lock" => to(self.session.lock(&s(&a, "track")?, b(&a, "on")?).map_err(e)?)?,
            "undo" => to(self.session.undo().map_err(e)?)?,
            "redo" => to(self.session.redo().map_err(e)?)?,
            "save" => {
                let v = self.session.view().map_err(e)?;
                json!({ "json": self.session.save().map_err(e)?, "title": v.title, "gallery_id": v.gallery_id, "seed": v.seed })
            }
            "open_project" => to(self.session.open_project(&s(&a, "json")?).map_err(e)?)?,
            "play" => {
                let piece = self.session.piece().map_err(e)?;
                self.engine.load(piece);
                self.loaded = true;
                if let Some(t) = a.get("from").and_then(Value::as_i64) { self.engine.seek(t); }
                self.engine.play();
                self.transport()
            }
            "stop" => { self.engine.stop(); self.transport() }
            "transport" => self.transport(),
            "mix" => {
                let piece = self.session.piece().map_err(e)?;
                let (muted, soloed) = (bools(&a, "muted"), bools(&a, "soloed"));
                for t in 0..piece.tracks {
                    self.engine.set_mute(t, muted.get(t).copied().unwrap_or(false));
                    self.engine.set_solo(t, soloed.get(t).copied().unwrap_or(false));
                }
                self.engine.set_loop(if b(&a, "looping")? { Some((0, piece.end)) } else { None });
                Value::Null
            }
            other => return Err(format!("unknown command {other:?}")),
        };
        if !matches!(cmd, "play" | "stop" | "transport" | "mix" | "gallery" | "make" | "save" | "view" | "takes") {
            self.reload();
        }
        Ok(out.to_string())
    }

    fn transport(&self) -> Value { json!({ "playing": self.engine.is_playing(), "position": self.engine.position() }) }

    /// Interleaved stereo frames of the preview; silence while stopped.
    pub fn render(&mut self, out: &mut [f32]) { if self.loaded { self.engine.render(out) } else { out.fill(0.0) } }

    /// The open piece's MIDI, as text2midi exports it (`profile`: approx, mpe, tuning): (file name, bytes) each.
    pub fn export(&self, profile: &str) -> Result<Vec<(String, Vec<u8>)>, String> {
        let profile = match profile {
            "mpe" => TuningProfile::Mpe,
            "tuning" => TuningProfile::TuningFile,
            "approx" => TuningProfile::Approx12,
            other => return Err(format!("unknown profile {other:?}")),
        };
        Ok(self.session.export(profile, &[]).map_err(|e| e.to_string())?.files)
    }

    /// The open piece fitted for the FM-1 (its approx MIDI, as fm1c convert reads it).
    pub fn fitted(&self, poly: usize) -> Result<(Vec<u8>, crate::fit::Fitted), String> {
        let v = self.session.view().map_err(|e| e.to_string())?;
        let midi = self.export("approx")?.into_iter().find(|f| f.0.ends_with(".mid")).map(|f| f.1)
            .ok_or("text2midi wrote no MIDI")?;
        let fitted = crate::fit::fit(&midi, &v.title, poly)?;
        Ok((midi, fitted))
    }
}

