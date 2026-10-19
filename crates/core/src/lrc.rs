//! Parser format LRC (lirik tersinkron).

use std::sync::OnceLock;

use regex::Regex;

use crate::model::LyricLine;

fn timestamp_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // [mm:ss], [mm:ss.xx], [mm:ss:xx], [mm:ss.xxx]
    RE.get_or_init(|| Regex::new(r"\[(\d{1,3}):(\d{1,2})(?:[.:](\d{1,3}))?\]").unwrap())
}

/// Parse teks LRC menjadi daftar baris yang terurut berdasarkan waktu.
///
/// - Satu baris boleh punya beberapa timestamp (`[00:10.00][01:20.00]Reff`).
/// - Mendukung `[mm:ss.xx]` dan `[mm:ss:xx]`.
/// - Baris tag metadata (`[ar:...]`) dan baris tanpa timestamp diabaikan.
/// - Baris bertimestamp dengan teks kosong dipertahankan (penanda jeda).
pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let re = timestamp_re();
    let mut lines = Vec::new();
    for raw in input.lines() {
        let raw = raw.trim();
        let mut times = Vec::new();
        let mut rest = raw;
        // Timestamp hanya dihitung kalau berada di awal baris (berurutan).
        while let Some(m) = re.find(rest) {
            if m.start() != 0 {
                break;
            }
            let caps = re.captures(m.as_str()).expect("match implies captures");
            let min: f64 = caps[1].parse().unwrap_or(0.0);
            let sec: f64 = caps[2].parse().unwrap_or(0.0);
            let frac = caps
                .get(3)
                .map(|f| {
                    let digits = f.as_str();
                    let v: f64 = digits.parse().unwrap_or(0.0);
                    v / 10f64.powi(digits.len() as i32)
                })
                .unwrap_or(0.0);
            times.push(min * 60.0 + sec + frac);
            rest = rest[m.end()..].trim_start();
        }
        if times.is_empty() {
            continue;
        }
        let text = rest.trim().to_string();
        for t in times {
            lines.push(LyricLine {
                time_secs: t,
                text: text.clone(),
            });
        }
    }
    // sort_by stabil: baris dengan waktu sama tetap urut sesuai file.
    lines.sort_by(|a, b| a.time_secs.total_cmp(&b.time_secs));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn parses_basic_dot_format() {
        let l = parse_lrc("[00:12.34]Halo dunia\n[01:02.50]Baris dua");
        assert_eq!(l.len(), 2);
        assert!(approx(l[0].time_secs, 12.34));
        assert_eq!(l[0].text, "Halo dunia");
        assert!(approx(l[1].time_secs, 62.5));
    }

    #[test]
    fn parses_colon_fraction_format() {
        let l = parse_lrc("[00:05:20]Titik dua");
        assert_eq!(l.len(), 1);
        assert!(approx(l[0].time_secs, 5.2));
    }

    #[test]
    fn parses_milliseconds_and_no_fraction() {
        let l = parse_lrc("[00:01.250]ms\n[00:03]bulat");
        assert!(approx(l[0].time_secs, 1.25));
        assert!(approx(l[1].time_secs, 3.0));
    }

    #[test]
    fn expands_multiple_timestamps_and_sorts() {
        let l = parse_lrc("[00:30.00]B\n[00:10.00][00:50.00]Reff\n[00:20.00]A");
        let got: Vec<(f64, &str)> = l.iter().map(|x| (x.time_secs, x.text.as_str())).collect();
        assert_eq!(
            got,
            vec![(10.0, "Reff"), (20.0, "A"), (30.0, "B"), (50.0, "Reff")]
        );
    }

    #[test]
    fn keeps_empty_timed_lines_and_skips_blank_and_meta() {
        let input = "[ar:Artis]\n[ti:Judul]\n\n[00:01.00]Satu\n[00:04.00]\n   \nteks tanpa waktu\n[00:06.00] Dua ";
        let l = parse_lrc(input);
        let got: Vec<&str> = l.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(got, vec!["Satu", "", "Dua"]);
    }

    #[test]
    fn handles_crlf() {
        let l = parse_lrc("[00:01.00]A\r\n[00:02.00]B\r\n");
        assert_eq!(l.len(), 2);
        assert_eq!(l[1].text, "B");
    }
}
