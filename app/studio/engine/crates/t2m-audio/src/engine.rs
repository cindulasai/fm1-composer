//! The engine: transport, voices and mixing. No I/O, and no allocation while rendering.

use crate::piece::Piece;
use crate::synth::{Shape, Timbre};
use t2m_core::Tick;

const TICKS_PER_QUARTER: f64 = 960.0;
const MAX_VOICES: usize = 96;
/// How often (in frames) pitch bends are re-read.
const CONTROL_BLOCK: usize = 32;
const SILENT: f32 = 1e-4;
/// Seconds for a voice that is being cut off (stop, seek, mute) to fall silent.
const KILL_SECONDS: f32 = 0.008;
/// A hard ceiling on the notes accepted, so a hostile piece cannot exhaust memory.
const MAX_NOTES: usize = 4_000_000;

#[derive(Clone, Copy)]
struct Voice {
    active: bool,
    note: usize,
    timbre: Timbre,
    shape_decay: f32,
    shape_attack: f32,
    shape_release: f32,
    one_shot: bool,
    gain: f32,
    base_hz: f32,
    hz: f32,
    phase: f32,
    age: f32,
    env: f32,
    releasing: bool,
    /// Per-frame multiplier while releasing.
    release_mul: f32,
    pan: f32,
}

const IDLE: Voice = Voice {
    active: false,
    note: 0,
    timbre: Timbre::Sine,
    shape_decay: 0.0,
    shape_attack: 0.0,
    shape_release: 0.0,
    one_shot: false,
    gain: 0.0,
    base_hz: 0.0,
    hz: 0.0,
    phase: 0.0,
    age: 0.0,
    env: 0.0,
    releasing: false,
    release_mul: 0.0,
    pan: 0.0,
};

pub struct Engine {
    rate: f32,
    piece: Piece,
    cursor: usize,
    /// Position in ticks.
    pos: f64,
    playing: bool,
    looping: Option<(Tick, Tick)>,
    muted: Vec<bool>,
    soloed: Vec<bool>,
    voices: [Voice; MAX_VOICES],
    noise: u32,
    last_noise: f32,
}

impl Engine {
    /// An engine for `sample_rate` frames per second (clamped to 8,000–384,000).
    pub fn new(sample_rate: u32) -> Engine {
        Engine {
            rate: sample_rate.clamp(8_000, 384_000) as f32,
            piece: Piece {
                notes: Vec::new(),
                tempo_bpm: 120,
                tracks: 0,
                end: 0,
            },
            cursor: 0,
            pos: 0.0,
            playing: false,
            looping: None,
            muted: Vec::new(),
            soloed: Vec::new(),
            voices: [IDLE; MAX_VOICES],
            noise: 0x1234_5678,
            last_noise: 0.0,
        }
    }

    /// Replaces the piece. Sounding voices fade out; the position is kept (clamped to the
    /// new end) and notes that would start before it are skipped. Mute and solo are kept.
    pub fn load(&mut self, mut piece: Piece) {
        piece.notes.truncate(MAX_NOTES);
        piece.notes.sort_by_key(|n| n.start);
        if piece == self.piece {
            return;
        }
        self.kill_all();
        self.muted.resize(piece.tracks, false);
        self.soloed.resize(piece.tracks, false);
        self.piece = piece;
        self.pos = self.pos.min(self.piece.end.max(0) as f64);
        self.reset_cursor();
        if self.playing {
            self.resume_held_notes();
        }
    }

    /// Starts the held (not one-shot) notes that are in progress at the position, so
    /// a change to the piece does not leave a gap under long notes.
    fn resume_held_notes(&mut self) {
        for index in 0..self.cursor {
            let note = &self.piece.notes[index];
            if (note.end as f64) > self.pos
                && !note.timbre.shape().one_shot
                && self.audible(note.track)
            {
                self.start_voice(index);
            }
        }
    }

    pub fn play(&mut self) {
        if self.pos >= self.piece.end as f64 && self.looping.is_none() {
            self.pos = 0.0;
            self.reset_cursor();
        }
        self.playing = true;
    }

    /// Stops; the position stays where it is and sound fades within a few milliseconds.
    pub fn stop(&mut self) {
        self.playing = false;
        self.kill_all();
    }

    pub fn seek(&mut self, tick: Tick) {
        self.kill_all();
        self.pos = tick.clamp(0, self.piece.end.max(0)) as f64;
        self.reset_cursor();
    }

