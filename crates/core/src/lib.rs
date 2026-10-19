//! Logika inti MiniLyrics yang tidak bergantung pada GUI maupun OS.
//!
//! Semua yang ada di sini bisa dites dengan `cargo test -p mini-lyrics-core`
//! di Linux sekalipun.

pub mod applescript;
pub mod controller;
pub mod karaoke;
pub mod locale;
pub mod lrc;
pub mod lrclib;
pub mod model;
pub mod wintime;

pub use controller::{LyricsController, LyricsProvider, LyricsView};
pub use model::{FetchOutcome, LyricLine, LyricsStatus, PlayerState, TrackQuery};
