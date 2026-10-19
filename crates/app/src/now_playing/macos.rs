//! Now playing di macOS lewat AppleScript ke Spotify (`osascript`).
//!
//! Tidak memakai private framework MediaRemote karena aksesnya dibatasi
//! sejak macOS 15.4.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use mini_lyrics_core::applescript::{parse_spotify_output, SPOTIFY_SCRIPT};
use mini_lyrics_core::PlayerState;

use super::NowPlayingSource;

const SCRIPT_TIMEOUT: Duration = Duration::from_secs(4);

pub struct Source;

impl Source {
    pub fn new() -> Result<Self, String> {
        Ok(Self)
    }
}

impl NowPlayingSource for Source {
    fn poll(&mut self) -> Result<Option<PlayerState>, String> {
        // Cek proses dulu: kalau Spotify tidak ter-install, `tell application`
        // bisa memunculkan dialog "Where is Spotify?".
        if !spotify_process_running() {
            return Ok(None);
        }
        let started = Instant::now();
        let stdout = run_osascript(SPOTIFY_SCRIPT)?;
        // Posisi diambil di tengah-tengah eksekusi script.
        let sampled_at = started + started.elapsed() / 2;
        parse_spotify_output(&stdout, sampled_at)
    }
}

fn spotify_process_running() -> bool {
    Command::new("/usr/bin/pgrep")
        .args(["-x", "Spotify"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        // Kalau pgrep tidak bisa dijalankan, biarkan AppleScript yang cek.
        .unwrap_or(true)
}

fn run_osascript(script: &str) -> Result<String, String> {
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("gagal menjalankan osascript: {e}"))?;

    let deadline = Instant::now() + SCRIPT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("osascript timeout".into());
            }
            Err(e) => return Err(format!("osascript: {e}")),
        }
    };

    let mut stdout = String::new();
    let mut stderr = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut stdout);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut stderr);
    }
    if !status.success() {
        let hint = if stderr.contains("-1743") {
            " (izin Automation untuk Spotify ditolak: System Settings → Privacy & Security → Automation)"
        } else {
            ""
        };
        return Err(format!(
            "osascript gagal ({status}): {}{hint}",
            stderr.trim()
        ));
    }
    Ok(stdout)
}
