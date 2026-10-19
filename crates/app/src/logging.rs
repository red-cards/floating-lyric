//! Log ke file dengan rotasi sederhana (maks ~1 MB per file).

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use simplelog::{ConfigBuilder, LevelFilter, WriteLogger};

pub const MAX_LOG_BYTES: u64 = 1024 * 1024;
pub const LOG_FILE: &str = "mini-lyrics.log";
const OLD_LOG_FILE: &str = "mini-lyrics.old.log";

/// File log yang dipindah ke `mini-lyrics.old.log` saat melewati batas
/// ukuran, sehingga total log paling banyak ~2 MB.
struct RotatingFile {
    path: PathBuf,
    old_path: PathBuf,
    file: File,
    written: u64,
}

impl RotatingFile {
    fn open(dir: &Path) -> io::Result<Self> {
        let path = dir.join(LOG_FILE);
        let old_path = dir.join(OLD_LOG_FILE);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        let mut me = Self {
            path,
            old_path,
            file,
            written,
        };
        if me.written >= MAX_LOG_BYTES {
            me.rotate()?;
        }
        Ok(me)
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let _ = fs::remove_file(&self.old_path);
        fs::rename(&self.path, &self.old_path)?;
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.written = 0;
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written + buf.len() as u64 > MAX_LOG_BYTES {
            // Kalau rotasi gagal (mis. file dikunci antivirus), tetap tulis.
            let _ = self.rotate();
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Pasang logger ke file di `dir`. Gagal membuat log tidak boleh membuat app
/// berhenti, jadi error hanya dicetak ke stderr.
pub fn init(dir: &Path) {
    let config = ConfigBuilder::new()
        .set_time_format_rfc3339()
        .set_thread_level(LevelFilter::Off)
        .set_target_level(LevelFilter::Off)
        .add_filter_ignore_str("winit")
        .add_filter_ignore_str("eframe")
        .add_filter_ignore_str("egui")
        .add_filter_ignore_str("epaint")
        .add_filter_ignore_str("reqwest")
        .add_filter_ignore_str("hyper")
        .add_filter_ignore_str("rustls")
        .build();
    let level = if cfg!(debug_assertions) {
        LevelFilter::Debug
    } else {
        LevelFilter::Info
    };
    let result = fs::create_dir_all(dir)
        .and_then(|_| RotatingFile::open(dir))
        .map_err(|e| e.to_string())
        .and_then(|file| WriteLogger::init(level, config, file).map_err(|e| e.to_string()));
    if let Err(e) = result {
        eprintln!("gagal membuat log di {}: {e}", dir.display());
    }

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        log::error!("PANIC: {info}");
        default_hook(info);
    }));
}
