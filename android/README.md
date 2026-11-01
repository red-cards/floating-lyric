# MiniLyrics untuk Android / Android Auto (eksperimental)

Prototype **tahap 1**: membuktikan apakah lirik bisa tampil di **card media
Android Auto** (dashboard split-screen / Coolwalk) dengan pendekatan
**"proxy media app"**.

Android Auto tidak mengizinkan UI custom atau overlay. Card media hanya
menampilkan metadata `MediaSession` dari media app yang aktif. Karena itu app ini
menyamar sebagai media app. Metadata session-nya diisi baris lirik (judul =
baris sekarang, artis = baris berikutnya), sementara audio tetap diputar Spotify.

Tahap ini **belum** membaca Spotify dan **belum** mengambil lirik asli. Isinya 8
baris dummy bernomor buatan sendiri yang berganti tiap 2 detik, termasuk satu
baris sangat panjang (tes truncation) dan satu baris dengan emoji.

Hanya untuk pemakaian pribadi (sideload). Tidak ada rencana rilis ke Play Store.

## Isi folder

| Path | Isi |
|---|---|
| `app/src/main/java/dev/minilyrics/android/LyricTestService.kt` | `MediaBrowserServiceCompat` + `MediaSessionCompat`: browse tree, transport callback, ganti metadata tiap 2 dtk, foreground service + notifikasi MediaStyle |
| `app/src/main/java/dev/minilyrics/android/MainActivity.kt` | Layar tes tanpa XML layout: minta izin notifikasi, tombol Play/Pause/Prev/Next/Stop lewat `MediaBrowserCompat` |
| `app/src/main/AndroidManifest.xml` | Service `exported` dengan intent-filter `android.media.browse.MediaBrowserService`, `foregroundServiceType="mediaPlayback"`, meta-data `com.google.android.gms.car.application` |
| `app/src/main/res/xml/automotive_app_desc.xml` | `<uses name="media"/>` |

- Package / applicationId: `dev.minilyrics.android` (mengikuti namespace
  `dev.minilyrics.*` yang dipakai bundle macOS)
- minSdk 26, compileSdk/targetSdk 34, Java 17, Kotlin, Gradle Kotlin DSL
- Dependency: `androidx.core:core-ktx`, `androidx.media:media`
- Gradle wrapper sendiri (`./gradlew`, Gradle 8.9, AGP 8.5.2)

Project ini berdiri sendiri. Gradle tidak menyentuh `crates/`, dan Cargo
workspace di root tidak memuat `android/` (member-nya eksplisit:
`crates/core`, `crates/app`).

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

### Tes cepat di HP (tanpa mobil)

1. Buka app **MiniLyrics**, izinkan notifikasi.
2. Tekan **Play**: teks di layar dan notifikasi media berganti tiap 2 detik.
3. Coba Pause / Prev / Next / Stop, dari app maupun dari notifikasi atau lock screen.

## APK dari CI (GitHub Actions)

Workflow `.github/workflows/android.yml` jalan setiap ada push (branch apa pun)
yang mengubah `android/**`. Bisa juga dijalankan manual: tab **Actions** →
**Android** → **Run workflow**. Workflow ini menjalankan
`./gradlew assembleDebug lintDebug` lalu meng-upload dua artifact:

- `minilyrics-android-debug`: berisi `app-debug.apk` (di dalam zip artifact)
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
5. Di DHU: buka launcher → **MiniLyrics** → **Mulai tes lirik**.
   Bisa juga lewat suara: "Hey Google, putar MiniLyrics". Perintah ini masuk ke
   `onPlayFromSearch`, dan query-nya diabaikan.

Tetap jalankan `adb logcat -s MiniLyrics` di terminal lain selama tes.

## Skenario tes

Catat hasilnya (screenshot DHU/mobil + potongan log) di PR atau issue.

| | Langkah | Yang diamati | Hasil |
|---|---|---|---|
| **A** | Buka MiniLyrics di Android Auto → **Mulai tes lirik** | Apakah card media/now playing berganti baris tiap 2 detik? Seberapa panjang baris 5 (sangat panjang) sebelum terpotong, di judul maupun subtitle? Apakah emoji di baris 4 tampil? | ☐ |
| **B** ⭐ | Putar lagu di Spotify dulu. Lalu di Android Auto buka MiniLyrics → play | **Paling penting.** Apakah Spotify ikut ter-pause? (Cek suaranya, dan log `onPlay` vs state Spotify di `dumpsys media_session`.) Apakah card media tetap milik MiniLyrics, atau balik ke Spotify? | ☐ |
| **C** | Lanjutan B: biarkan 1–2 lagu Spotify lewat (atau tekan next di Spotify dari HP) | Apakah card media balik sendiri ke Spotify waktu Spotify ganti lagu atau update metadata? | ☐ |
| **D** | Mode split-screen dengan Maps aktif | Apakah lirik tampil di slot card media (kiri) sementara Maps tetap di kanan? | ☐ |

Catatan untuk skenario B: service ini **sengaja tidak meminta audio focus**
dan tidak memutar audio. Jadi kalau Spotify berhenti, penyebabnya Android Auto
atau sistem, bukan app ini. Hasil B menentukan arah tahap berikutnya.

## Batasan yang diketahui

- `startForeground` bisa ditolak di Android 12+/14 kalau play dipicu saat app
  di background. Pemanggilannya dibungkus try/catch dan log mencatat
  `startForeground gagal: ...`. Service tetap jalan selama Android Auto
  masih ter-bind.
- Tidak ada signing release. Pakai build debug untuk sideload.

## Tahap berikutnya (belum dikerjakan)

1. Baca session Spotify via `MediaSessionManager.getActiveSessions()` +
   `NotificationListenerService` (judul, artis, durasi, posisi, status).
2. Ambil lirik ber-timestamp (LRC) dari sumber yang sama dengan versi desktop
   ([LRCLIB](https://lrclib.net)), parse, lalu hitung baris aktif dari posisi
   playback.
3. Teruskan tombol transport dari card ke `MediaController` milik Spotify.
4. Kalau di skenario B Spotify ter-pause, cari cara me-resume Spotify otomatis
   (mis. `transportControls.play()` ke controller Spotify setelah card
   diambil alih).

### Kode Rust yang relevan untuk dipakai ulang

Keputusan nanti: ekstrak logika lirik jadi crate Rust bersama yang dipanggil dari
Kotlin via **UniFFI**, atau tulis ulang di Kotlin. Semua modul di bawah ada di
`crates/core` (`mini-lyrics-core`) yang memang tanpa GUI/OS:

| Modul | Isi | Catatan untuk Android |
|---|---|---|
| `lrc.rs` | `parse_lrc`: multi-timestamp, `[mm:ss.xx]`/`[mm:ss:xx]` | Murni, mudah di-port atau di-expose |
| `lrclib.rs` | Klien LRCLIB: `get_exact`, `search`, `clean_title`, `pick_best`/`rank_candidates` (durasi terdekat) | Memakai `reqwest` blocking + rustls (native roots). Perlu dicek di NDK, atau HTTP-nya diganti di Kotlin |
| `karaoke.rs` | `active_index` (baris aktif dari posisi) | Murni |
| `model.rs` | `PlayerState` (posisi terinterpolasi), `TrackQuery`, `LyricLine`, `LyricsCandidate` | Tipe untuk boundary UniFFI |
| `controller.rs` | Cache per lagu, fetch di thread, retry 30 dtk, buang hasil basi, kandidat alternatif | Bisa dipakai utuh kalau lewat UniFFI |

`applescript.rs` dan `wintime.rs` khusus desktop dan tidak relevan untuk Android.
