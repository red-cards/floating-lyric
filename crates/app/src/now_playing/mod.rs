//! Poller now-playing: thread terpisah yang menanyai player tiap 1 detik dan
//! menaruh hasilnya di `Arc<Mutex<NowPlaying>>` untuk dibaca UI.

use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mini_lyrics_core::PlayerState;

pub mod demo;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos::Source;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows::Source;

#[cfg(not(any(windows, target_os = "macos")))]
mod unsupported;
#[cfg(not(any(windows, target_os = "macos")))]
use unsupported::Source;

pub const POLL_INTERVAL: Duration = Duration::from_secs(1);

/// Sumber data now-playing per OS.
pub trait NowPlayingSource {
    /// `Ok(None)` = tidak ada yang diputar.
    fn poll(&mut self) -> Result<Option<PlayerState>, String>;
}

/// Data yang dibagi ke UI.
#[derive(Debug, Clone, Default)]
pub struct NowPlaying {
    pub player: Option<PlayerState>,
    pub last_error: Option<String>,
    pub last_poll: Option<Instant>,
    /// Lama satu kali polling (untuk debug).
    pub poll_duration: Option<Duration>,
}

pub type SharedNowPlaying = Arc<Mutex<NowPlaying>>;

/// Jalankan thread poller. `on_update` dipanggil setiap selesai polling
/// (dipakai untuk membangunkan UI).
pub fn spawn(on_update: impl Fn() + Send + 'static) -> SharedNowPlaying {
    let shared: SharedNowPlaying = Arc::default();
    let out = shared.clone();
    let spawned = thread::Builder::new()
        .name("now-playing".into())
        .spawn(move || run(shared, on_update));
    if let Err(e) = spawned {
        log::error!("gagal menjalankan thread now-playing: {e}");
    }
    out
}

fn run(shared: SharedNowPlaying, on_update: impl Fn()) {
    // Source dibuat di thread ini (Windows: apartment WinRT per thread).
    let demo = demo::enabled();
    let mut source: Option<Box<dyn NowPlayingSource>> = None;
    let mut last_logged_error: Option<String> = None;
    let mut last_track: Option<String> = None;
    loop {
        let started = Instant::now();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if source.is_none() {
                source = Some(if demo {
                    Box::new(demo::Source::new()?)
                } else {
                    Box::new(Source::new()?)
                });
            }
            source.as_mut().expect("baru dibuat").poll()
        }))
        .unwrap_or_else(|_| {
            source = None;
            Err("panic di poller now-playing".into())
        });
        let poll_duration = started.elapsed();

        match &result {
            Ok(player) => {
                last_logged_error = None;
                let key = player.as_ref().map(|p| p.track_key.clone());
                if key != last_track {
                    match player {
                        Some(p) => log::info!(
                            "now playing [{}]: \"{}\" — \"{}\" ({:.0}s)",
                            p.source_app,
                            p.title,
                            p.artist,
                            p.duration_secs
                        ),
                        None => log::info!("tidak ada lagu yang diputar"),
                    }
                    last_track = key;
                }
            }
            Err(e) => {
                // Log hanya saat pesan error berubah supaya file log tidak penuh.
                if last_logged_error.as_ref() != Some(e) {
                    log::warn!("now-playing error: {e}");
                    last_logged_error = Some(e.clone());
                }
            }
        }

        if let Ok(mut s) = shared.lock() {
            s.last_poll = Some(Instant::now());
            s.poll_duration = Some(poll_duration);
            match result {
                Ok(player) => {
                    s.player = player;
                    s.last_error = None;
                }
                Err(e) => {
                    s.player = None;
                    s.last_error = Some(e);
                }
            }
        }
        on_update();

        thread::sleep(
            POLL_INTERVAL
                .saturating_sub(started.elapsed())
                .max(Duration::from_millis(200)),
        );
    }
}
