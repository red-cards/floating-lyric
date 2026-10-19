//! Menyuntikkan commit pendek ke binary supaya tampil di menu tray.
//!
//! Urutan: `MINI_LYRICS_COMMIT_SHA` (diset workflow CI ke head commit PR),
//! `GITHUB_SHA`, lalu `git rev-parse --short HEAD` untuk build lokal.

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=MINI_LYRICS_COMMIT_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");

    let from_env = ["MINI_LYRICS_COMMIT_SHA", "GITHUB_SHA"]
        .iter()
        .filter_map(|k| std::env::var(k).ok())
        .find(|v| !v.trim().is_empty());
    let sha = from_env
        .or_else(|| {
            Command::new("git")
                .args(["rev-parse", "--short", "HEAD"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .and_then(|o| String::from_utf8(o.stdout).ok())
        })
        .map(|s| s.trim().chars().take(7).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "dev".to_string());
    println!("cargo:rustc-env=MINI_LYRICS_COMMIT={sha}");
}
