# Build lokal

Panduan lengkap build MiniLyrics dari source di **macOS** dan **Windows**.
Cuma mau pakai? Download artifact dari tab Actions saja (lihat
[README](README.md#cara-pakai-tanpa-build-dari-artifact-ci)) — tidak perlu
toolchain apa pun.

- [1. Pasang toolchain](#1-pasang-toolchain)
- [2. Ambil source](#2-ambil-source)
- [3. Jalankan (debug)](#3-jalankan-debug)
- [4. Test & lint](#4-test--lint)
- [5. Build rilis](#5-build-rilis)
- [6. Update](#6-update)
- [Troubleshooting](#troubleshooting)

Butuh **Rust stable ≥ 1.95** (dibutuhkan eframe 0.37). Build pertama
mengunduh & mengompilasi ±300 crate (5–10 menit); build berikutnya jauh lebih
cepat.

## 1. Pasang toolchain

### macOS

```sh
# Compiler C, linker, git (sekali saja)
xcode-select --install

# Rust (tanpa sudo, terpasang di ~/.cargo)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

rustc --version   # harus ≥ 1.95
```

Mau bikin `.app` universal (Apple Silicon + Intel) seperti CI? Tambahkan target:

```sh
rustup target add aarch64-apple-darwin x86_64-apple-darwin
```

### Windows

Butuh dua hal: **rustup** dan **MSVC (Visual Studio Build Tools)**.

1. **rustup** — tanpa hak admin. Download `rustup-init.exe` dari
   <https://rustup.rs> lalu jalankan (pilihan default, terpasang di
   `%USERPROFILE%\.cargo`). Atau lewat winget:

   ```powershell
   winget install Rustlang.Rustup
   ```

2. **Visual Studio Build Tools** dengan workload **Desktop development with
   C++** (MSVC + Windows SDK). Installer ini **butuh hak admin**:

   ```powershell
   winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
   ```

   Atau manual dari <https://visualstudio.microsoft.com/visual-cpp-build-tools/>
   → centang **Desktop development with C++**.

3. Buka terminal **baru** (supaya PATH terbaru terbaca), lalu cek:

   ```powershell
   rustc --version          # harus ≥ 1.95
   rustup show              # default host: x86_64-pc-windows-msvc
   ```

> **Laptop kantor tanpa hak admin:** Build Tools tidak bisa dipasang tanpa
> admin, jadi build lokal biasanya tidak memungkinkan. Pakai artifact CI
> (`mini-lyrics-windows-<sha>.zip`) — itu `.exe` yang sama dengan hasil
> `cargo build --release`.

## 2. Ambil source

```sh
git clone https://github.com/red-cards/floating-lyric.git
cd floating-lyric
```

## 3. Jalankan (debug)

```sh
cargo run -p mini-lyrics
```

Window lirik muncul di tengah bawah layar dan ikon not kuning muncul di
tray/menu bar. Build debug menulis log level **Debug** (lebih detail daripada
rilis) ke folder log — buka lewat tray → **Buka folder log**. Di Windows, build
debug ikut membuka jendela console kalau dijalankan dari Explorer; build rilis
tidak.

Variabel env yang berguna saat development:

| Variabel | Fungsi |
|---|---|
| `MINI_LYRICS_DEMO=1` | Lagu & lirik palsu yang berulang: cek UI tanpa Spotify/jaringan |
| `MINI_LYRICS_FAKE_NOTCH=185x32` | Paksa bentuk notch (lebar x tinggi, points) di mode Notch, untuk layar tanpa notch/Windows |
| `HTTPS_PROXY=http://host:port` | Proxy untuk request LRCLIB |

macOS / Linux (bash/zsh):

```sh
MINI_LYRICS_DEMO=1 cargo run -p mini-lyrics
MINI_LYRICS_DEMO=1 MINI_LYRICS_FAKE_NOTCH=185x32 cargo run -p mini-lyrics
```

Windows PowerShell (berlaku untuk sesi terminal itu):

```powershell
$env:MINI_LYRICS_DEMO = "1"; cargo run -p mini-lyrics
$env:MINI_LYRICS_FAKE_NOTCH = "185x32"; cargo run -p mini-lyrics
Remove-Item Env:MINI_LYRICS_DEMO, Env:MINI_LYRICS_FAKE_NOTCH   # matikan lagi
```

**macOS – izin Automation:** saat dijalankan lewat `cargo run`, prompt izin
mengontrol Spotify ditujukan ke aplikasi terminal-mu (Terminal/iTerm/VS Code),
bukan ke MiniLyrics. Izinkan di System Settings → Privacy & Security →
Automation → (terminal) → Spotify.

Cek koneksi ke LRCLIB tanpa membuka app:

```sh
cargo run -p mini-lyrics-core --example fetch -- "Judul" "Artis" ["Album"] [durasi-detik]
```

## 4. Test & lint

Sama dengan yang dijalankan CI — jalankan sebelum push:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`crates/core` tidak bergantung pada GUI/OS, jadi `cargo test -p mini-lyrics-core`
juga jalan di Linux. Crate app ikut terkompilasi di Linux (tanpa tray &
now-playing), cukup untuk mencoba UI dengan `MINI_LYRICS_DEMO=1`; di Linux
butuh paket dev X11/Wayland (mis. `libxkbcommon-x11-0`).

## 5. Build rilis

```sh
cargo build --release -p mini-lyrics
```

| OS | Hasil |
|---|---|
| Windows | `target\release\mini-lyrics.exe` — portable, bisa langsung disalin ke komputer lain |
| macOS | `target/release/mini-lyrics` — binary polos; bungkus jadi `.app` (di bawah) supaya tanpa ikon Dock dan izin Automation tertuju ke MiniLyrics |

Commit pendek yang tampil di menu tray diambil otomatis dari
`git rev-parse --short HEAD` oleh `crates/app/build.rs` (di CI dari
`GITHUB_SHA`).

### macOS: bikin `MiniLyrics.app`

```sh
cargo build --release -p mini-lyrics
scripts/bundle-macos.sh                 # → dist/MiniLyrics.app (codesign ad-hoc)
open dist/MiniLyrics.app
```

Universal binary (Apple Silicon + Intel), sama seperti CI:

```sh
cargo build --release -p mini-lyrics --target aarch64-apple-darwin
cargo build --release -p mini-lyrics --target x86_64-apple-darwin
lipo -create -output target/mini-lyrics-universal \
  target/aarch64-apple-darwin/release/mini-lyrics \
  target/x86_64-apple-darwin/release/mini-lyrics
scripts/bundle-macos.sh target/mini-lyrics-universal dist
```

Format: `scripts/bundle-macos.sh [binary] [folder-output]`. Bundle hasil build
lokal tidak kena karantina, jadi tidak perlu `xattr`. Setelah rebuild,
izin Automation bisa ditanyakan ulang karena signature ad-hoc berubah.

## 6. Update

```sh
git pull
cargo build --release -p mini-lyrics    # (+ scripts/bundle-macos.sh di macOS)
```

Setting & posisi window tersimpan di luar folder repo, jadi tidak hilang saat
rebuild (lokasinya di README → *Lokasi file*).

## Troubleshooting

| Gejala | Penyebab & solusi |
|---|---|
| `package ... requires rustc 1.95` / `rust-version` | Toolchain lama → `rustup update stable` |
| Windows: `linker 'link.exe' not found` | MSVC belum terpasang → pasang Build Tools + workload C++, buka terminal baru |
| Windows: error `ring` / `cc` / `failed to find tool "cl.exe"` | Sama: Build Tools + Windows SDK. Jangan pakai toolchain `-gnu`; pastikan `rustup default stable-msvc` |
| macOS: `xcrun: error: invalid active developer path` | `xcode-select --install` |
| `cargo` gagal download crate di jaringan kantor | Set proxy: `HTTPS_PROXY=http://proxy:port` (cargo membacanya). Kalau proxy memasang sertifikat sendiri: `CARGO_HTTP_CAINFO=<file .pem root CA kantor>` |
| `failed to run custom build command for ring` saat cross-compile | Build di OS targetnya masing-masing (atau andalkan CI); cross-compile tidak didukung resmi |
| App jalan tapi lirik "Gagal konek ke server lirik" | Cek log (tray → Buka folder log) — pesan error jaringan lengkap tercatat; coba `cargo run -p mini-lyrics-core --example fetch -- "Judul" "Artis"` |
| Windows: app langsung tertutup / dialog "gagal jalan" | Biasanya OpenGL tidak tersedia (VM/Remote Desktop). Detail di log |
| Build terasa rusak setelah ganti toolchain | `cargo clean` lalu build lagi |
