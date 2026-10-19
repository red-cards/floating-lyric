//! Perhitungan baris aktif dan progress efek karaoke.

use crate::model::LyricLine;

/// Durasi minimum isian karaoke (detik).
pub const MIN_FILL_SECS: f64 = 1.5;
/// Perkiraan durasi per karakter (detik).
pub const SECS_PER_CHAR: f64 = 0.12;

/// Indeks baris aktif: baris terakhir yang waktunya `<= position`.
/// `None` kalau posisi masih sebelum baris pertama.
pub fn active_index(lines: &[LyricLine], position: f64) -> Option<usize> {
    let idx = lines.partition_point(|l| l.time_secs <= position);
    idx.checked_sub(1)
}

/// Waktu (detik) isian karaoke baris `idx` selesai:
/// `min(waktu baris berikutnya, start + max(1.5, jumlah karakter × 0.12))`.
pub fn fill_end(lines: &[LyricLine], idx: usize) -> f64 {
    let line = &lines[idx];
    let chars = line.text.chars().count() as f64;
    let estimated = line.time_secs + (chars * SECS_PER_CHAR).max(MIN_FILL_SECS);
    match lines.get(idx + 1) {
        Some(next) => estimated.min(next.time_secs),
        None => estimated,
    }
}

/// Progress isian karaoke baris `idx` pada `position`, dalam rentang 0..=1.
pub fn progress(lines: &[LyricLine], idx: usize, position: f64) -> f32 {
    let start = lines[idx].time_secs;
    let end = fill_end(lines, idx);
    if end <= start {
        return 1.0;
    }
    (((position - start) / (end - start)).clamp(0.0, 1.0)) as f32
}

/// Tiga baris yang ditampilkan: sebelumnya, aktif, berikutnya.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame<'a> {
    pub prev: Option<&'a str>,
    pub current: Option<&'a str>,
    pub next: Option<&'a str>,
    /// Progress karaoke baris aktif (0..=1).
    pub progress: f32,
}

pub fn frame_at(lines: &[LyricLine], position: f64) -> Frame<'_> {
    match active_index(lines, position) {
        Some(i) => Frame {
            prev: i.checked_sub(1).map(|p| lines[p].text.as_str()),
            current: Some(lines[i].text.as_str()),
            next: lines.get(i + 1).map(|l| l.text.as_str()),
            progress: progress(lines, i, position),
        },
        None => Frame {
            prev: None,
            current: None,
            next: lines.first().map(|l| l.text.as_str()),
            progress: 0.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(spec: &[(f64, &str)]) -> Vec<LyricLine> {
        spec.iter()
            .map(|(t, s)| LyricLine {
                time_secs: *t,
                text: s.to_string(),
            })
            .collect()
    }

    #[test]
    fn active_index_boundaries() {
        let l = lines(&[(1.0, "a"), (5.0, "b"), (9.0, "c")]);
        assert_eq!(active_index(&l, 0.5), None);
        assert_eq!(active_index(&l, 1.0), Some(0));
        assert_eq!(active_index(&l, 4.99), Some(0));
        assert_eq!(active_index(&l, 5.0), Some(1));
        assert_eq!(active_index(&l, 100.0), Some(2));
        assert_eq!(active_index(&[], 3.0), None);
    }

    #[test]
    fn short_line_uses_min_fill() {
        // 3 karakter × 0.12 = 0.36 < 1.5 → isian 1.5 dtk.
        let l = lines(&[(10.0, "abc"), (20.0, "x")]);
        assert!((fill_end(&l, 0) - 11.5).abs() < 1e-9);
        assert!((progress(&l, 0, 10.75) - 0.5).abs() < 1e-6);
        assert_eq!(progress(&l, 0, 12.0), 1.0);
    }

    #[test]
    fn long_line_uses_char_estimate() {
        // 50 karakter × 0.12 = 6 dtk.
        let text = "a".repeat(50);
        let l = lines(&[(0.0, &text), (30.0, "x")]);
        assert!((fill_end(&l, 0) - 6.0).abs() < 1e-9);
        assert!((progress(&l, 0, 3.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn fill_is_capped_by_next_line() {
        let text = "a".repeat(50);
        let l = lines(&[(0.0, &text), (2.0, "x")]);
        assert!((fill_end(&l, 0) - 2.0).abs() < 1e-9);
        assert!((progress(&l, 0, 1.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn progress_counts_unicode_chars() {
        // 15 karakter CJK = 15 × 0.12 = 1.8 dtk (bukan jumlah byte).
        let l = lines(&[(0.0, "あいうえおかきくけこさしすせそ")]);
        assert!((fill_end(&l, 0) - 1.8).abs() < 1e-9);
    }

    #[test]
    fn frame_picks_prev_current_next() {
        let l = lines(&[(1.0, "a"), (5.0, "b"), (9.0, "c")]);
        let f = frame_at(&l, 6.0);
        assert_eq!(
            (f.prev, f.current, f.next),
            (Some("a"), Some("b"), Some("c"))
        );
        let f = frame_at(&l, 0.0);
        assert_eq!((f.prev, f.current, f.next), (None, None, Some("a")));
        let f = frame_at(&l, 10.0);
        assert_eq!((f.prev, f.current, f.next), (Some("b"), Some("c"), None));
    }
}
