use std::sync::Arc;
use std::time::{Duration, Instant};

/// Status player yang sedang memutar lagu, hasil polling now-playing.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerState {
    /// Nama/ID aplikasi sumber (mis. "Spotify" atau AUMID di Windows).
    pub source_app: String,
    /// Kunci unik lagu. Lirik hanya di-fetch ulang saat kunci ini berubah.
    pub track_key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Durasi lagu dalam detik (0 kalau tidak diketahui).
    pub duration_secs: f64,
    /// Posisi pemutaran (detik) pada saat `position_sampled_at`.
    pub position_secs: f64,
    /// Kapan `position_secs` diambil. Dipakai untuk interpolasi posisi.
    pub position_sampled_at: Instant,
    pub is_playing: bool,
}

impl PlayerState {
    /// Posisi terinterpolasi (tanpa offset) pada waktu `now`.
    ///
    /// Saat playing: posisi sampel + waktu berlalu sejak sampel.
    /// Saat pause: posisi sampel apa adanya.
    pub fn position_at(&self, now: Instant) -> f64 {
        let mut pos = self.position_secs;
        if self.is_playing {
            pos += now
                .saturating_duration_since(self.position_sampled_at)
                .as_secs_f64();
        }
        if self.duration_secs > 0.0 {
            pos = pos.min(self.duration_secs);
        }
        pos.max(0.0)
    }

    /// Umur sampel posisi pada waktu `now`.
    pub fn sample_age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.position_sampled_at)
    }

    pub fn query(&self) -> TrackQuery {
        TrackQuery {
            track_key: self.track_key.clone(),
            title: self.title.clone(),
            artist: self.artist.clone(),
            album: self.album.clone(),
            duration_secs: self.duration_secs,
        }
    }
}

/// Data yang dibutuhkan untuk mencari lirik sebuah lagu.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackQuery {
    pub track_key: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
}

/// Satu baris lirik tersinkron.
#[derive(Debug, Clone, PartialEq)]
pub struct LyricLine {
    /// Waktu mulai baris dalam detik.
    pub time_secs: f64,
    pub text: String,
}

/// Satu kandidat lirik alternatif (mis. hasil pencarian LRCLIB lain) yang
/// bisa dipilih user kalau lirik yang ketemu tidak cocok.
#[derive(Debug, Clone, PartialEq)]
pub struct LyricsCandidate {
    /// ID record di sumber lirik (LRCLIB), kalau ada.
    pub id: Option<i64>,
    /// Label singkat untuk UI/log, mis. "Judul — Artis · 3:51".
    pub label: String,
    pub lines: Vec<LyricLine>,
}

/// Label kandidat: "Judul — Artis · m:ss" (bagian kosong dilewati).
pub fn candidate_label(title: &str, artist: &str, duration_secs: Option<f64>) -> String {
    let mut out = title.trim().to_string();
    if !artist.trim().is_empty() {
        if !out.is_empty() {
            out.push_str(" — ");
        }
        out.push_str(artist.trim());
    }
    if let Some(d) = duration_secs.filter(|d| *d > 0.0) {
        let s = d.round() as u64;
        out.push_str(&format!(" · {}:{:02}", s / 60, s % 60));
    }
    out
}

/// Hasil satu kali percobaan mengambil lirik.
#[derive(Debug, Clone, PartialEq)]
pub enum FetchOutcome {
    Found(Vec<LyricLine>),
    NotFound,
    /// Timeout, DNS, diblok firewall/proxy, HTTP 5xx, dsb.
    NetworkError(String),
}

/// Status lirik untuk lagu yang sedang diputar.
#[derive(Debug, Clone, PartialEq)]
pub enum LyricsStatus {
    /// Belum ada lagu.
    Idle,
    Loading,
    Ready(Arc<Vec<LyricLine>>),
    NotFound,
    NetworkError(String),
}

impl LyricsStatus {
    pub fn label(&self) -> &'static str {
        match self {
            LyricsStatus::Idle => "Idle",
            LyricsStatus::Loading => "Loading",
            LyricsStatus::Ready(_) => "Ready",
            LyricsStatus::NotFound => "NotFound",
            LyricsStatus::NetworkError(_) => "NetworkError",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(pos: f64, playing: bool, at: Instant) -> PlayerState {
        PlayerState {
            source_app: "test".into(),
            track_key: "k".into(),
            title: "t".into(),
            artist: "a".into(),
            album: "b".into(),
            duration_secs: 100.0,
            position_secs: pos,
            position_sampled_at: at,
            is_playing: playing,
        }
    }

    #[test]
    fn interpolates_only_while_playing() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_millis(2500);
        assert!((state(10.0, true, t0).position_at(later) - 12.5).abs() < 1e-9);
        assert!((state(10.0, false, t0).position_at(later) - 10.0).abs() < 1e-9);
    }

    #[test]
    fn interpolation_is_clamped_to_duration() {
        let t0 = Instant::now();
        let later = t0 + Duration::from_secs(60);
        assert_eq!(state(90.0, true, t0).position_at(later), 100.0);
    }
}
