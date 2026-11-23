//! COMPOSER Studio's engine. A brief (from the AI, or written) becomes a piece the FM-1 plays: text2midi composes it
//! (its crates, copied unchanged), then it is fitted and encoded exactly as `fm1c compose` does. Also: the AI's system
//! prompt, and its reply read back into a checked brief. Compiled to WASM for the page; `studio-cli` for tests.
pub mod desk;
pub mod f1p;
pub mod fit;
pub mod smf;
pub mod sounds;

use t2m_core::{Brief, Knowledge, repair, validate};
use t2m_midi::{ExportOptions, SectionSelection, TrackSelection, TuningProfile};

/// The largest brief read (a real one is a few KB).
pub const MAX_BRIEF: usize = 256 * 1024;
/// Notes at once per channel, as fm1c's default.
pub const POLY: usize = 6;

fn knowledge() -> Result<Knowledge, String> {
    Knowledge::load().map_err(|e| format!("the built-in knowledge base is damaged: {e}"))
}

/// The system prompt for the AI (what a brief is, the schema, the style and groove names).
pub fn prompt() -> Result<String, String> { Ok(t2m_intent::system_prompt(&knowledge()?)) }

/// The text of an OpenAI-compatible chat reply body, or why there is none.
pub fn reply_text(body: &str) -> Result<String, String> { t2m_intent::parse_response(body).map_err(|e| e.to_string()) }

/// The brief in the AI's answer, repaired and checked, as JSON; or what is wrong with it (said back to the AI).
pub fn brief_from_reply(answer: &str) -> Result<String, String> {
    let kb = knowledge()?;
    let json = t2m_intent::extract_json(answer).ok_or("no JSON object found")?;
    let brief = Brief::from_json(json).map_err(|e| e.to_string())?;
    let (brief, _) = repair(brief, &kb);
    check(&brief, &kb)?;
    Ok(brief.to_json())
}

fn check(brief: &Brief, kb: &Knowledge) -> Result<(), String> {
    validate(brief, kb).map_err(|issues| {
        issues.iter().take(3).map(|i| format!("{} {:?}: {}", i.path, i.kind, i.was)).collect::<Vec<_>>().join("; ")
    })
}

/// text2midi's MIDI for a brief, as `t2m-cli export --profile approx` writes it.
pub fn compose_midi(brief_json: &str, seed: u64) -> Result<Vec<u8>, String> {
    if brief_json.len() > MAX_BRIEF {
        return Err(format!("the brief is {} KB; at most {} KB is read", brief_json.len() / 1024, MAX_BRIEF / 1024));
    }
    let kb = knowledge()?;
    let brief = Brief::from_json(brief_json).map_err(|e| format!("not a brief: {e}"))?;
    check(&brief, &kb).map_err(|e| format!("the brief cannot be composed: {e}"))?;
    let score = t2m_compose::compose(&brief, &kb, seed);
    let opts = ExportOptions { tracks: TrackSelection::All, sections: SectionSelection::All, profile: TuningProfile::Approx12,
                               one_file_per_track: false, name: "piece".into() };
    let bundle = t2m_midi::export(&score, &kb, &opts).map_err(|e| e.to_string())?;
    bundle.files.into_iter().next().map(|f| f.1).ok_or_else(|| "text2midi wrote no MIDI".to_string())
}

/// A brief → (its MIDI, the piece): `fm1c compose BRIEF --seed N --name NAME`.
pub fn make(brief_json: &str, seed: u64, name: &str, poly: usize) -> Result<(Vec<u8>, fit::Fitted), String> {
    let midi = compose_midi(brief_json, seed)?;
    let fitted = fit::fit(&midi, name, poly)?;
    Ok((midi, fitted))
}

#[cfg(target_arch = "wasm32")]
mod web {
    use wasm_bindgen::prelude::*;

    fn js(e: String) -> JsValue { JsValue::from_str(&e) }

    /// A piece made in the page.
    #[wasm_bindgen]
    pub struct Made { midi: Vec<u8>, f1p: Vec<u8>, report: Vec<String>, summary: String, name: String, channels: u32, notes: u32, bars: u32, bpm: String }

