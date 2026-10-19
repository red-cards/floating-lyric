//! Klien LRCLIB (https://lrclib.net) — sumber lirik gratis tanpa API key.

use std::time::Duration;

use serde::Deserialize;

use crate::lrc::parse_lrc;
use crate::model::{FetchOutcome, TrackQuery};

pub const BASE_URL: &str = "https://lrclib.net";

/// Satu record lirik dari LRCLIB.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LrclibRecord {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub track_name: Option<String>,
    #[serde(default)]
    pub artist_name: Option<String>,
    #[serde(default)]
    pub album_name: Option<String>,
    #[serde(default)]
    pub duration: Option<f64>,
    #[serde(default)]
    pub instrumental: Option<bool>,
    #[serde(default)]
    pub synced_lyrics: Option<String>,
}

impl LrclibRecord {
    fn synced(&self) -> Option<&str> {
        self.synced_lyrics
            .as_deref()
            .filter(|s| !s.trim().is_empty())
    }
}

/// Bersihkan judul untuk pencarian: potong setelah `" - "` (mis. "- Remastered
/// 2011") dan setelah `" (feat"` / `" [feat"`.
pub fn clean_title(title: &str) -> String {
    let mut end = title.len();
    if let Some(i) = title.find(" - ") {
        end = end.min(i);
    }
    for pat in [" (feat", " [feat", " (ft.", " [ft."] {
        if let Some(i) = find_ascii_ci(title, pat) {
            end = end.min(i);
        }
    }
    let cleaned = title[..end].trim();
    if cleaned.is_empty() {
        title.trim().to_string()
    } else {
        cleaned.to_string()
    }
}

