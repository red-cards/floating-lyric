//! Parse angka yang formatnya mengikuti locale.
//!
//! AppleScript mengubah bilangan real menjadi teks sesuai locale sistem,
//! jadi di locale Indonesia `12.5` tercetak sebagai `"12,5"`.

/// Parse bilangan desimal yang bisa memakai koma atau titik sebagai pemisah
/// desimal. Kalau keduanya muncul, pemisah yang terakhir dianggap desimal dan
/// sisanya dianggap pemisah ribuan.
pub fn parse_locale_f64(input: &str) -> Option<f64> {
    let s: String = input
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{a0}' && *c != '\u{202f}')
        .collect();
    if s.is_empty() {
        return None;
    }
    let last_sep = s.rfind([',', '.']);
    let normalized = match last_sep {
        Some(idx) => {
            let (int_part, frac_part) = s.split_at(idx);
            let int_part: String = int_part
                .chars()
                .filter(|c| *c != ',' && *c != '.')
                .collect();
            format!("{}.{}", int_part, &frac_part[1..])
        }
        None => s,
    };
    normalized.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comma_decimal() {
        assert_eq!(parse_locale_f64("12,5"), Some(12.5));
        assert_eq!(parse_locale_f64(" 0,25\n"), Some(0.25));
    }

    #[test]
    fn parses_dot_decimal() {
        assert_eq!(parse_locale_f64("12.5"), Some(12.5));
        assert_eq!(parse_locale_f64("187"), Some(187.0));
    }

    #[test]
    fn parses_thousands_separators() {
        assert_eq!(parse_locale_f64("1.234,5"), Some(1234.5));
        assert_eq!(parse_locale_f64("1,234.5"), Some(1234.5));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_locale_f64(""), None);
        assert_eq!(parse_locale_f64("abc"), None);
        assert_eq!(parse_locale_f64("missing value"), None);
    }
}
