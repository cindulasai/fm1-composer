//! Sound: a score played by a small built-in synthesiser.
//!
//! `piece_of` turns a score into notes with exact frequencies; `Engine` renders them to
//! audio buffers, with transport (play, stop, seek, loop) and mute/solo. The engine does
//! no I/O and never allocates while rendering, so a sound-device callback can drive it.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod engine;
mod piece;
mod synth;
#[cfg(test)]
mod testkit;

pub use engine::Engine;
pub use piece::{Piece, PlayNote, piece_of};
pub use synth::Timbre;
