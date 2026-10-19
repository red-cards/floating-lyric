//! Parser output AppleScript Spotify (macOS). Ditaruh di core supaya bisa
//! dites di semua OS; pemanggilan `osascript`-nya ada di crate app.

use std::time::Instant;

use crate::locale::parse_locale_f64;
use crate::model::PlayerState;

pub const NOT_RUNNING: &str = "NOT_RUNNING";
pub const STOPPED: &str = "STOPPED";
pub const OK: &str = "OK";

/// Script yang dijalankan tiap detik lewat `osascript -e`.
///
/// - Cek `is running` dulu supaya Spotify tidak ikut terbuka.
/// - Field dipisah linefeed: OK, id, name, artist, album, duration (ms),
///   player position (detik, real berlocale), player state.
pub const SPOTIFY_SCRIPT: &str = r#"if application "Spotify" is running then
	tell application "Spotify"
		with timeout of 2 seconds
			set st to (player state as string)
			if st is "stopped" then return "STOPPED"
			set t to current track
			return "OK" & linefeed & (id of t) & linefeed & (name of t) & linefeed & (artist of t) & linefeed & (album of t) & linefeed & ((duration of t) as string) & linefeed & ((player position) as string) & linefeed & st
		end timeout
	end tell
end if
return "NOT_RUNNING""#;

/// Parse stdout `osascript`. `Ok(None)` = Spotify tidak jalan / stop.
pub fn parse_spotify_output(
    stdout: &str,
    sampled_at: Instant,
) -> Result<Option<PlayerState>, String> {
    let text = stdout.trim_end_matches(['\n', '\r']);
    let first = text.lines().next().unwrap_or("").trim();
    if first == NOT_RUNNING || first == STOPPED || text.trim().is_empty() {
        return Ok(None);
    }
    if first != OK {
        return Err(format!("output AppleScript tak dikenal: {text:?}"));
    }
    let fields: Vec<&str> = text.split('\n').map(|f| f.trim_end_matches('\r')).collect();
    if fields.len() < 8 {
        return Err(format!(
            "output AppleScript kurang field ({}): {text:?}",
            fields.len()
        ));
    }
    let id = fields[1].trim();
    let duration_ms = parse_locale_f64(fields[5])
        .ok_or_else(|| format!("durasi tidak valid: {:?}", fields[5]))?;
    let position = parse_locale_f64(fields[6])
        .ok_or_else(|| format!("posisi tidak valid: {:?}", fields[6]))?;
    let state = fields[7].trim().to_lowercase();
    // Kalau terminologi Spotify tidak termuat, state bisa berupa kode
    // «constant ****kPSP» (playing) / kPSp (paused).
    let is_playing = state == "playing" || fields[7].contains("kPSP");
    Ok(Some(PlayerState {
        source_app: "Spotify".into(),
        track_key: if id.is_empty() {
            format!("{}|{}|{}", fields[2], fields[3], fields[4])
        } else {
            id.to_string()
        },
        title: fields[2].trim().to_string(),
        artist: fields[3].trim().to_string(),
        album: fields[4].trim().to_string(),
        duration_secs: duration_ms / 1000.0,
        position_secs: position,
        position_sampled_at: sampled_at,
        is_playing,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_indonesian_locale_output() {
        let out = "OK\nspotify:track:abc\nJudul\nArtis\nAlbum\n215000\n12,5\nplaying\n";
        let p = parse_spotify_output(out, Instant::now()).unwrap().unwrap();
        assert_eq!(p.track_key, "spotify:track:abc");
        assert_eq!(p.title, "Judul");
        assert_eq!(p.duration_secs, 215.0);
        assert_eq!(p.position_secs, 12.5);
        assert!(p.is_playing);
    }

    #[test]
    fn parses_paused_and_dot_locale() {
        let out = "OK\nid\nT\nA\nB\n1000\n0.25\npaused";
        let p = parse_spotify_output(out, Instant::now()).unwrap().unwrap();
        assert_eq!(p.position_secs, 0.25);
        assert!(!p.is_playing);
    }

    #[test]
    fn raw_constant_states() {
        let playing = "OK\nid\nT\nA\nB\n1000\n1\n«constant ****kPSP»";
        let paused = "OK\nid\nT\nA\nB\n1000\n1\n«constant ****kPSp»";
        assert!(
            parse_spotify_output(playing, Instant::now())
                .unwrap()
                .unwrap()
                .is_playing
        );
        assert!(
            !parse_spotify_output(paused, Instant::now())
                .unwrap()
                .unwrap()
                .is_playing
        );
    }

    #[test]
    fn not_running_and_stopped_are_none() {
        assert_eq!(
            parse_spotify_output("NOT_RUNNING\n", Instant::now()),
            Ok(None)
        );
        assert_eq!(parse_spotify_output("STOPPED", Instant::now()), Ok(None));
        assert_eq!(parse_spotify_output("", Instant::now()), Ok(None));
    }

    #[test]
    fn garbage_is_error() {
        assert!(parse_spotify_output("huh", Instant::now()).is_err());
        assert!(parse_spotify_output("OK\nid\nT", Instant::now()).is_err());
        assert!(parse_spotify_output("OK\nid\nT\nA\nB\nx\n1\nplaying", Instant::now()).is_err());
    }
}
