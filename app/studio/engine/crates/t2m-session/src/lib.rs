//! The session behind the app window: one open piece, its seed, rerolls and locks, an
//! undo history, and the commands that change them. It knows nothing of any UI toolkit;
//! every command returns a `View` the window can draw.

#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

mod prompt;
mod session;
mod view;

pub use session::{GalleryItem, Session, SessionError};
pub use view::{SectionView, TrackView, View};
