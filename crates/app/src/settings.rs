//! Setting yang disimpan sebagai JSON di direktori config.

use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Mode tampilan lirik.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayMode {
    /// Panel 3 baris yang bisa di-drag (default: tengah bawah).
    #[default]
    Panel,
    /// Satu baris di bawah notch (atau pill di tengah atas).
    Notch,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub display_mode: DisplayMode,
    /// Posisi pojok kiri atas window (koordinat logis). `None` = default
    /// (tengah bawah monitor utama).
    pub window_pos: Option<[f32; 2]>,
    /// Offset lirik dalam detik. Positif = lirik tampil lebih cepat.
    pub offset_secs: f64,
    /// Click-through untuk mode panel.
    pub click_through: bool,
    /// Click-through untuk mode notch (default aktif).
    pub notch_click_through: bool,
    pub debug: bool,
    pub hidden: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            display_mode: DisplayMode::Panel,
            window_pos: None,
            offset_secs: 0.0,
            click_through: false,
            notch_click_through: true,
            debug: false,
            hidden: false,
        }
    }
}

/// Lokasi folder app.
#[derive(Debug, Clone)]
pub struct Paths {
    pub config_file: PathBuf,
    pub log_dir: PathBuf,
}

impl Paths {
    pub fn resolve() -> Self {
        match ProjectDirs::from("dev", "minilyrics", "MiniLyrics") {
            Some(dirs) => Self {
                config_file: dirs.config_dir().join("settings.json"),
                log_dir: dirs.data_local_dir().join("logs"),
            },
            None => {
                // Hampir mustahil, tapi jangan crash: pakai folder sementara.
                let tmp = std::env::temp_dir().join("MiniLyrics");
                Self {
                    config_file: tmp.join("settings.json"),
                    log_dir: tmp.join("logs"),
                }
            }
        }
    }
}

impl Settings {
    /// Click-through untuk mode yang sedang aktif.
    pub fn click_through(&self) -> bool {
        match self.display_mode {
            DisplayMode::Panel => self.click_through,
            DisplayMode::Notch => self.notch_click_through,
        }
    }

    pub fn toggle_click_through(&mut self) {
        match self.display_mode {
            DisplayMode::Panel => self.click_through = !self.click_through,
            DisplayMode::Notch => self.notch_click_through = !self.notch_click_through,
        }
    }

    pub fn load(path: &Path) -> Self {
        match fs::read_to_string(path) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(s) => s,
                Err(e) => {
                    log::warn!("settings.json rusak, pakai default: {e}");
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &Path) {
        let result = (|| -> Result<(), String> {
            if let Some(dir) = path.parent() {
                fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
            // Tulis ke file sementara lalu rename supaya tidak setengah jadi.
            let tmp = path.with_extension("json.tmp");
            fs::write(&tmp, json).map_err(|e| e.to_string())?;
            fs::rename(&tmp, path).map_err(|e| e.to_string())
        })();
        match result {
            Ok(()) => log::debug!("setting disimpan ke {}", path.display()),
            Err(e) => log::warn!("gagal simpan setting ke {}: {e}", path.display()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_tolerates_missing_fields() {
        let dir = std::env::temp_dir().join(format!("ml-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let s = Settings {
            display_mode: DisplayMode::Notch,
            window_pos: Some([10.0, 20.0]),
            offset_secs: -0.5,
            click_through: true,
            notch_click_through: false,
            debug: true,
            hidden: false,
        };
        s.save(&path);
        assert_eq!(Settings::load(&path), s);

        fs::write(&path, r#"{"offset_secs": 1.5}"#).unwrap();
        let partial = Settings::load(&path);
        assert_eq!(partial.offset_secs, 1.5);
        assert!(!partial.click_through);
        assert_eq!(partial.display_mode, DisplayMode::Panel);
        assert!(partial.notch_click_through, "notch default click-through");

        fs::write(&path, "bukan json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn click_through_is_per_mode() {
        let mut s = Settings::default();
        assert!(!s.click_through());
        s.display_mode = DisplayMode::Notch;
        assert!(s.click_through());
        s.toggle_click_through();
        assert!(!s.notch_click_through);
        assert!(!s.click_through, "mode panel tidak ikut berubah");
        assert_eq!(
            serde_json::to_string(&DisplayMode::Notch).unwrap(),
            "\"notch\""
        );
    }
}
