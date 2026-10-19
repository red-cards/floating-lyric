# MiniLyrics (floating-lyric)

Jendela **lirik tersinkron yang melayang** di atas semua window — ala MiniLyrics
jaman dulu — buat karaokean sambil ngoding. Ditulis **murni Rust** (eframe/egui,
tanpa Node/web), jalan di **macOS** dan **Windows**.

- Baca lagu yang sedang diputar (Spotify di macOS; semua player di media
  overlay Windows, termasuk Spotify Web/YouTube Music di browser).
- Ambil lirik sinkron dari [LRCLIB](https://lrclib.net) (gratis, tanpa API key).
- Tampil 3 baris dengan efek karaoke kuning, always on top, tidak mencuri fokus
  editor, bisa click-through, atur offset dari tray.

## Cara pakai tanpa build (dari artifact CI)

1. Buka tab **Actions** → pilih run terbaru dari branch/PR yang mau dites →
   bagian **Artifacts**.
2. Download sesuai OS:
   - **Windows:** `mini-lyrics-windows-<sha>.zip` → ekstrak → jalankan
     `mini-lyrics.exe`. Portable: tanpa installer, tanpa hak admin, tanpa runtime
     tambahan.
     SmartScreen akan muncul karena exe tidak ditandatangani:
     klik **More info → Run anyway**.
   - **macOS:** `mini-lyrics-macos-<sha>.zip` (di dalamnya ada zip lagi berisi
     `MiniLyrics.app`, binary universal Apple Silicon + Intel). Ekstrak, pindahkan
     ke `/Applications` (opsional), lalu hapus atribut karantina:

     ```sh
     xattr -dr com.apple.quarantine MiniLyrics.app
     open MiniLyrics.app
     ```

     Saat pertama membaca Spotify, macOS menanyakan izin **Automation**
     ("MiniLyrics ingin mengontrol Spotify") → **OK**. Kalau terlanjur ditolak:
     System Settings → Privacy & Security → Automation → MiniLyrics → centang
     Spotify.
3. Putar lagu. Ikon not kuning muncul di tray (Windows) / menu bar (macOS) —
   semua pengaturan ada di menunya.

> **Windows:** semua player yang muncul di media overlay Windows ikut terbaca,
> termasuk Spotify Web/YouTube Music di Chrome/Edge. Prioritas: Spotify desktop
> → session lain yang sedang Playing → session aktif Windows.

### Menu tray

| Menu | Fungsi |
|---|---|
| `MiniLyrics v… (sha)` | Versi + commit build (untuk laporan bug) |
| Sembunyikan/Tampilkan lirik | Window tetap ada tapi tidak menggambar apa pun & tembus klik |
| Click-through | Klik tembus ke window di bawahnya (tidak bisa di-drag) |
| Lirik lebih cepat / lebih lambat 0,5 dtk, Reset offset | Koreksi sinkron |
| Mode debug | Overlay info: sumber, posisi mentah/interpolasi, offset, status & latency LRCLIB, umur sampel |
| Buka folder log | Buka folder log di Explorer/Finder |
| Keluar | |

Window bisa dipindah dengan drag (tombol kiri) di panel lirik. Posisi, offset,
click-through, dan mode debug tersimpan otomatis.

### Lokasi file

| | Windows | macOS |
|---|---|---|
| Setting | `%APPDATA%\minilyrics\MiniLyrics\config\settings.json` | `~/Library/Application Support/dev.minilyrics.MiniLyrics/settings.json` |
| Log | `%LOCALAPPDATA%\minilyrics\MiniLyrics\data\logs\` | `~/Library/Application Support/dev.minilyrics.MiniLyrics/logs/` |

Log dirotasi otomatis (`mini-lyrics.log` maks ~1 MB, lalu jadi
`mini-lyrics.old.log`). Error jaringan dicatat lengkap (berguna kalau kena proxy
atau firewall kantor).

### Jaringan kantor / proxy

- Proxy dibaca dari env `HTTPS_PROXY` / `HTTP_PROXY` / `NO_PROXY`.
- Sertifikat dipercaya dari root bawaan **dan** certificate store OS, jadi proxy
  kantor yang memasang root CA sendiri tetap jalan.
- Kalau server lirik tidak bisa dihubungi, tampil "Gagal konek ke server lirik"
  dan dicoba ulang otomatis tiap 30 detik (tidak di-cache sebagai "tidak ketemu").
- Cek koneksi dari terminal (butuh toolchain Rust):
  `cargo run -p mini-lyrics-core --example fetch -- "Judul" "Artis"`

## Build lokal

### Syarat

- [rustup](https://rustup.rs) (toolchain stable).
- **macOS:** Xcode Command Line Tools (`xcode-select --install`).
- **Windows:** Visual Studio Build Tools dengan workload **Desktop development
  with C++** (MSVC + Windows SDK).

### Perintah

```sh
cargo run -p mini-lyrics                 # jalankan (debug)
MINI_LYRICS_DEMO=1 cargo run -p mini-lyrics   # mode demo: lagu & lirik palsu, tanpa Spotify/jaringan
cargo test --workspace                   # semua test (core juga jalan di Linux)
cargo test -p mini-lyrics-core           # test logika inti saja
cargo build --release -p mini-lyrics     # binary rilis di target/release/

# macOS: susun MiniLyrics.app (+ codesign ad-hoc) ke dist/
scripts/bundle-macos.sh                  # pakai target/release/mini-lyrics
```

Windows PowerShell untuk mode demo: `$env:MINI_LYRICS_DEMO=1; cargo run -p mini-lyrics`.

Commit pendek yang tampil di tray disuntikkan oleh `crates/app/build.rs`
(`GITHUB_SHA` di CI, atau `git rev-parse --short HEAD` di lokal).

## Struktur

| Path | Isi |
|---|---|
| `crates/core` | Lib tanpa GUI/OS: `model` (PlayerState), `lrc` (parser LRC), `lrclib` (klien + pembersihan judul + pilih durasi terdekat), `controller` (cache, fetch di thread, retry 30 dtk, buang hasil basi), `karaoke` (baris aktif & progress), `locale` (angka `12,5`), `applescript` (parser output Spotify), `wintime` (TimeSpan/FILETIME) |
| `crates/app/src/main.rs` | Setup window (ViewportBuilder), logging, error fatal |
| `crates/app/src/ui.rs` | UI egui: panel, 3 baris, karaoke, status, drag, overlay debug |
| `crates/app/src/now_playing/` | Thread poller 1 dtk; `macos.rs` (osascript), `windows.rs` (SMTC), `demo.rs` |
| `crates/app/src/tray.rs` | Ikon (RGBA dari kode) & menu tray |
| `crates/app/src/os_window.rs` | NSWindow (macOS) / extended style HWND (Windows) |
| `crates/app/src/fonts.rs` | Font fallback sistem untuk CJK & simbol |
| `crates/app/src/settings.rs`, `logging.rs` | Setting JSON, log berotasi |
| `scripts/bundle-macos.sh` | Susun `MiniLyrics.app` + Info.plist + codesign ad-hoc |
| `.github/workflows/ci.yml` | CI + artifact |
| `TESTING.md` | Checklist tes manual macOS & Windows |

### Alur data

```
thread now-playing (1 dtk) ──► Arc<Mutex<NowPlaying>> ──┐
                                                         ├─► UI (egui, tiap frame)
thread fetch lirik ◄── job ── LyricsController ◄─────────┘
        └── hasil ──► channel ──► LyricsController ──► UI
```

UI tidak pernah blocking: posisi dihitung tiap frame
(`posisi sampel + waktu sejak sampel (kalau playing) + offset`), repaint ~16 ms
saat playing dan ~250 ms saat idle.

## Known issues

- **Belum dites di hardware nyata oleh CI** — CI hanya membuktikan build & unit
  test; perilaku window/tray perlu dicek manual (lihat `TESTING.md`).
- **macOS – app full-screen:** window diset `NSStatusWindowLevel` +
  `canJoinAllSpaces | fullScreenAuxiliary | stationary | ignoresCycle`. Kalau di
  macOS versi tertentu tetap tidak muncul di atas app full-screen, catat di
  issue (solusi lanjutan: ganti ke `NSPanel` non-activating).
- **macOS – fokus:** app memakai activation policy Accessory (tanpa ikon Dock),
  tapi klik/drag di panel bisa mengaktifkan MiniLyrics sesaat sehingga editor
  kehilangan fokus. Pakai click-through kalau mengganggu.
- **macOS – hanya Spotify desktop** yang dibaca (Apple Music/browser belum).
- **Windows – Spotify & posisi:** Spotify jarang memperbarui posisi di SMTC, jadi
  posisi dihitung dari `LastUpdatedTime`. Kalau resume dari pause tanpa update
  timeline, app memakai patokan posisi saat pause (akurasi ±0,5 dtk). Pakai
  offset kalau perlu.
- **Windows – OpenGL:** renderer memakai OpenGL (glow). Di VM/Remote Desktop tanpa
  driver OpenGL app bisa gagal start; pesan error muncul di dialog dan di log.
- Posisi tersimpan tidak divalidasi terhadap monitor yang terpasang; kalau
  monitor dilepas dan window hilang, hapus `window_pos` di `settings.json`.
- Font tebal disimulasikan (teks digambar dua kali) karena egui tidak punya
  varian bold untuk font default.

## Roadmap

- Apple Music (dan player lain) di macOS
- Atur font, ukuran, dan warna
- Cache lirik ke disk
- Shortcut global (hide/show, offset)
- Pilih sumber player secara manual

## Lisensi

MIT
