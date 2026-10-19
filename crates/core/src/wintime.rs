//! Konversi satuan waktu WinRT (Windows SMTC). Murni aritmetika supaya bisa
//! dites di semua OS.
//!
//! - `TimeSpan` = jumlah tick 100 ns.
//! - `DateTime.UniversalTime` = FILETIME: tick 100 ns sejak 1601-01-01 UTC.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// FILETIME untuk 1970-01-01 UTC.
pub const FILETIME_UNIX_EPOCH: i64 = 116_444_736_000_000_000;
const TICKS_PER_SEC: f64 = 10_000_000.0;

pub fn timespan_to_secs(ticks: i64) -> f64 {
    ticks as f64 / TICKS_PER_SEC
}

/// FILETIME → milidetik Unix. `None` kalau nilainya kosong/tidak masuk akal.
pub fn filetime_to_unix_ms(filetime: i64) -> Option<i64> {
    (filetime > FILETIME_UNIX_EPOCH).then(|| (filetime - FILETIME_UNIX_EPOCH) / 10_000)
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Ubah waktu sampel (ms Unix, mis. dari `LastUpdatedTime`) menjadi `Instant`
/// relatif terhadap `now`. Waktu di masa depan (jam tidak sinkron) dianggap
/// `now`; `None` juga dianggap `now`.
pub fn sampled_instant(sample_unix_ms: Option<i64>, now_unix_ms: i64, now: Instant) -> Instant {
    let Some(sample) = sample_unix_ms else {
        return now;
    };
    let age_ms = (now_unix_ms - sample).max(0) as u64;
    now.checked_sub(Duration::from_millis(age_ms))
        .unwrap_or(now)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timespan_ticks_to_seconds() {
        assert_eq!(timespan_to_secs(10_000_000), 1.0);
        assert_eq!(timespan_to_secs(2_150_000_000), 215.0);
        assert_eq!(timespan_to_secs(5_000_000), 0.5);
    }

    #[test]
    fn filetime_conversion() {
        assert_eq!(filetime_to_unix_ms(FILETIME_UNIX_EPOCH + 10_000), Some(1));
        // 2024-01-01T00:00:00Z
        assert_eq!(
            filetime_to_unix_ms(133_485_408_000_000_000),
            Some(1_704_067_200_000)
        );
        assert_eq!(filetime_to_unix_ms(0), None);
    }

    #[test]
    fn sampled_instant_goes_back_by_age() {
        let now = Instant::now() + Duration::from_secs(100);
        let s = sampled_instant(Some(1_000), 4_000, now);
        assert_eq!(now - s, Duration::from_secs(3));
        assert_eq!(sampled_instant(Some(9_000), 4_000, now), now);
        assert_eq!(sampled_instant(None, 4_000, now), now);
    }
}