    /// Loops between two ticks (start < end); anything else clears the loop.
    pub fn set_loop(&mut self, range: Option<(Tick, Tick)>) {
        self.looping = range.filter(|&(a, b)| a >= 0 && b > a);
    }

    pub fn set_mute(&mut self, track: usize, on: bool) {
        let before = self.audibility();
        if let Some(flag) = self.muted.get_mut(track) {
            *flag = on;
        }
        self.after_mix_change(&before);
    }

    pub fn set_solo(&mut self, track: usize, on: bool) {
        let before = self.audibility();
        if let Some(flag) = self.soloed.get_mut(track) {
            *flag = on;
        }
        self.after_mix_change(&before);
    }

    fn audibility(&self) -> Vec<bool> {
        (0..self.piece.tracks).map(|t| self.audible(t)).collect()
    }

    /// Cuts what is no longer audible and, while playing, brings back the held notes of parts
    /// that just became audible, so unmuting a pad mid-note is heard at once.
    fn after_mix_change(&mut self, before: &[bool]) {
        self.cut_inaudible();
        if !self.playing {
            return;
        }
        for track in 0..self.piece.tracks {
            if !before.get(track).copied().unwrap_or(false) && self.audible(track) {
                self.resume_track(track);
            }
        }
    }

    fn resume_track(&mut self, track: usize) {
        for index in 0..self.cursor {
            let note = &self.piece.notes[index];
            if note.track == track && (note.end as f64) > self.pos && !note.timbre.shape().one_shot
            {
                self.start_voice(index);
            }
        }
    }

