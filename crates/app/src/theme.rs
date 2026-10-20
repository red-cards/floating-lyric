//! Token visual MiniLyrics, diambil dari handoff Claude Design arah gaya
//! **1a · Kalem** (minimalis modern) untuk mode "Panel bawah", plus token
//! mode Notch, overlay debug, dan ikon tray.
//!
//! Semua warna, ukuran, opacity, dan durasi animasi ada di sini supaya bisa
//! disetel tanpa membongkar kode gambar. Warna ditulis hex + alpha, sama
//! seperti notasi di spec desain (`#FFFFFF α0.50`).

use eframe::egui::{vec2, Color32, Vec2};

/// Warna hex + alpha (0..1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tint {
    pub hex: u32,
    pub alpha: f32,
}

impl Tint {
    pub const fn new(hex: u32, alpha: f32) -> Self {
        Self { hex, alpha }
    }

    pub fn color(self) -> Color32 {
        self.faded(1.0)
    }

    /// Warna dengan alpha dikali `k` (opacity state, hover, fade).
    pub fn faded(self, k: f32) -> Color32 {
        let [_, r, g, b] = self.hex.to_be_bytes();
        let a = (self.alpha * k).clamp(0.0, 1.0);
        Color32::from_rgba_unmultiplied(r, g, b, (a * 255.0).round() as u8)
    }
}

/// Gaya satu jenis teks. `weight` mengikuti skala CSS (400 regular,
/// 600 semibold); lihat `fonts::weight_coords` untuk cara menerapkannya.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub weight: f32,
    /// Tinggi baris untuk tata letak vertikal (bukan tinggi glyph).
    pub line_height: f32,
    /// Letter spacing dalam em (dikali ukuran font).
    pub letter_spacing_em: f32,
}

