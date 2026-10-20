# Checklist tes manual

Tes dilakukan paralel: **macOS** (rumah) dan **Windows** (laptop kantor, tanpa
hak admin). Pakai artifact CI dari PR yang sedang dites (lihat README → "Cara
pakai tanpa build"), lalu centang hasilnya di PR.

Tips umum:
- Nyalakan **Mode debug** dari tray untuk melihat sumber, posisi mentah vs
  interpolasi, offset, status & latency LRCLIB, dan umur sampel.
- Kalau ada yang aneh, lampirkan isi `mini-lyrics.log` (tray → **Buka folder
  log**) dan teks versi/commit di baris pertama menu tray.
- `MINI_LYRICS_DEMO=1` menjalankan lagu & lirik palsu untuk cek UI tanpa
  Spotify/jaringan (termasuk baris Jepang/Korea/Mandarin dan satu baris
  sangat panjang). Untuk state lain: `MINI_LYRICS_DEMO=paused`, `loading`,
  `notfound`, `error`, `idle`.

## Persiapan

| | macOS | Windows |
|---|---|---|
| App terbuka tanpa error (macOS: setelah `xattr -dr com.apple.quarantine`; Windows: SmartScreen → Run anyway) | ☐ | ☐ |
| Tidak ada ikon di Dock (macOS) / taskbar (Windows) | ☐ | ☐ |
| Ikon tray (kotak + dua garis) muncul di menu bar / tray; Windows: warnanya kontras dengan taskbar terang/gelap | ☐ | ☐ |
| Label versi + commit di menu tray sesuai PR | ☐ | ☐ |
| macOS: prompt izin Automation untuk Spotify muncul & diizinkan | ☐ | – |
| Tanpa lagu diputar: tampil "Putar lagu dulu ♪" | ☐ | ☐ |

## Pemutaran

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Putar lagu populer | "Nyari lirik…" lalu lirik muncul; baris aktif terisi putih penuh dari kiri seirama | ☐ | ☐ |
| Ganti lagu (next/prev) | Lirik berganti ≤ ~1–2 dtk; tidak pernah menampilkan lirik lagu sebelumnya | ☐ | ☐ |
| Kembali ke lagu sebelumnya | Lirik langsung muncul (cache), tanpa "Nyari lirik…" lama | ☐ | ☐ |
| Seek maju | Baris lompat ke posisi baru ≤ ~1 dtk | ☐ | ☐ |
| Seek mundur | Sama seperti di atas | ☐ | ☐ |
| Pause | Seluruh panel meredup (±45%), tanda ❚❚ di kiri, lirik berhenti bergerak | ☐ | ☐ |
| Resume setelah pause lama (> 30 dtk) | Lanjut dari posisi yang benar (tidak loncat ke depan) | ☐ | ☐ |
| Lagu tanpa lirik sinkron (mis. lagu indie/instrumental) | "Lirik sinkron nggak ketemu" + judul — artis | ☐ | ☐ |
| Judul dengan "- Remastered"/"(feat. …)" | Tetap ketemu lewat fallback search | ☐ | ☐ |
| Lagu berlirik Jepang/Korea/Mandarin | Huruf tampil benar (bukan kotak) | ☐ | ☐ |
| Windows: Spotify Web / YouTube Music di Chrome/Edge (Spotify desktop ditutup) | Terbaca; `src` di debug = Chrome/MSEdge | – | ☐ |
| Windows: Spotify desktop + YouTube di browser bersamaan | Spotify desktop diprioritaskan | – | ☐ |

## Jaringan

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Offline (Wi-Fi mati) lalu putar lagu baru | "Gagal konek ke server lirik"; app tidak crash | ☐ | ☐ |
| Online lagi | Dalam ≤ 30 dtk lirik muncul sendiri (retry otomatis) | ☐ | ☐ |
| Jaringan kantor / lrclib.net diblok firewall | Status error jaringan; pesan error lengkap ada di log | ☐ | ☐ |
| Pakai proxy (`HTTPS_PROXY`) | Lirik tetap ketemu; env proxy tercatat di log (tanpa password) | ☐ | ☐ |

## Window

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Posisi awal (tanpa setting) | Tengah bawah monitor utama | ☐ | ☐ |
| Drag panel | Window pindah; editor tidak kehilangan fokus (Windows) | ☐ | ☐ |
| Kursor di atas panel (click-through mati) | Muncul `− Sinkron +` (kiri atas) dan `Lirik lain` (kanan atas); hilang lagi saat kursor keluar | ☐ | ☐ |
| Klik `+` / `−` di panel | Offset berubah 0,5 dtk, label menunjukkan offset, menu tray ikut berubah; klik label = reset | ☐ | ☐ |
| Klik `Lirik lain` pada lagu yang liriknya salah | "Nyari lirik lain…" lalu lirik berganti; `‹ 2/N ›` untuk pindah antar kandidat, bisa kembali ke hasil awal | ☐ | ☐ |
| `Lirik lain` saat "Lirik sinkron nggak ketemu" | Kalau ada kandidat, lirik tampil; kalau tidak, "Nggak ada lirik lain" | ☐ | ☐ |
| Tray → Coba lirik lain (mis. mode Notch / click-through) | Sama dengan tombol `Lirik lain` | ☐ | ☐ |
| Klik tombol panel | Tidak ikut men-drag window | ☐ | ☐ |
| Restart app | Posisi, offset, click-through, debug tetap seperti sebelumnya | ☐ | ☐ |
| Mengetik di editor sambil lirik jalan | Fokus tetap di editor | ☐ | ☐ |
| App lain full-screen (VS Code/browser full-screen, video) | Lirik tetap di atas (kalau tidak: catat sebagai known issue) | ☐ | ☐ |
| Pindah Space/desktop virtual | Lirik ikut di semua Space/desktop | ☐ | ☐ |
| Multi monitor: drag ke monitor kedua, restart | Muncul lagi di monitor kedua | ☐ | ☐ |
| Windows: Alt+Tab | MiniLyrics tidak muncul di daftar | – | ☐ |
| macOS: Cmd+Tab | MiniLyrics tidak muncul di daftar | ☐ | – |
| Latar transparan | Hanya kotak rounded semi-transparan yang terlihat, sisanya tembus pandang | ☐ | ☐ |

## Mode Notch

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Tray → Tampilan: Notch | Centang pindah (seperti radio); window pindah ke tengah atas | ☐ | ☐ |
| Mac ber-notch, tidak ada lagu / pause | Bentuk hitam menyatu dengan notch kamera (tidak terlihat beda) | ☐ | – |
| Mac ber-notch, lagu diputar | Bentuk memanjang ke bawah dengan animasi; 1 baris lirik di bawah area kamera, area kamera kosong | ☐ | – |
| Sudut bentuk notch | Sudut atas melengkung ke luar (inverse fillet) menyatu dengan tepi atas, sudut bawah membulat | ☐ | – |
| Posisi di atas menu bar | Bentuk menempel ke tepi atas layar, tidak terdorong ke bawah menu bar (cek log kalau ada `di-constrain`) | ☐ | – |
| Layar tanpa notch / Windows | Pill hitam kecil di tengah atas; memanjang saat lagu diputar | ☐ | ☐ |
| Pause → play → pause | Collapsed/expanded dengan animasi tinggi + fade teks | ☐ | ☐ |
| Click-through default | Langsung aktif di mode Notch; klik menu bar/tab di bawahnya tembus | ☐ | ☐ |
| Kursor masuk area panel | Panel & teks meredup; keluar → normal lagi | ☐ | ☐ |
| Mac + monitor eksternal (tanpa notch) | Tetap tampil di layar MacBook (yang ber-notch) | ☐ | – |
| Clamshell / hanya monitor eksternal | Pill di layar utama | ☐ | – |
| Windows multi monitor | Pill di monitor utama | – | ☐ |
| Mode debug di mode Notch | Overlay muncul di bawah bentuk | ☐ | ☐ |
| Kembali ke Tampilan: Panel bawah | Panel kembali ke posisi tersimpan; click-through panel tidak ikut berubah | ☐ | ☐ |
| Restart app | Mode tampilan tetap seperti sebelumnya | ☐ | ☐ |
| `MINI_LYRICS_FAKE_NOTCH=185x32` (+ `MINI_LYRICS_DEMO=1`) | Bentuk notch tampil di layar tanpa notch | ☐ | ☐ |

## Tray

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Sembunyikan lirik | Tidak ada yang tergambar; klik tembus ke bawah | ☐ | ☐ |
| Tampilkan lirik (setelah disembunyikan lama) | Muncul lagi | ☐ | ☐ |
| Click-through on | Klik di area lirik mengenai window di bawahnya; drag tidak bisa | ☐ | ☐ |
| Click-through off | Bisa drag lagi | ☐ | ☐ |
| Lirik lebih cepat 0,5 dtk ×2 | Label offset "+1,0 dtk"; lirik maju | ☐ | ☐ |
| Lirik lebih lambat 0,5 dtk | Label offset berkurang 0,5 | ☐ | ☐ |
| Reset offset | Label "+0,0 dtk" | ☐ | ☐ |
| Mode debug on/off | Overlay muncul di bawah lirik / hilang; window tidak keluar layar | ☐ | ☐ |
| Buka folder log | Explorer/Finder terbuka di folder log berisi `mini-lyrics.log` | ☐ | ☐ |
| Keluar | App tertutup, ikon tray hilang | ☐ | ☐ |

## Ketahanan

| Skenario | Yang diharapkan | macOS | Windows |
|---|---|---|---|
| Tutup Spotify saat lagu diputar | "Putar lagu dulu ♪"; tidak crash | ☐ | ☐ |
| Buka Spotify lagi | Lirik muncul lagi tanpa restart app | ☐ | ☐ |
| Biarkan jalan > 1 jam | Tidak crash; log tidak lebih dari ~2 MB total | ☐ | ☐ |
| Sleep/wake laptop | Lirik tetap jalan setelah bangun | ☐ | ☐ |