    pub fn position(&self) -> Tick {
        self.pos as Tick
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    fn audible(&self, track: usize) -> bool {
        let solo = self.soloed.iter().any(|&s| s);
        !self.muted.get(track).copied().unwrap_or(false)
            && (!solo || self.soloed.get(track).copied().unwrap_or(false))
    }

    fn reset_cursor(&mut self) {
        let pos = self.pos;
        self.cursor = self.piece.notes.partition_point(|n| (n.start as f64) < pos);
    }

    fn kill_all(&mut self) {
        let mul = self.release_mul(KILL_SECONDS);
        for v in &mut self.voices {
            if v.active {
                v.releasing = true;
                v.one_shot = false;
                v.release_mul = mul;
            }
        }
    }

    fn cut_inaudible(&mut self) {
        let mul = self.release_mul(KILL_SECONDS);
        for i in 0..MAX_VOICES {
            let v = self.voices[i];
            if v.active && !self.audible(self.piece.notes.get(v.note).map_or(0, |n| n.track)) {
                self.voices[i].releasing = true;
                self.voices[i].one_shot = false;
                self.voices[i].release_mul = mul;
            }
        }
    }

    /// Per-frame multiplier that makes a level fall by 60 dB in `seconds`.
    fn release_mul(&self, seconds: f32) -> f32 {
        let frames = (seconds * self.rate).max(1.0);
        (1e-3f32).powf(1.0 / frames)
    }

    fn start_voice(&mut self, index: usize) {
        let note = &self.piece.notes[index];
        let shape: Shape = note.timbre.shape();
        let slot = self
            .voices
            .iter()
            .position(|v| !v.active)
            .or_else(|| {
                // All busy: take the quietest.
                let mut best = 0;
                for (i, v) in self.voices.iter().enumerate() {
                    if v.env < self.voices[best].env {
                        best = i;
                    }
                }
                Some(best)
            })
            .unwrap_or(0);
        let decay_mul = shape.decay.map_or(1.0, |s| self.release_mul(s));
        let pan = ((note.track as f32 * 0.37).sin()) * 0.35;
        self.voices[slot] = Voice {
            active: true,
            note: index,
            timbre: note.timbre,
            shape_decay: decay_mul,
            shape_attack: 1.0 / (shape.attack * self.rate).max(1.0),
            shape_release: self.release_mul(shape.release),
            one_shot: shape.one_shot,
            gain: shape.gain * f32::from(note.vel) / 127.0,
            base_hz: note.hz,
            hz: note.hz,
            phase: 0.0,
            age: 0.0,
            env: 0.0,
            releasing: false,
            release_mul: self.release_mul(shape.release),
            pan,
        };
    }

    /// The voice's pitch now, in Hz, with any bend curve applied.
    fn bent_hz(&self, v: &Voice) -> f32 {
        let Some(note) = self.piece.notes.get(v.note) else {
            return v.base_hz;
        };
        if note.bend.is_empty() {
            return v.base_hz;
        }
        let ticks = ((self.pos - note.start as f64).max(0.0)) as f32;
        let mut cents = note.bend[0].1;
        let mut prev = (note.bend[0].0 as f32, note.bend[0].1);
        for &(after, c) in &note.bend {
            let at = after as f32;
            if ticks >= at {
                cents = c;
                prev = (at, c);
            } else {
                let span = (at - prev.0).max(1e-6);
                cents = prev.1 + (c - prev.1) * ((ticks - prev.0) / span);
                break;
            }
        }
        v.base_hz * (cents / 1200.0).exp2()
    }

    fn next_noise(&mut self) -> f32 {
        let mut x = self.noise;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.noise = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Fills `out` with interleaved stereo frames. Silence while stopped. Never allocates.
    pub fn render(&mut self, out: &mut [f32]) {
        let ticks_per_frame = f64::from(TICKS_PER_QUARTER as f32) * f64::from(self.piece.tempo_bpm)
            / 60.0
            / f64::from(self.rate);
        let inv_rate = 1.0 / self.rate;
        let frames = out.len() / 2;
        for frame in 0..frames {
            if self.playing {
                self.pos += ticks_per_frame;
                if let Some((from, to)) = self.looping
                    && self.pos >= to as f64
                {
                    self.kill_all();
                    let overshoot = self.pos - to as f64;
                    // Notes exactly at the loop start belong to the new pass.
                    self.pos = from as f64;
                    self.reset_cursor();
                    self.pos += overshoot;
                }
                while self.cursor < self.piece.notes.len()
                    && (self.piece.notes[self.cursor].start as f64) <= self.pos
                {
                    let index = self.cursor;
                    self.cursor += 1;
                    if self.audible(self.piece.notes[index].track) {
                        self.start_voice(index);
                    }
                }
            }
            let bend_now = frame % CONTROL_BLOCK == 0;
            let (mut left, mut right) = (0.0f32, 0.0f32);
            let mut any = false;
            for i in 0..MAX_VOICES {
                if !self.voices[i].active {
                    continue;
                }
                any = true;
                let mut v = self.voices[i];
                if bend_now {
                    v.hz = self.bent_hz(&v);
                }
                let noise = self.next_noise();
                let hz = v.timbre.drum_hz(v.hz, v.age);
                v.phase += hz * inv_rate;
                v.phase -= v.phase.floor();
                // Held notes release when their note ends; drums ring out by themselves.
                if !v.releasing && !v.one_shot {
                    let end = self.piece.notes.get(v.note).map_or(0, |n| n.end);
                    if self.playing && self.pos >= end as f64 {
                        v.releasing = true;
                        v.release_mul = v.shape_release;
                    }
                }
                if v.releasing {
                    v.env *= v.release_mul;
                } else {
                    if v.env < 1.0 && v.age * self.rate < 1.0 / v.shape_attack {
                        v.env = (v.env + v.shape_attack).min(1.0);
                    } else if v.shape_decay < 1.0 {
                        v.env *= v.shape_decay;
                    } else {
                        v.env = 1.0;
                    }
                }
                v.age += inv_rate;
                let s = v.timbre.sample(v.phase, v.age, noise) * v.env * v.gain;
                left += s * (1.0 - v.pan);
                right += s * (1.0 + v.pan);
                if v.env < SILENT && (v.releasing || v.shape_decay < 1.0) {
                    v.active = false;
                }
                self.voices[i] = v;
            }
            out[frame * 2] = soft_clip(left);
            out[frame * 2 + 1] = soft_clip(right);
            if !any
                && self.playing
                && self.cursor >= self.piece.notes.len()
                && self.looping.is_none()
                && self.pos >= self.piece.end as f64
            {
                self.playing = false;
                self.pos = self.piece.end as f64;
            }
        }
        for sample in out.iter_mut().skip(frames * 2) {
            *sample = 0.0;
        }
        self.last_noise = 0.0;
    }
}

/// Keeps the mix inside -1..1 without harsh clipping; also removes any NaN.
fn soft_clip(x: f32) -> f32 {
    if x.is_finite() { x.tanh() } else { 0.0 }
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
