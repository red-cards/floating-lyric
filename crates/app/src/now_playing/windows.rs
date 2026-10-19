//! Now playing di Windows lewat Global System Media Transport Controls
//! (SMTC) — sumber yang sama dengan media overlay Windows, jadi Spotify
//! desktop, Spotify Web/YouTube Music di Chrome/Edge, dsb. ikut terbaca.
//!
//! Operasi async WinRT ditunggu blocking di thread poller. Di crate `windows`
//! 0.62 method `IAsyncOperation::get()` berganti nama menjadi `join()`.

use std::time::Instant;

use mini_lyrics_core::wintime::{
    filetime_to_unix_ms, now_unix_ms, sampled_instant, timespan_to_secs,
};
use mini_lyrics_core::PlayerState;
use windows::core::HSTRING;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as SessionManager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as PlaybackStatus,
};
use windows::Win32::System::WinRT::{RoInitialize, RO_INIT_MULTITHREADED};

use super::NowPlayingSource;

/// Patokan posisi buatan untuk Spotify, yang sering tidak memperbarui
/// Position/LastUpdatedTime saat resume dari pause.
struct Anchor {
    track_key: String,
    /// LastUpdatedTime (FILETIME) saat patokan dibuat.
    last_updated: i64,
    position_secs: f64,
    sampled_at: Instant,
    /// `false` = sedang pause, posisi beku di `position_secs`.
    running: bool,
}

/// Snapshot polling sebelumnya untuk mendeteksi transisi pause → play.
struct Previous {
    track_key: String,
    last_updated: i64,
    is_playing: bool,
    position_secs: f64,
}

pub struct Source {
    manager: Option<SessionManager>,
    previous: Option<Previous>,
    anchor: Option<Anchor>,
}

impl Source {
    pub fn new() -> Result<Self, String> {
        // Inisialisasi apartment WinRT (MTA) untuk thread poller ini.
        // Error "sudah diinisialisasi dengan mode lain" tidak fatal.
        if let Err(e) = unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
            log::warn!("RoInitialize: {e}");
        }
        Ok(Self {
            manager: None,
            previous: None,
            anchor: None,
        })
    }

    fn manager(&mut self) -> Result<SessionManager, String> {
        if let Some(m) = &self.manager {
            return Ok(m.clone());
        }
        let m = SessionManager::RequestAsync()
            .and_then(|op| op.join())
            .map_err(|e| format!("SMTC RequestAsync: {e}"))?;
        self.manager = Some(m.clone());
        Ok(m)
    }

    fn poll_inner(&mut self) -> Result<Option<PlayerState>, String> {
        let manager = self.manager()?;
        let Some(session) = pick_session(&manager)? else {
            self.previous = None;
            self.anchor = None;
            return Ok(None);
        };
        let polled_at = Instant::now();
        let now_ms = now_unix_ms();

        let source_app = session
            .SourceAppUserModelId()
            .map(|s| s.to_string())
            .unwrap_or_default();
        let props = session
            .TryGetMediaPropertiesAsync()
            .and_then(|op| op.join())
            .map_err(|e| format!("TryGetMediaPropertiesAsync: {e}"))?;
        let title = hstring(props.Title());
        let artist = hstring(props.Artist());
        let album = hstring(props.AlbumTitle());
        if title.trim().is_empty() {
            return Ok(None);
        }

        let timeline = session
            .GetTimelineProperties()
            .map_err(|e| format!("GetTimelineProperties: {e}"))?;
        let position = timeline.Position().map(|t| t.Duration).unwrap_or(0);
        let start = timeline.StartTime().map(|t| t.Duration).unwrap_or(0);
        let end = timeline.EndTime().map(|t| t.Duration).unwrap_or(0);
        let last_updated = timeline
            .LastUpdatedTime()
            .map(|d| d.UniversalTime)
            .unwrap_or(0);

        let is_playing = session
            .GetPlaybackInfo()
            .and_then(|i| i.PlaybackStatus())
            .map(|s| s == PlaybackStatus::Playing)
            .unwrap_or(false);

        let track_key = format!("{title}|{artist}|{album}");
        let raw_position = timespan_to_secs(position - start).max(0.0);
        let duration_secs = timespan_to_secs(end - start).max(0.0);

        // PENTING: Spotify jarang memperbarui Position di SMTC (biasanya hanya
        // saat ganti lagu atau seek). Waktu sampel diambil dari
        // LastUpdatedTime, bukan waktu polling.
        let mut position_secs = raw_position;
        let mut sampled_at = sampled_instant(filetime_to_unix_ms(last_updated), now_ms, polled_at);

        // Resume dari pause tanpa LastUpdatedTime baru: interpolasi dari
        // LastUpdatedTime lama akan ikut menghitung lamanya pause. Pakai
        // posisi terakhir sebagai patokan, mulai dari saat resume terdeteksi.
        let matches = |track: &str, lut: i64| track == track_key && lut == last_updated;
        if !self
            .anchor
            .as_ref()
            .is_some_and(|a| matches(&a.track_key, a.last_updated))
        {
            self.anchor = None;
            if let Some(prev) = &self.previous {
                if matches(&prev.track_key, prev.last_updated) && !prev.is_playing && is_playing {
                    log::debug!("resume tanpa update timeline SMTC, pakai patokan posisi pause");
                    self.anchor = Some(Anchor {
                        track_key: track_key.clone(),
                        last_updated,
                        position_secs: prev.position_secs,
                        sampled_at: resume_instant(polled_at),
                        running: true,
                    });
                }
            }
        }
        if let Some(a) = &mut self.anchor {
            if is_playing {
                if !a.running {
                    a.running = true;
                    a.sampled_at = resume_instant(polled_at);
                }
            } else if a.running {
                // Pause lagi (tetap tanpa update timeline): bekukan posisi.
                a.position_secs += polled_at
                    .saturating_duration_since(a.sampled_at)
                    .as_secs_f64();
                a.sampled_at = polled_at;
                a.running = false;
            }
            position_secs = a.position_secs;
            sampled_at = a.sampled_at;
        }

        let state = PlayerState {
            source_app,
            track_key,
            title,
            artist,
            album,
            duration_secs,
            position_secs,
            position_sampled_at: sampled_at,
            is_playing,
        };
        self.previous = Some(Previous {
            track_key: state.track_key.clone(),
            last_updated,
            is_playing,
            position_secs: state.position_at(polled_at),
        });
        Ok(Some(state))
    }
}