    #[wasm_bindgen]
    impl Made {
        #[wasm_bindgen(getter)] pub fn midi(&self) -> Vec<u8> { self.midi.clone() }
        #[wasm_bindgen(getter)] pub fn f1p(&self) -> Vec<u8> { self.f1p.clone() }
        /// every change made to fit the FM-1, one per line
        #[wasm_bindgen(getter)] pub fn report(&self) -> String { self.report.join("\n") }
        #[wasm_bindgen(getter)] pub fn summary(&self) -> String { self.summary.clone() }
        #[wasm_bindgen(getter)] pub fn name(&self) -> String { self.name.clone() }
        #[wasm_bindgen(getter)] pub fn channels(&self) -> u32 { self.channels }
        #[wasm_bindgen(getter)] pub fn notes(&self) -> u32 { self.notes }
        #[wasm_bindgen(getter)] pub fn bars(&self) -> u32 { self.bars }
        #[wasm_bindgen(getter)] pub fn bpm(&self) -> String { self.bpm.clone() }
    }

    fn made(midi: Vec<u8>, f: crate::fit::Fitted) -> Made {
        let p = &f.piece;
        Made { summary: f.summary(&format!("{}.f1p", p.name)), name: p.name.clone(), channels: p.channels.len() as u32,
               notes: f.notes() as u32, bars: p.len_ticks / 384, bpm: crate::fit::bpm(p.tempo_x10), midi, f1p: f.bytes, report: f.report }
    }

    #[wasm_bindgen] pub fn prompt() -> Result<String, JsValue> { crate::prompt().map_err(js) }
    #[wasm_bindgen] pub fn reply_text(body: &str) -> Result<String, JsValue> { crate::reply_text(body).map_err(js) }
    #[wasm_bindgen] pub fn brief_from_reply(answer: &str) -> Result<String, JsValue> { crate::brief_from_reply(answer).map_err(js) }
    #[wasm_bindgen]
    pub fn make(brief_json: &str, seed: u32, name: &str) -> Result<Made, JsValue> {
        crate::make(brief_json, seed as u64, name, crate::POLY).map(|(m, f)| made(m, f)).map_err(js)
    }
    /// text2midi's window commands (desk.rs): one session and its preview audio
    #[wasm_bindgen]
    pub struct Desk(crate::desk::Desk);

    #[wasm_bindgen]
    impl Desk {
        #[wasm_bindgen(constructor)]
        pub fn new(sample_rate: u32) -> Result<Desk, JsValue> { crate::desk::Desk::new(sample_rate).map(Desk).map_err(js) }
        pub fn call(&mut self, cmd: &str, args: &str) -> Result<String, JsValue> { self.0.call(cmd, args).map_err(js) }
        pub fn render(&mut self, out: &mut [f32]) { self.0.render(out) }
        /// the open piece's MIDI files: [name, bytes, name, bytes, ...]
        pub fn export(&self, profile: &str) -> Result<js_sys_array::Files, JsValue> { self.0.export(profile).map(js_sys_array::Files).map_err(js) }
        /// the open piece, fitted for the FM-1
        pub fn fitted(&self) -> Result<Made, JsValue> { self.0.fitted(crate::POLY).map(|(m, f)| made(m, f)).map_err(js) }
    }

    pub mod js_sys_array {
        use wasm_bindgen::prelude::*;
        /// exported MIDI files
        #[wasm_bindgen]
        pub struct Files(pub(crate) Vec<(String, Vec<u8>)>);
        #[wasm_bindgen]
        impl Files {
            pub fn count(&self) -> usize { self.0.len() }
            pub fn name(&self, i: usize) -> String { self.0.get(i).map(|f| f.0.clone()).unwrap_or_default() }
            pub fn bytes(&self, i: usize) -> Vec<u8> { self.0.get(i).map(|f| f.1.clone()).unwrap_or_default() }
        }
    }

    /// a MIDI file the user brings → a piece
    #[wasm_bindgen]
    pub fn convert(midi: &[u8], name: &str) -> Result<Made, JsValue> {
        crate::fit::fit(midi, name, crate::POLY).map(|f| made(midi.to_vec(), f)).map_err(js)
    }
}
