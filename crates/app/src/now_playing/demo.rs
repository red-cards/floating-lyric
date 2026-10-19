//! Mode demo (`MINI_LYRICS_DEMO=1`): lagu palsu yang berulang, berguna untuk
//! mengecek UI tanpa Spotify/jaringan.
//!
//! Selain `1`, nilai lain menampilkan satu state tertentu:
//! `paused`, `loading`, `notfound`, `error`, `idle`.

use std::time::{Duration, Instant};

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
[00:25.00]Kalau besok pagi semua lampu kota padam, aku tetap di sini nungguin kamu pulang
[00:31.00]Atur offset lewat menu tray ✓
[00:35.00]Baris terakhir sebelum diulang dari awal
";

/// Posisi lagu saat skenario `paused`: baris kedua, karaoke ±40%.
const PAUSED_AT: f64 = 4.4;

/// State yang ditampilkan mode demo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
    Playing,
    Paused,
    Loading,
    NotFound,
    NetworkError,
    Idle,
}

/// Skenario dari `MINI_LYRICS_DEMO`; `None` = mode demo mati.
pub fn scenario() -> Option<Scenario> {
    let v = std::env::var("MINI_LYRICS_DEMO").ok()?;
    match v.trim().to_ascii_lowercase().as_str() {
        "" | "0" => None,
        "paused" => Some(Scenario::Paused),
        "loading" => Some(Scenario::Loading),
        "notfound" => Some(Scenario::NotFound),
        "error" => Some(Scenario::NetworkError),
        "idle" => Some(Scenario::Idle),
        _ => Some(Scenario::Playing),
    }
}

pub fn enabled() -> bool {
    scenario().is_some()
}

pub struct Source {
    started: Instant,
    scenario: Scenario,
}

impl Source {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            started: Instant::now(),
            scenario: scenario().unwrap_or(Scenario::Playing),
        })
    }
}

impl NowPlayingSource for Source {
    fn poll(&mut self) -> Result<Option<PlayerState>, String> {
        let now = Instant::now();
        let (pos, is_playing) = match self.scenario {
            Scenario::Idle => return Ok(None),
            Scenario::Paused => (PAUSED_AT, false),
            _ => (
                now.duration_since(self.started).as_secs_f64() % DURATION,
                true,
            ),
        };
        Ok(Some(PlayerState {
            source_app: "demo".into(),
            track_key: "demo".into(),
            title: "Lagu Demo".into(),
            artist: "MiniLyrics".into(),
            album: "Demo".into(),
            duration_secs: DURATION,
            position_secs: pos,
            position_sampled_at: now,
            is_playing,
        }))
    }
}

pub struct Provider;

impl LyricsProvider for Provider {
    fn fetch(&self, _query: &TrackQuery) -> FetchOutcome {
        match scenario() {
            Some(Scenario::NotFound) => FetchOutcome::NotFound,
            Some(Scenario::NetworkError) => {
                FetchOutcome::NetworkError("demo: koneksi ditolak".into())
            }
            Some(Scenario::Loading) => {
                // Biarkan request "menggantung" selamanya.
                loop {
                    std::thread::sleep(Duration::from_secs(3600));
                }
            }
            _ => FetchOutcome::Found(parse_lrc(LRC)),
        }
    }
}