impl NowPlayingSource for Source {
    fn poll(&mut self) -> Result<Option<PlayerState>, String> {
        let result = self.poll_inner();
        if result.is_err() {
            // Minta ulang session manager di polling berikutnya.
            self.manager = None;
        }
        result
    }
}

/// Resume terjadi di antara polling sebelumnya dan sekarang; ambil tengahnya.
fn resume_instant(polled_at: Instant) -> Instant {
    polled_at
        .checked_sub(super::POLL_INTERVAL / 2)
        .unwrap_or(polled_at)
}

fn hstring(r: windows::core::Result<HSTRING>) -> String {
    r.map(|s| s.to_string()).unwrap_or_default()
}

/// Prioritas: Spotify desktop → session lain yang sedang Playing (termasuk
/// browser) → GetCurrentSession.
fn pick_session(manager: &SessionManager) -> Result<Option<Session>, String> {
    let sessions = manager
        .GetSessions()
        .map_err(|e| format!("GetSessions: {e}"))?;
    let count = sessions.Size().unwrap_or(0);
    let mut all = Vec::with_capacity(count as usize);
    for i in 0..count {
        if let Ok(s) = sessions.GetAt(i) {
            all.push(s);
        }
    }

    let app_id = |s: &Session| {
        s.SourceAppUserModelId()
            .map(|id| id.to_string())
            .unwrap_or_default()
    };
    let playing = |s: &Session| {
        s.GetPlaybackInfo()
            .and_then(|i| i.PlaybackStatus())
            .map(|st| st == PlaybackStatus::Playing)
            .unwrap_or(false)
    };

    if let Some(s) = all
        .iter()
        .find(|s| app_id(s).to_lowercase().contains("spotify"))
    {
        return Ok(Some(s.clone()));
    }
    if let Some(s) = all.iter().find(|s| playing(s)) {
        return Ok(Some(s.clone()));
    }
    Ok(manager.GetCurrentSession().ok())
}