fn find_ascii_ci(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return None;
    }
    (0..=h.len() - n.len())
        .find(|&i| haystack.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Pilih hasil pencarian yang punya `syncedLyrics` dengan durasi paling dekat
/// ke `duration_secs`. Kalau durasi tidak diketahui (<= 0), ambil hasil pertama
/// yang punya lirik sinkron.
pub fn pick_best(records: &[LrclibRecord], duration_secs: f64) -> Option<&LrclibRecord> {
    let with_synced = records.iter().filter(|r| r.synced().is_some());
    if duration_secs <= 0.0 {
        return with_synced.into_iter().next();
    }
    with_synced.min_by(|a, b| {
        let da = a.duration.map_or(f64::MAX, |d| (d - duration_secs).abs());
        let db = b.duration.map_or(f64::MAX, |d| (d - duration_secs).abs());
        da.total_cmp(&db)
    })
}

/// Klien HTTP blocking untuk LRCLIB.
///
/// reqwest otomatis membaca proxy dari env `HTTP_PROXY`/`HTTPS_PROXY`/`NO_PROXY`
/// (berguna di jaringan kantor) — jangan panggil `.no_proxy()`.
pub struct LrclibClient {
    http: reqwest::blocking::Client,
    base_url: String,
}

impl LrclibClient {
    pub fn new(app_version: &str) -> Result<Self, String> {
        Self::with_base_url(app_version, BASE_URL)
    }

    pub fn with_base_url(app_version: &str, base_url: &str) -> Result<Self, String> {
        let http = reqwest::blocking::Client::builder()
            .user_agent(format!(
                "MiniLyrics/{app_version} (https://github.com/red-cards/floating-lyric)"
            ))
            .connect_timeout(Duration::from_secs(8))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|e| format!("gagal membuat HTTP client: {}", error_chain(&e)))?;
        Ok(Self {
            http,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    /// Langkah 1: `/api/get` dengan judul, artis, album, durasi.
    /// `Ok(None)` artinya tidak ada lirik sinkron (404 atau hanya plain).
    pub fn get_exact(&self, q: &TrackQuery) -> Result<Option<LrclibRecord>, String> {
        let duration = q.duration_secs.round().max(0.0) as u64;
        let url = format!("{}/api/get", self.base_url);
        let resp = self
            .http
            .get(&url)
            .query(&[
                ("track_name", q.title.as_str()),
                ("artist_name", q.artist.as_str()),
                ("album_name", q.album.as_str()),
                ("duration", duration.to_string().as_str()),
            ])
            .send()
            .map_err(|e| format!("GET {url}: {}", error_chain(&e)))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(format!("GET {url}: HTTP {status}"));
        }
        let rec: LrclibRecord = resp
            .json()
            .map_err(|e| format!("GET {url}: respons tidak valid: {}", error_chain(&e)))?;
        Ok(rec.synced().is_some().then_some(rec))
    }

    /// Langkah 2: `/api/search` dengan judul yang sudah dibersihkan.
    pub fn search(&self, title: &str, artist: &str) -> Result<Vec<LrclibRecord>, String> {
        let url = format!("{}/api/search", self.base_url);
        let resp = self
            .http
            .get(&url)
            .query(&[("track_name", title), ("artist_name", artist)])
            .send()
            .map_err(|e| format!("GET {url}: {}", error_chain(&e)))?;
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if !status.is_success() {
            return Err(format!("GET {url}: HTTP {status}"));
        }
        resp.json()
            .map_err(|e| format!("GET {url}: respons tidak valid: {}", error_chain(&e)))
    }

    /// Alur lengkap: get → (fallback) search → parse LRC.
    pub fn fetch(&self, q: &TrackQuery) -> FetchOutcome {
        if q.title.trim().is_empty() {
            return FetchOutcome::NotFound;
        }
        // /api/get butuh semua parameter; lewati kalau album/durasi kosong.
        if !q.album.trim().is_empty() && q.duration_secs > 0.0 {
            match self.get_exact(q) {
                Ok(Some(rec)) => {
                    if let Some(out) = to_outcome(&rec) {
                        return out;
                    }
                }
                Ok(None) => {}
                Err(e) => return FetchOutcome::NetworkError(e),
            }
        }
        let title = clean_title(&q.title);
        match self.search(&title, &q.artist) {
            Ok(records) => pick_best(&records, q.duration_secs)
                .and_then(to_outcome)
                .unwrap_or(FetchOutcome::NotFound),
            Err(e) => FetchOutcome::NetworkError(e),
        }
    }
}

fn to_outcome(rec: &LrclibRecord) -> Option<FetchOutcome> {
    let lines = parse_lrc(rec.synced()?);
    (!lines.is_empty()).then_some(FetchOutcome::Found(lines))
}

/// Gabungkan rantai `source()` supaya pesan error lengkap (berguna saat kena
/// proxy/firewall kantor: "error sending request → connection refused" dsb).
pub fn error_chain(e: &dyn std::error::Error) -> String {
    let mut msg = e.to_string();
    let mut cur = e.source();
    while let Some(s) = cur {
        let s_msg = s.to_string();
        if !msg.contains(&s_msg) {
            msg.push_str(" → ");
            msg.push_str(&s_msg);
        }
        cur = s.source();
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_title_cuts_suffixes() {
        assert_eq!(clean_title("Yesterday - Remastered 2009"), "Yesterday");
        assert_eq!(clean_title("Song (feat. Someone)"), "Song");
        assert_eq!(clean_title("Song (Feat. Someone) - Live"), "Song");
        assert_eq!(clean_title("Song [feat. X]"), "Song");
        assert_eq!(clean_title("Lagu Biasa"), "Lagu Biasa");
        assert_eq!(clean_title("  Spasi  "), "Spasi");
    }

    #[test]
    fn clean_title_keeps_hyphenated_words_and_unicode() {
        assert_eq!(clean_title("Hip-Hop Song"), "Hip-Hop Song");
        assert_eq!(clean_title("夜に駆ける - Remix"), "夜に駆ける");
        assert_eq!(clean_title("Ünïcode (feat. Ä)"), "Ünïcode");
    }

    #[test]
    fn clean_title_never_returns_empty() {
        assert_eq!(clean_title(" - Intro"), "- Intro");
    }

    fn rec(id: i64, duration: Option<f64>, synced: Option<&str>) -> LrclibRecord {
        LrclibRecord {
            id: Some(id),
            track_name: None,
            artist_name: None,
            album_name: None,
            duration,
            instrumental: None,
            synced_lyrics: synced.map(str::to_string),
        }
    }

    #[test]
    fn pick_best_prefers_closest_duration_with_synced() {
        let rs = vec![
            rec(1, Some(200.0), Some("[00:01.00]a")),
            rec(2, Some(181.0), None), // paling dekat tapi tanpa lirik sinkron
            rec(3, Some(185.0), Some("[00:01.00]b")),
            rec(4, Some(170.0), Some("   ")),
        ];
        assert_eq!(pick_best(&rs, 180.0).and_then(|r| r.id), Some(3));
    }

    #[test]
    fn pick_best_without_duration_takes_first_synced() {
        let rs = vec![rec(1, Some(200.0), None), rec(2, None, Some("[00:01.00]a"))];
        assert_eq!(pick_best(&rs, 0.0).and_then(|r| r.id), Some(2));
        assert!(pick_best(&[rec(1, Some(1.0), None)], 1.0).is_none());
    }

    #[test]
    fn deserializes_lrclib_json() {
        let json = r#"[{"id":7,"trackName":"T","artistName":"A","albumName":"B",
            "duration":181.0,"instrumental":false,"plainLyrics":"x",
            "syncedLyrics":"[00:01.00]Halo"}]"#;
        let rs: Vec<LrclibRecord> = serde_json::from_str(json).unwrap();
        assert_eq!(rs[0].duration, Some(181.0));
        assert_eq!(
            to_outcome(&rs[0]),
            Some(FetchOutcome::Found(vec![crate::LyricLine {
                time_secs: 1.0,
                text: "Halo".into()
            }]))
        );
    }

    #[test]
    fn unreachable_server_is_network_error() {
        // Port 9 (discard) di localhost hampir pasti menolak koneksi.
        let c = LrclibClient::with_base_url("test", "http://127.0.0.1:9").unwrap();
        let q = TrackQuery {
            track_key: "k".into(),
            title: "T".into(),
            artist: "A".into(),
            album: "B".into(),
            duration_secs: 100.0,
        };
        assert!(matches!(c.fetch(&q), FetchOutcome::NetworkError(_)));
    }
}
