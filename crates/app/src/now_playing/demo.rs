//! Mode demo (`MINI_LYRICS_DEMO=1`): lagu palsu yang berulang, berguna untuk
//! mengecek UI tanpa Spotify/jaringan.

use std::time::Instant;

use mini_lyrics_core::lrc::parse_lrc;
use mini_lyrics_core::{FetchOutcome, LyricsProvider, PlayerState, TrackQuery};

use super::NowPlayingSource;

pub const DURATION: f64 = 40.0;

pub const LRC: &str = "\
[00:00.50]♪ MiniLyrics mode demo ♪
[00:03.00]Lirik melayang di atas editor
[00:07.00]Sambil ngoding sambil karaokean
[00:11.50]日本語の歌詞もちゃんと表示される
[00:15.50]한국어 가사도 보여요
[00:19.00]中文歌词也没问题
[00:23.00]
[00:25.00]Baris terakhir sebelum diulang dari awal
[00:31.00]Atur offset lewat menu tray ✓
";

pub fn enabled() -> bool {
    std::env::var("MINI_LYRICS_DEMO").is_ok_and(|v| !v.is_empty() && v != "0")
}

pub struct Source {
    started: Instant,
}

impl Source {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            started: Instant::now(),
        })
    }
}

impl NowPlayingSource for Source {
    fn poll(&mut self) -> Result<Option<PlayerState>, String> {
        let now = Instant::now();
        let pos = now.duration_since(self.started).as_secs_f64() % DURATION;
        Ok(Some(PlayerState {
            source_app: "demo".into(),
            track_key: "demo".into(),
            title: "Lagu Demo".into(),
            artist: "MiniLyrics".into(),
            album: "Demo".into(),
            duration_secs: DURATION,
            position_secs: pos,
            position_sampled_at: now,
            is_playing: true,
        }))
    }
}

pub struct Provider;

impl LyricsProvider for Provider {
    fn fetch(&self, _query: &TrackQuery) -> FetchOutcome {
        FetchOutcome::Found(parse_lrc(LRC))
    }
}
