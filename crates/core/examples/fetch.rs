//! Coba ambil lirik dari LRCLIB lewat terminal, berguna untuk cek koneksi
//! (proxy/firewall) tanpa membuka app:
//!
//! ```text
//! cargo run -p mini-lyrics-core --example fetch -- "Judul" "Artis" ["Album"] [durasi]
//! ```

use mini_lyrics_core::lrclib::LrclibClient;
use mini_lyrics_core::{FetchOutcome, TrackQuery};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("pakai: fetch <judul> <artis> [album] [durasi-detik]");
        std::process::exit(2);
    }
    let q = TrackQuery {
        track_key: "cli".into(),
        title: args[0].clone(),
        artist: args[1].clone(),
        album: args.get(2).cloned().unwrap_or_default(),
        duration_secs: args.get(3).and_then(|d| d.parse().ok()).unwrap_or(0.0),
    };
    let client = LrclibClient::new(env!("CARGO_PKG_VERSION")).expect("http client");
    let started = std::time::Instant::now();
    match client.fetch(&q) {
        FetchOutcome::Found(lines) => {
            println!("{} baris ({} ms)", lines.len(), started.elapsed().as_millis());
            for l in lines.iter().take(10) {
                println!("[{:7.2}] {}", l.time_secs, l.text);
            }
        }
        FetchOutcome::NotFound => println!("tidak ada lirik sinkron"),
        FetchOutcome::NetworkError(e) => println!("error jaringan: {e}"),
    }
}
