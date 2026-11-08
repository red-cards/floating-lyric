# MiniLyrics untuk Android / Android Auto (eksperimental)

Menampilkan **lirik tersinkron lagu Spotify di card media Android Auto**
(dashboard split-screen / Coolwalk: Maps di kanan, card media di kiri).

Android Auto tidak mengizinkan UI custom atau overlay. Card media hanya
menampilkan metadata `MediaSession` dari media app yang aktif. Karena itu
MiniLyrics menyamar sebagai media app (**"proxy media app"**):

- audio tetap diputar **Spotify**; MiniLyrics tidak memutar suara apa pun,
- session MiniLyrics mencerminkan Spotify (status play/pause, posisi, durasi,
  album art), tapi **title + subtitle diisi baris lirik yang sedang dinyanyikan**,
- semua tombol di card (play/pause, lagu sebelumnya/berikutnya, seek)
  **diteruskan ke Spotify**.

Lirik diambil dari [LRCLIB](https://lrclib.net), sumber yang sama dengan versi
desktop. Hanya untuk pemakaian pribadi (sideload); tidak untuk Play Store.

Status: tahap 1 (prototype dummy) **lolos di mobil asli**. Tahap 2 (dokumen
ini) memakai lirik asli dan perlu dites ulang (lihat [Skenario tes](#skenario-tes)).

## Cara kerja

```
Spotify ──MediaController (getActiveSessions)──► SpotifySource
                                                    │ lagu, PlaybackState
                                                    ▼
LRCLIB ◄── LyricsRepository (cache memori+disk) ◄── LyricsService ──► MediaSession MiniLyrics ──► Android Auto
                                                    ▲                       (title/subtitle = lirik)
            tombol card (play/pause/prev/next/seek) ┘──► transportControls Spotify
```

1. **Membaca Spotify.** `SpotifyNotificationListener` hanya ada supaya app
   boleh memanggil `MediaSessionManager.getActiveSessions()`; isi notifikasi
   tidak dibaca. `SpotifySource` memilih session milik `com.spotify.music`
   (session MiniLyrics sendiri selalu diabaikan) dan mendengarkan
   `onSessionsChanged`, `onMetadataChanged`, `onPlaybackStateChanged`.
2. **Posisi** tidak di-polling, tapi diekstrapolasi dari `PlaybackState` Spotify:
   `position + (elapsedRealtime − lastPositionUpdateTime) × speed`.
3. **Lirik** (port dari `crates/core`): `/api/get` dengan judul, artis, album,
   durasi → kalau tidak ada lirik sinkron, `/api/search` dengan judul yang
   dibersihkan (`- Remastered`, `(feat. …)` dibuang) + artis, pilih durasi
   paling dekat. Parse LRC sama dengan desktop. Fetch di coroutine
   (`Dispatchers.IO`), cache per lagu di memori + disk (`filesDir/lyrics`).
   Error jaringan tidak di-cache dan dicoba ulang tiap 30 dtk.
4. **Card** hanya di-update saat baris berganti: timer dijadwalkan tepat ke
   timestamp baris berikutnya, dan dihitung ulang setiap Spotify play/pause/seek.

### Isi card

Tes di mobil: title dan subtitle masing-masing terpotong sekitar 15–17
karakter. Jadi **satu** baris lirik aktif dipecah dua di batas kata, target
`CardText.CHARS_PER_PART = 16` karakter per bagian (ubah di
`CardText.kt` saja).

| Situasi | Title | Subtitle |
|---|---|---|
| Baris panjang | bagian 1 baris aktif (≤ 16) | sisa baris aktif |
| Baris pendek (≤ 16) | baris aktif | `› ` + awal baris berikutnya |
| Intro (sebelum baris pertama) | judul lagu | `› ` + awal baris pertama |
| Jeda (baris kosong di LRC) | `♪` | `› ` + awal baris berikutnya |
| Sedang mencari lirik | judul lagu | `mencari lirik…` |
| Lirik tidak ketemu / error jaringan | judul lagu | artis |
| Lirik tanpa timestamp | judul lagu | `lirik tidak sinkron` |
| Lagu instrumental | judul lagu | `♪ instrumental` |
| Spotify tidak aktif | `Spotify tidak aktif` | `Putar lagu dulu` |
| Izin belum diberikan | `Izin belum aktif` | `Buka app MiniLyrics` |

Teks tanpa spasi (mis. Jepang) dipotong paksa di 16 karakter tanpa membelah
emoji. Artwork card = album art Spotify (diperkecil ke 320 px).

### Tombol & auto-resume

- Play/pause/prev/next/seek dari card → `transportControls` Spotify.
  Status play/pause MiniLyrics selalu mengikuti Spotify.
- **Pengaman auto-resume:** kalau dalam **3 detik** setelah MiniLyrics di-play,
  Spotify berubah ke PAUSED **tanpa** perintah pause dari user, MiniLyrics
  mengirim `play` ke Spotify **sekali**. Log:
  `AUTO-RESUME: Spotify ter-pause 840 ms setelah MiniLyrics di-play, tanpa
  perintah pause dari user → kirim play ke Spotify (sekali)`. Jumlah dan waktu
  kejadian terakhir juga tampil di layar app.

**Replay-10 / forward-10 di tahap 1.** Dokumentasi Android Auto hanya
menyebut bahwa slot kiri/kanan di kontrol diisi `ACTION_SKIP_TO_PREVIOUS` /
`ACTION_SKIP_TO_NEXT`, dan kalau salah satunya tidak didukung slotnya bisa
dipakai tombol lain. Kapan persisnya Android Auto memilih ikon replay-10/
forward-10 tidak terdokumentasi, jadi penyebab pastinya belum terbukti.
Perubahan di tahap 2 untuk memaksa prev/next:

- session extras `android.media.playback.ALWAYS_RESERVE_SPACE_FOR.ACTION_SKIP_TO_NEXT`
  dan `…ACTION_SKIP_TO_PREVIOUS` = `true` (slot dikunci untuk prev/next),
- `ACTION_SKIP_TO_NEXT/PREVIOUS` selalu ada di actions; **tanpa**
  `ACTION_REWIND`/`ACTION_FAST_FORWARD` dan tanpa custom action,
- durasi sekarang durasi lagu asli (prototype memakai durasi 16 dtk).

Kalau di mobil masih muncul replay-10/forward-10, log akan menunjukkan apa
yang dikirim Android Auto saat tombol itu ditekan (`onSkipToNext`,
`onSeekTo`, `onRewind`, `onFastForward`, atau `onCustomAction`). Kirimkan
potongan log itu.

## Isi folder

| Path (`app/src/main/java/dev/minilyrics/android/`) | Isi |
|---|---|
| `LyricsService.kt` | `MediaBrowserServiceCompat` + session: mirror Spotify, susun card, jadwal ganti baris, teruskan tombol, auto-resume, foreground service |
| `MainActivity.kt` | Onboarding izin + status (Spotify, lagu, sumber lirik, isi card, auto-resume) + kontrol + toggle demo (build debug) |
| `CardText.kt` | Pemecah baris lirik ke title/subtitle (`CHARS_PER_PART`) |
| `Status.kt` | Status bersama service → layar app, setelan (mode demo) |
| `player/SpotifySource.kt` | Baca & kendalikan session Spotify |
| `player/SpotifyNotificationListener.kt` | Notification listener kosong (hanya untuk izin) |
| `player/DemoSource.kt` | Player + lirik palsu untuk tes tanpa Spotify |
| `lyrics/Lrc.kt`, `lyrics/Lrclib.kt` | Parser LRC & klien LRCLIB (port dari `crates/core`) |
| `lyrics/LyricsRepository.kt` | Cache memori + disk, fetch di coroutine |

Unit test (`app/src/test/`): parser LRC, matching LRCLIB (kasus yang sama
dengan test Rust), dan pemecah card. `LrclibLiveTest` mengetes ke server LRCLIB
asli dan hanya jalan dengan `LRCLIB_LIVE=1`.

- Package / applicationId: `dev.minilyrics.android`
- minSdk 26, compileSdk/targetSdk 34, Java 17, Kotlin, Gradle Kotlin DSL
- Dependency: `androidx.core:core-ktx`, `androidx.media:media`,
  `kotlinx-coroutines-android`
- Gradle wrapper sendiri (`./gradlew`, Gradle 8.9, AGP 8.5.2)

Project ini berdiri sendiri. Gradle tidak menyentuh `crates/`, dan Cargo
workspace di root tidak memuat `android/`. Kode Rust tidak dipakai langsung
(belum UniFFI); logikanya di-port ke Kotlin.

## Izin

| Izin | Untuk apa |
|---|---|
| **Akses notifikasi** (notification listener) | Wajib. Syarat `getActiveSessions()` untuk membaca session Spotify. Isi notifikasi tidak dibaca. |
| Notifikasi (`POST_NOTIFICATIONS`) | Notifikasi media MiniLyrics saat foreground service jalan |
| Internet | Ambil lirik dari lrclib.net |
| Foreground service (media playback) | Menjaga service tetap hidup saat lagu diputar |

**Memberi izin akses notifikasi:** buka app MiniLyrics → **Buka setelan
akses notifikasi** → aktifkan MiniLyrics. Di **Android 13+**, app hasil
sideload sering diblokir dengan pesan *"Restricted setting"* (tombolnya
abu-abu). Caranya: **Buka Info aplikasi** → menu **⋮** (kanan atas) →
**Allow restricted settings** → konfirmasi PIN/sidik jari → ulangi langkah
izin tadi. Menu ⋮ itu baru muncul setelah kamu mencoba memberi izin sekali.

## Build & install

Syarat:

- **JDK 17+** (paling gampang pakai JDK bawaan Android Studio)
- **Android SDK** dengan platform **API 34** (Android Studio → SDK Manager)
- Lokasi SDK lewat `ANDROID_HOME`, atau file `android/local.properties`
  (`sdk.dir=/Users/<kamu>/Library/Android/sdk`). File ini tidak di-commit.

```sh
cd android
./gradlew assembleDebug                  # → app/build/outputs/apk/debug/app-debug.apk
./gradlew installDebug                   # install ke HP yang tercolok (USB debugging aktif)
./gradlew testDebugUnitTest             # unit test
./gradlew lintDebug                      # Android lint (harus tanpa error)
# atau:
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Bisa juga buka folder `android/` (bukan root repo) di Android Studio.

Lihat log (semua event diberi tag `MiniLyrics`):

```sh
adb logcat -s MiniLyrics
adb shell dumpsys media_session          # session aktif + metadata + state
```

### Tes di HP (tanpa mobil)

1. Buka app **MiniLyrics**, izinkan notifikasi dan akses notifikasi (lihat
   [Izin](#izin)).
2. Putar lagu di Spotify. Layar MiniLyrics menampilkan: Spotify terdeteksi,
   judul lagu, sumber lirik (`LRCLIB` / `cache disk` / `cache memori`), dan isi
   card yang berganti tiap baris lirik.
3. Tombol Play/Pause/Lagu sebelumnya/berikutnya di app lewat jalur yang sama
   dengan Android Auto, jadi harus mengendalikan Spotify.

**Mode demo (tanpa Spotify):** di build debug (termasuk APK dari CI), bagian
**Debug** di layar app punya toggle *Mode demo*. Sumbernya jadi player palsu
dengan lagu & lirik buatan sendiri (baris pendek, panjang, emoji, Jepang, jeda).
Matikan lagi untuk kembali ke Spotify. Build release tidak punya toggle ini.

## APK dari CI (GitHub Actions)

Workflow `.github/workflows/android.yml` jalan setiap ada push (branch apa pun)
yang mengubah `android/**`. Bisa juga dijalankan manual: tab **Actions** →
**Android** → **Run workflow**. Workflow ini menjalankan
`./gradlew assembleDebug testDebugUnitTest lintDebug` lalu meng-upload dua artifact:

- `minilyrics-android-debug`: berisi `app-debug.apk` (di dalam zip artifact)
- `minilyrics-android-tests`: laporan unit test (HTML)
- `minilyrics-android-lint`: laporan lint (HTML, XML, SARIF)

Sidik jari SHA-256 sertifikat APK tercatat di ringkasan run, berguna untuk
memastikan keystore yang dipakai tetap sama.

### Debug keystore yang konsisten

Android hanya mau meng-install APK baru **menimpa** versi lama kalau
sertifikatnya sama. Tanpa keystore tetap, setiap run CI membuat debug keystore
baru, jadi kamu harus uninstall dulu tiap kali (dan log peringatan muncul di
run).

`app/build.gradle.kts` memakai keystore dari env `MINILYRICS_DEBUG_KEYSTORE`
kalau di-set. Kalau tidak, dipakai `~/.android/debug.keystore` bawaan mesin.
CI mengisi env itu dari secret **`DEBUG_KEYSTORE_BASE64`**.

**Opsi 1 (disarankan): pakai debug keystore Mac kamu.** APK dari CI dan dari
`./gradlew installDebug` di Mac jadi bisa saling menimpa. File ini sudah ada
kalau kamu pernah build Android di Mac. Kalau belum ada, cukup build sekali.

```sh
base64 -i ~/.android/debug.keystore | pbcopy     # macOS: salin ke clipboard
```

**Opsi 2: buat keystore baru khusus proyek ini.**

```sh
keytool -genkeypair -v -keystore minilyrics-debug.keystore \
  -storepass android -keypass android -alias androiddebugkey \
  -keyalg RSA -keysize 2048 -validity 10000 \
  -dname "CN=Android Debug,O=Android,C=US"
base64 -i minilyrics-debug.keystore | pbcopy     # macOS
# Linux: base64 -w0 minilyrics-debug.keystore
```

**Windows (PowerShell).** Kalau `keytool` tidak dikenali, pakai yang dibawa
Android Studio: `& "C:\Program Files\Android\Android Studio\jbr\bin\keytool.exe"`.

```powershell
cd $HOME
keytool -genkeypair -v -keystore minilyrics-debug.keystore -storepass android -keypass android -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
[Convert]::ToBase64String([IO.File]::ReadAllBytes("$HOME\minilyrics-debug.keystore")) | Set-Clipboard
# Opsi 1 di Windows: ganti path di atas dengan "$HOME\.android\debug.keystore"
```

Build lokal di Windows dengan keystore itu:
`$env:MINILYRICS_DEBUG_KEYSTORE = "$HOME\minilyrics-debug.keystore"; .\gradlew.bat installDebug`.

Simpan file `.keystore`-nya di luar repo (`*.keystore` sudah di-`.gitignore`).
Untuk build lokal dengan keystore yang sama:
`MINILYRICS_DEBUG_KEYSTORE=/path/ke/minilyrics-debug.keystore ./gradlew installDebug`.

**Masukkan ke secret:** repo di GitHub → **Settings → Secrets and variables →
Actions → New repository secret**. Isi Name dengan `DEBUG_KEYSTORE_BASE64` dan
Secret dengan hasil base64 tadi (paste) → **Add secret**. Setelah itu jalankan
ulang workflow. Peringatan "debug keystore default" hilang dan sidik jari di
ringkasan run sama dengan `keytool -list -v -keystore <file> -storepass android`.

Password dan alias diasumsikan konvensi debug Android (`android` /
`androiddebugkey`). Kalau keystore-mu berbeda, set juga
`MINILYRICS_DEBUG_KEYSTORE_PASSWORD`, `MINILYRICS_DEBUG_KEY_ALIAS`, dan
`MINILYRICS_DEBUG_KEY_PASSWORD`. Ini keystore **debug**, bukan kunci rilis,
tapi tetap jangan di-commit.

Catatan: secret tidak tersedia untuk workflow dari fork, jadi build dari fork
selalu memakai keystore default.

## Mengaktifkan developer mode Android Auto

1. Di HP, buka setting Android Auto. Di kebanyakan HP: **Settings → Connected
   devices → Connection preferences → Android Auto**. Bisa juga lewat app
   Android Auto, kalau ada.
2. Scroll ke bawah, ketuk **Version** sekitar 10 kali sampai muncul dialog
   "Allow development settings?" → **OK**.
3. Menu ⋮ (kanan atas) → **Developer settings**.
4. Centang **Unknown sources**. Tanpa ini, app sideload tidak muncul di
   launcher Android Auto.
5. Putuskan lalu sambungkan ulang ke mobil/DHU. **MiniLyrics** sekarang ada di
   daftar app Android Auto.

## Tes dengan Desktop Head Unit (DHU) dari Mac

1. Android Studio → **SDK Manager → SDK Tools** → centang **Android Auto
   Desktop Head Unit Emulator** → Apply. DHU terpasang di
   `$ANDROID_HOME/extras/google/auto/`.
2. Di HP: Android Auto → ⋮ → **Start head unit server**.
3. Colok HP ke Mac via USB, lalu:

   ```sh
   adb forward tcp:5277 tcp:5277
   cd ~/Library/Android/sdk/extras/google/auto
   ./desktop-head-unit
   ```

   Kalau macOS memblokir binary-nya (Gatekeeper):
   `xattr -dr com.apple.quarantine ~/Library/Android/sdk/extras/google/auto`.
4. Untuk layout split-screen (Maps + card media), jalankan DHU dengan
   config layar lebar. Lihat file `.ini` di folder `config/` DHU, contohnya
   `./desktop-head-unit -c config/<file-layar-lebar>.ini`. Nama file berbeda
   antar versi DHU. Bisa juga atur ukuran layar lewat `--help`.
5. Putar lagu di Spotify, lalu di DHU: launcher → **MiniLyrics** →
   **Tampilkan lirik (Spotify)**.
   Bisa juga lewat suara: "Hey Google, putar MiniLyrics". Perintah ini masuk ke
   `onPlayFromSearch`, dan query-nya diabaikan.

Tetap jalankan `adb logcat -s MiniLyrics` di terminal lain selama tes.

## Skenario tes

Catat hasilnya (foto layar mobil/DHU + potongan `adb logcat -s MiniLyrics`) di
PR atau issue.

| | Langkah | Yang diamati | Hasil |
|---|---|---|---|
| **A** | Spotify memutar lagu populer. Buka MiniLyrics di Android Auto | Card berganti **tepat** saat baris dinyanyikan? Pemecahan title/subtitle terbaca (≤ 16 karakter per bagian)? Artwork = album art? | ☐ |
| **B** ⭐ | Spotify memutar, lalu di Android Auto pilih MiniLyrics → play | Spotify tetap bunyi? Kalau sempat berhenti, apakah log `AUTO-RESUME` muncul dan Spotify lanjut sendiri? | ☐ |
| **C** | Tombol di card | Ikon kiri/kanan sekarang **prev/next lagu** (bukan replay-10/forward-10)? Next/prev/play/pause/seek mengendalikan Spotify? Kalau tidak, catat callback yang muncul di log | ☐ |
| **D** | Biarkan 2–3 lagu lewat; next di Spotify dari HP | Card tetap milik MiniLyrics dan lirik ikut ganti lagu? | ☐ |
| **E** | Seek di Spotify (HP) dan pause/play | Baris lirik ikut melompat ≤ ~1 dtk? | ☐ |
| **F** | Lagu tanpa lirik / lagu instrumental / mode pesawat | Card: judul + artis (atau `lirik tidak sinkron` / `♪ instrumental`), lalu lirik muncul sendiri setelah internet kembali (retry 30 dtk) | ☐ |
| **G** | Matikan Spotify (force stop) | Card: `Spotify tidak aktif` / `Putar lagu dulu`; kembali normal setelah Spotify diputar lagi | ☐ |

## Batasan yang diketahui

- **Hanya Spotify** (`com.spotify.music`). Player lain diabaikan.
- **Dua notifikasi media di HP** (Spotify + MiniLyrics) selama lagu diputar,
  karena MiniLyrics perlu foreground service dan session aktif.
- `startForeground` bisa ditolak Android 12+/14 kalau dipicu dari background
  (mis. Spotify mulai diputar dari HP saat app MiniLyrics tidak terbuka). Log:
  `startForeground gagal: …`. Service tetap jalan selama Android Auto atau
  layar app masih ter-bind.
- **Auto-resume tidak bisa membedakan** pause yang dipicu Android Auto dengan
  pause yang kamu tekan **langsung di app Spotify** dalam 3 detik pertama.
  Pause lewat card/app MiniLyrics tetap dikenali sebagai pause dari user.
- Penyebab pasti tombol replay-10/forward-10 belum terbukti (lihat
  [Tombol & auto-resume](#tombol--auto-resume)).
- Lirik tanpa timestamp hanya diberi label `lirik tidak sinkron`, isinya
  tidak ditampilkan.
- Belum ada fitur desktop "Lirik lain" (pilih kandidat LRCLIB lain) dan offset
  sinkron.
- Iklan Spotify diperlakukan sebagai lagu biasa (biasanya "tidak ketemu" →
  judul + artis).
- Tidak ada signing release. Pakai build debug untuk sideload.

## Tahap berikutnya (ide)

- Offset sinkron dan "Lirik lain" seperti di desktop.
- Pertimbangkan UniFFI kalau logika lirik di desktop dan Android mulai sering
  berubah bersamaan. Saat ini modul yang di-port: `lrc.rs` (`parse_lrc`),
  `lrclib.rs` (`clean_title`, `pick_best`, alur `fetch`), `karaoke.rs`
  (`active_index`). `controller.rs` (cache, retry, buang hasil basi) ditulis
  ulang dalam bentuk coroutine.