impl TextStyle {
    const fn new(size: f32, weight: f32, line_height: f32) -> Self {
        Self {
            size,
            weight,
            line_height,
            letter_spacing_em: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Panel bawah (1a · Kalem)
// ---------------------------------------------------------------------------

/// Ukuran kotak panel lirik.
pub const PANEL_SIZE: Vec2 = vec2(560.0, 120.0);
pub const PANEL_RADIUS: u8 = 14;
pub const PANEL_BG: Tint = Tint::new(0x000000, 0.62);
pub const PANEL_BORDER: Tint = Tint::new(0xFFFFFF, 0.08);
pub const PANEL_BORDER_WIDTH: f32 = 1.0;
/// Satu shadow sederhana: `0 6px 18px #000000 α0.18`.
pub const PANEL_SHADOW_OFFSET: [i8; 2] = [0, 6];
pub const PANEL_SHADOW_BLUR: u8 = 18;
pub const PANEL_SHADOW: Tint = Tint::new(0x000000, 0.18);
/// Ruang transparan di sekeliling panel di dalam window, supaya shadow
/// tidak terpotong tepi window (blur egui melebar setengahnya ke luar).
pub const WINDOW_MARGIN_X: f32 = 10.0;
pub const WINDOW_MARGIN_TOP: f32 = 4.0;
pub const WINDOW_MARGIN_BOTTOM: f32 = 16.0;
/// Padding kiri/kanan isi panel (lebar teks maksimum 520 px).
pub const PANEL_PAD_X: f32 = 20.0;

/// Baris sebelumnya: kecil, paling redup.
pub const PREV_TEXT: TextStyle = TextStyle::new(13.0, 400.0, 18.0);
pub const PREV_COLOR: Tint = Tint::new(0xFFFFFF, 0.40);
/// Baris aktif: besar, semibold, letter spacing −0.01em.
pub const ACTIVE_TEXT: TextStyle = TextStyle {
    letter_spacing_em: -0.01,
    ..TextStyle::new(22.0, 600.0, 30.0)
};
/// Karaoke: teks putih redup terisi putih penuh dari kiri ke kanan.
pub const ACTIVE_BASE: Tint = Tint::new(0xFFFFFF, 0.50);
pub const ACTIVE_FILL: Tint = Tint::new(0xFFFFFF, 1.0);
/// Baris berikutnya: kecil, sedikit lebih terang dari baris sebelumnya.
pub const NEXT_TEXT: TextStyle = TextStyle::new(13.0, 400.0, 18.0);
pub const NEXT_COLOR: Tint = Tint::new(0xFFFFFF, 0.62);
/// Jarak vertikal antarbaris lirik.
pub const LINE_GAP: f32 = 5.0;
/// Baris terlalu panjang: font mengecil sampai batas ini, sisanya ellipsis.
pub const MIN_FONT_SCALE: f32 = 0.6;

/// Status (loading, tidak ketemu, error, idle): judul + sub.
pub const STATUS_MAIN_TEXT: TextStyle = TextStyle::new(15.0, 500.0, 20.0);
pub const STATUS_MAIN_COLOR: Tint = Tint::new(0xFFFFFF, 0.88);
pub const STATUS_SUB_TEXT: TextStyle = TextStyle::new(12.0, 400.0, 16.0);
pub const STATUS_SUB_COLOR: Tint = Tint::new(0xFFFFFF, 0.50);
/// Jarak baris judul ke sub.
pub const STATUS_GAP: f32 = 6.0;
/// Jarak ikon status ke teks judul.
pub const STATUS_ICON_GAP: f32 = 9.0;
/// Loading: tiga titik, yang paling terang berpindah ke kanan.
pub const LOADING_DOT_SIZE: f32 = 5.0;
pub const LOADING_DOT_GAP: f32 = 4.0;
pub const LOADING_DOT_COLOR: Tint = Tint::new(0xFFFFFF, 1.0);
pub const LOADING_DOT_ALPHAS: [f32; 3] = [0.90, 0.55, 0.25];
/// Lirik tidak ketemu: cincin kosong.
pub const NOT_FOUND_RING_SIZE: f32 = 8.0;
pub const NOT_FOUND_RING_WIDTH: f32 = 1.5;
pub const NOT_FOUND_RING_COLOR: Tint = Tint::new(0xFFFFFF, 0.55);
/// Error jaringan: titik koral (satu-satunya warna di gaya Kalem).
pub const ERROR_DOT_SIZE: f32 = 7.0;
pub const ERROR_DOT_COLOR: Tint = Tint::new(0xFF8A7A, 1.0);

/// Opacity seluruh panel (latar + isi) saat lagu dijeda.
pub const PAUSED_OPACITY: f32 = 0.45;
/// Tanda jeda ❚❚ di kiri panel.
pub const PAUSE_BAR_SIZE: Vec2 = vec2(3.0, 14.0);
/// 1.5 di desain. CornerRadius egui bilangan bulat; 2 di-clamp egui ke
/// setengah lebar bar (1.5), jadi ujungnya tetap bulat penuh.
pub const PAUSE_BAR_RADIUS: u8 = 2;
pub const PAUSE_BAR_GAP: f32 = 3.0;
/// Posisi pojok kiri atas bar pertama, relatif ke pojok panel.
pub const PAUSE_BARS_POS: Vec2 = vec2(19.0, 51.0);
pub const PAUSE_BAR_COLOR: Tint = Tint::new(0xFFFFFF, 1.0);

/// Kontrol yang muncul saat kursor di atas panel (hanya kalau click-through
/// mati): geser sinkron di kiri atas, pilih lirik lain di kanan atas.
/// Saat tidak di-hover panel tetap bersih tanpa tombol.
pub const CONTROLS_TOP: f32 = 7.0;
pub const CONTROLS_INSET_X: f32 = 8.0;
pub const CONTROL_HEIGHT: f32 = 22.0;
/// Lebar tombol ikon (−, +, ‹, ›) dan padding tombol teks.
pub const CONTROL_ICON_WIDTH: f32 = 22.0;
pub const CONTROL_TEXT_PAD_X: f32 = 8.0;
pub const CONTROL_RADIUS: u8 = 11;
pub const CONTROL_GROUP_BG: Tint = Tint::new(0x2A2A2A, 0.96);
pub const CONTROL_HOVER_BG: Tint = Tint::new(0xFFFFFF, 0.12);
pub const CONTROL_PRESS_BG: Tint = Tint::new(0xFFFFFF, 0.20);
pub const CONTROL_ICON_COLOR: Tint = Tint::new(0xFFFFFF, 0.85);
pub const CONTROL_ICON_SIZE: f32 = 8.0;
pub const CONTROL_ICON_STROKE: f32 = 1.5;
pub const CONTROL_TEXT: TextStyle = TextStyle::new(11.0, 500.0, 14.0);
pub const CONTROL_TEXT_COLOR: Tint = Tint::new(0xFFFFFF, 0.75);
/// Opacity tombol yang tidak bisa diklik (mis. "Nyari…").
pub const CONTROL_DISABLED_ALPHA: f32 = 0.45;
/// Kontrol fade-in/out saat kursor masuk/keluar panel.
pub const CONTROLS_FADE: f32 = 0.15;

/// Opacity panel saat kursor berada di atasnya dan click-through aktif
/// (atau di mode notch), supaya yang di bawahnya tetap terlihat.
pub const HOVER_OPACITY: f32 = 0.3;

// ---------------------------------------------------------------------------
// Overlay debug (kotak terpisah di bawah panel)
// ---------------------------------------------------------------------------

pub const DEBUG_GAP: f32 = 8.0;
pub const DEBUG_RADIUS: u8 = 10;
pub const DEBUG_BG: Tint = Tint::new(0x000000, 0.72);
pub const DEBUG_PAD: Vec2 = vec2(12.0, 8.0);
pub const DEBUG_FONT_SIZE: f32 = 10.5;
pub const DEBUG_ROW_HEIGHT: f32 = 15.0;
/// Lebar kolom kunci (`src`, `lagu`, …) dan jarak ke kolom nilai.
pub const DEBUG_KEY_WIDTH: f32 = 48.0;
pub const DEBUG_COL_GAP: f32 = 8.0;
pub const DEBUG_KEY_COLOR: Tint = Tint::new(0xFFFFFF, 0.40);
pub const DEBUG_VALUE_COLOR: Tint = Tint::new(0xFFFFFF, 0.80);
/// 5 baris tetap + 2 baris error opsional.
pub const DEBUG_MAX_ROWS: usize = 7;
/// Tinggi tambahan window saat mode debug aktif.
pub const DEBUG_HEIGHT: f32 =
    DEBUG_GAP + 2.0 * DEBUG_PAD.y + DEBUG_MAX_ROWS as f32 * DEBUG_ROW_HEIGHT;

// ---------------------------------------------------------------------------
// Animasi (detik)
// ---------------------------------------------------------------------------

/// Panel meredup/kembali saat pause/play.
pub const PAUSE_FADE: f32 = 0.20;
/// Redup saat kursor masuk/keluar panel.
pub const HOVER_FADE: f32 = 0.15;
/// Isi baru fade-in saat state berganti (mis. loading → lirik).
pub const STATE_FADE: f32 = 0.18;
/// Pindah baris: semua baris naik dari `LINE_SHIFT_PX` ke posisinya sambil
/// fade-in dari `LINE_SHIFT_FROM_ALPHA`, easing cubic-out.
pub const LINE_SHIFT: f32 = 0.22;
pub const LINE_SHIFT_PX: f32 = 6.0;
pub const LINE_SHIFT_FROM_ALPHA: f32 = 0.4;
/// Titik loading berpindah tiap langkah ini.
pub const LOADING_STEP: f32 = 0.30;

// ---------------------------------------------------------------------------
// Mode Notch (arah gaya D, tidak berubah di putaran ini)
// ---------------------------------------------------------------------------

pub const NOTCH_TEXT: TextStyle = TextStyle::new(17.0, 600.0, 26.0);
pub const NOTCH_BASE: Tint = Tint::new(0xFFFFFF, 0.55);
pub const NOTCH_FILL: Tint = Tint::new(0xFFD64D, 1.0);
pub const NOTCH_HINT_TEXT: TextStyle = TextStyle::new(15.0, 400.0, 20.0);
pub const NOTCH_HINT_COLOR: Tint = Tint::new(0xFFFFFF, 0.55);
pub const NOTCH_STATUS_TEXT: TextStyle = TextStyle::new(14.0, 400.0, 20.0);
pub const NOTCH_STATUS_COLOR: Tint = Tint::new(0xFFFFFF, 0.80);
pub const NOTCH_EXPAND: f32 = 0.35;

// ---------------------------------------------------------------------------
// Ikon tray (grid 16 px, dirender 2× jadi 32×32)
// ---------------------------------------------------------------------------

/// (x, y, w, h, radius) di grid 16 px.
pub type IconRect = (f32, f32, f32, f32, f32);
/// Bingkai rounded rect (stroke).
pub const TRAY_FRAME: IconRect = (1.0, 2.5, 14.0, 11.0, 3.5);
pub const TRAY_FRAME_STROKE: f32 = 1.5;
/// Bar "baris aktif" dan bar kecil "baris berikutnya".
pub const TRAY_BAR: IconRect = (4.0, 5.75, 8.0, 2.0, 1.0);
pub const TRAY_BAR_SMALL: IconRect = (5.5, 9.0, 5.0, 1.5, 0.75);
/// Versi gelap untuk menu bar/taskbar terang, versi terang untuk yang gelap.
/// macOS memakai versi gelap sebagai template (OS yang membalik warna).
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub const TRAY_ON_LIGHT: u32 = 0x000000;
#[cfg_attr(not(windows), allow(dead_code))]
pub const TRAY_ON_DARK: u32 = 0xFFFFFF;
/// Opacity ikon saat lirik disembunyikan.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
pub const TRAY_HIDDEN_ALPHA: f32 = 0.45;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_matches_spec_notation() {
        assert_eq!(
            PANEL_BG.color(),
            Color32::from_rgba_unmultiplied(0, 0, 0, 158)
        );
        assert_eq!(ERROR_DOT_COLOR.color(), Color32::from_rgb(0xFF, 0x8A, 0x7A));
        assert_eq!(ACTIVE_BASE.faded(0.0).a(), 0);
    }

    #[test]
    fn debug_area_fits_all_rows() {
        assert_eq!(DEBUG_HEIGHT, 8.0 + 16.0 + 7.0 * 15.0);
    }
}
