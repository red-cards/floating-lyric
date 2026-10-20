// Di OS tanpa tray (Linux, untuk development) sebagian besar isi modul ini
// tidak terpakai.
#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

//! Ikon tray + menu (Windows & macOS).
//!
//! Tray harus dibuat SETELAH event loop berjalan (wajib di macOS), jadi
//! [`Tray::new`] dipanggil dari frame pertama, bukan dari `main`.

use crate::settings::{DisplayMode, Settings};
use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ModePanel,
    ModeNotch,
    ToggleHidden,
    ToggleClickThrough,
    OffsetEarlier,
    OffsetLater,
    ResetOffset,
    /// Pilih kandidat lirik lain (lirik yang ketemu tidak cocok).
    NextLyrics,
    /// Kandidat lirik sebelumnya (hanya dari tombol panel).
    PrevLyrics,
    ToggleDebug,
    OpenLogFolder,
    Quit,
}

/// Offset untuk label: koma desimal dan tanda minus asli (−0,5 dtk).
pub fn offset_text(offset: f64) -> String {
    format!("{offset:+.1} dtk")
        .replace('.', ",")
        .replace('-', "\u{2212}")
}

pub fn offset_label(offset: f64) -> String {
    format!("Offset: {}", offset_text(offset))
}

/// Ikon 32×32 RGBA digambar di kode (grid 16 px dari desain, skala 2×):
/// bingkai rounded rect dengan dua bar "baris lirik". `rgb` = warna ikon,
/// `alpha` = opacity (redup saat lirik disembunyikan).
pub fn icon_rgba(rgb: u32, alpha: f32) -> (Vec<u8>, u32, u32) {
    const N: u32 = 32;
    const SCALE: f32 = 2.0;
    /// Supersampling per sumbu untuk anti-alias.
    const SS: u32 = 4;
    let [_, r, g, b] = rgb.to_be_bytes();
    let mut px = vec![0u8; (N * N * 4) as usize];
    for y in 0..N {
        for x in 0..N {
            let mut hits = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = (x as f32 + (sx as f32 + 0.5) / SS as f32) / SCALE;
                    let fy = (y as f32 + (sy as f32 + 0.5) / SS as f32) / SCALE;
                    hits += u32::from(icon_covers(fx, fy));
                }
            }
            let coverage = hits as f32 / (SS * SS) as f32;
            let i = ((y * N + x) * 4) as usize;
            let a = (coverage * alpha.clamp(0.0, 1.0) * 255.0).round() as u8;
            px[i..i + 4].copy_from_slice(&[r, g, b, a]);
        }
    }
    (px, N, N)
}

/// Titik (grid 16 px) berada di dalam bentuk ikon?
fn icon_covers(x: f32, y: f32) -> bool {
    let (fx, fy, fw, fh, fr) = theme::TRAY_FRAME;
    let s = theme::TRAY_FRAME_STROKE;
    let hole = (
        fx + s,
        fy + s,
        fw - 2.0 * s,
        fh - 2.0 * s,
        (fr - s).max(0.0),
    );
    (in_rounded_rect(x, y, theme::TRAY_FRAME) && !in_rounded_rect(x, y, hole))
        || in_rounded_rect(x, y, theme::TRAY_BAR)
        || in_rounded_rect(x, y, theme::TRAY_BAR_SMALL)
}

fn in_rounded_rect(x: f32, y: f32, (rx, ry, w, h, r): theme::IconRect) -> bool {
    if x < rx || x > rx + w || y < ry || y > ry + h {
        return false;
    }
    let cx = x.clamp(rx + r, rx + w - r);
    let cy = y.clamp(ry + r, ry + h - r);
    (x - cx).powi(2) + (y - cy).powi(2) <= r * r
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::{icon_rgba, offset_label, TrayAction};
    use crate::settings::{DisplayMode, Settings};
    use crate::theme;

    pub struct Tray {
        icon: TrayIcon,
        /// Status `hidden` yang tergambar di ikon sekarang.
        icon_hidden: std::cell::Cell<bool>,
        mode_panel: CheckMenuItem,
        mode_notch: CheckMenuItem,
        hide: MenuItem,
        click_through: CheckMenuItem,
        offset: MenuItem,
        debug: CheckMenuItem,
        actions: Vec<(MenuId, TrayAction)>,
    }

    impl Tray {
        pub fn new(version_label: &str, settings: &Settings) -> Result<Self, String> {
            let err = |e: &dyn std::fmt::Display| format!("tray: {e}");
            let menu = Menu::new();
            let version = MenuItem::new(version_label, false, None);
            let notch = settings.display_mode == DisplayMode::Notch;
            let mode_header = MenuItem::new("Mode tampilan", false, None);
            let mode_notch = CheckMenuItem::new("Notch", true, notch, None);
            let mode_panel = CheckMenuItem::new("Panel bawah", true, !notch, None);
            let hide = MenuItem::new(hide_text(settings.hidden), true, None);
            let click_through =
                CheckMenuItem::new("Click-through", true, settings.click_through(), None);
            let earlier = MenuItem::new("Lirik lebih cepat 0,5 dtk", true, None);
            let later = MenuItem::new("Lirik lebih lambat 0,5 dtk", true, None);
            let reset = MenuItem::new("Reset offset", true, None);
            let next_lyrics = MenuItem::new("Coba lirik lain", true, None);
            let offset = MenuItem::new(offset_label(settings.offset_secs), false, None);
            let debug = CheckMenuItem::new("Mode debug", true, settings.debug, None);
            let logs = MenuItem::new("Buka folder log", true, None);
            let quit = MenuItem::new("Keluar", true, None);

            let sep = PredefinedMenuItem::separator;
            // Urutan sesuai mockup menu tray di desain.
            menu.append_items(&[
                &version,
                &sep(),
                &hide,
                &click_through,
                &sep(),
                &mode_header,
                &mode_notch,
                &mode_panel,
                &sep(),
                &offset,
                &earlier,
                &later,
                &reset,
                &sep(),
                &next_lyrics,
                &sep(),
                &debug,
                &logs,
                &sep(),
                &quit,
            ])
            .map_err(|e| err(&e))?;

            let icon = make_icon(settings.hidden).map_err(|e| err(&e))?;
            let builder = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_tooltip("MiniLyrics");
            // macOS: template image, warnanya dibalik OS sesuai menu bar.
            #[cfg(target_os = "macos")]
            let builder = builder.with_icon_templated(icon);
            #[cfg(not(target_os = "macos"))]
            let builder = builder.with_icon(icon);
            let tray = builder.build().map_err(|e| err(&e))?;

            let actions = vec![
                (mode_panel.id().clone(), TrayAction::ModePanel),
                (mode_notch.id().clone(), TrayAction::ModeNotch),
                (hide.id().clone(), TrayAction::ToggleHidden),
                (click_through.id().clone(), TrayAction::ToggleClickThrough),
                (earlier.id().clone(), TrayAction::OffsetEarlier),
                (later.id().clone(), TrayAction::OffsetLater),
                (reset.id().clone(), TrayAction::ResetOffset),
                (next_lyrics.id().clone(), TrayAction::NextLyrics),
                (debug.id().clone(), TrayAction::ToggleDebug),
                (logs.id().clone(), TrayAction::OpenLogFolder),
                (quit.id().clone(), TrayAction::Quit),
            ];
            Ok(Self {
                icon: tray,
                icon_hidden: std::cell::Cell::new(settings.hidden),
                mode_panel,
                mode_notch,
                hide,
                click_through,
                offset,
                debug,
                actions,
            })
        }

        /// Ambil semua klik menu yang menumpuk sejak frame sebelumnya.
        pub fn poll(&self) -> Vec<TrayAction> {
            let mut out = Vec::new();
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if let Some((_, action)) = self.actions.iter().find(|(id, _)| *id == event.id) {
                    out.push(*action);
                }
            }
            out
        }

        /// Samakan teks/centang menu dengan setting.
        pub fn sync(&self, settings: &Settings) {
            let notch = settings.display_mode == DisplayMode::Notch;
            // CheckMenuItem membalik centang sendiri saat diklik; samakan lagi
            // supaya berperilaku seperti radio.
            self.mode_panel.set_checked(!notch);
            self.mode_notch.set_checked(notch);
            self.hide.set_text(hide_text(settings.hidden));
            self.click_through.set_checked(settings.click_through());
            self.offset.set_text(offset_label(settings.offset_secs));
            self.debug.set_checked(settings.debug);
            if self.icon_hidden.get() != settings.hidden {
                self.icon_hidden.set(settings.hidden);
                let result = make_icon(settings.hidden).and_then(|icon| {
                    #[cfg(target_os = "macos")]
                    return self
                        .icon
                        .set_icon_templated(Some(icon))
                        .map_err(|e| e.to_string());
                    #[cfg(not(target_os = "macos"))]
                    self.icon.set_icon(Some(icon)).map_err(|e| e.to_string())
                });
                if let Err(e) = result {
                    log::warn!("tray: gagal mengganti ikon: {e}");
                }
            }
        }
    }

    /// Ikon tray; redup saat lirik disembunyikan.
    fn make_icon(hidden: bool) -> Result<Icon, String> {
        // macOS memakai template (hanya alpha yang dipakai). Windows: versi
        // gelap untuk taskbar terang, versi terang untuk taskbar gelap.
        #[cfg(target_os = "macos")]
        let rgb = theme::TRAY_ON_LIGHT;
        #[cfg(windows)]
        let rgb = if crate::os_window::taskbar_is_light() {
            theme::TRAY_ON_LIGHT
        } else {
            theme::TRAY_ON_DARK
        };
        let alpha = if hidden {
            theme::TRAY_HIDDEN_ALPHA
        } else {
            1.0
        };
        let (rgba, w, h) = icon_rgba(rgb, alpha);
        Icon::from_rgba(rgba, w, h).map_err(|e| e.to_string())
    }

    fn hide_text(hidden: bool) -> &'static str {
        if hidden {
            "Tampilkan lirik"
        } else {
            "Sembunyikan lirik"
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod imp {
    use super::TrayAction;
    use crate::settings::Settings;

    /// Linux (development): tidak ada tray.
    pub struct Tray;

    impl Tray {
        pub fn new(_version_label: &str, _settings: &Settings) -> Result<Self, String> {
            Err("tray tidak didukung di OS ini".into())
        }
        pub fn poll(&self) -> Vec<TrayAction> {
            Vec::new()
        }
        pub fn sync(&self, _settings: &Settings) {}
    }
}

pub use imp::Tray;

/// Terapkan aksi tray ke setting. Mengembalikan `true` kalau setting berubah.
pub fn apply_to_settings(action: TrayAction, s: &mut Settings) -> bool {
    match action {
        TrayAction::ModePanel => s.display_mode = DisplayMode::Panel,
        TrayAction::ModeNotch => s.display_mode = DisplayMode::Notch,
        TrayAction::ToggleHidden => s.hidden = !s.hidden,
        TrayAction::ToggleClickThrough => s.toggle_click_through(),
        TrayAction::OffsetEarlier => s.offset_secs = round_offset(s.offset_secs + 0.5),
        TrayAction::OffsetLater => s.offset_secs = round_offset(s.offset_secs - 0.5),
        TrayAction::ResetOffset => s.offset_secs = 0.0,
        TrayAction::ToggleDebug => s.debug = !s.debug,
        TrayAction::NextLyrics
        | TrayAction::PrevLyrics
        | TrayAction::OpenLogFolder
        | TrayAction::Quit => return false,
    }
    true
}

fn round_offset(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_actions() {
        let mut s = Settings::default();
        apply_to_settings(TrayAction::OffsetEarlier, &mut s);
        apply_to_settings(TrayAction::OffsetEarlier, &mut s);
        assert_eq!(s.offset_secs, 1.0);
        apply_to_settings(TrayAction::OffsetLater, &mut s);
        assert_eq!(s.offset_secs, 0.5);
        apply_to_settings(TrayAction::ResetOffset, &mut s);
        assert_eq!(s.offset_secs, 0.0);
        assert_eq!(offset_label(-0.5), "Offset: \u{2212}0,5 dtk");
        assert_eq!(offset_text(1.0), "+1,0 dtk");
    }

    #[test]
    fn mode_and_click_through_actions() {
        let mut s = Settings::default();
        apply_to_settings(TrayAction::ModeNotch, &mut s);
        assert_eq!(s.display_mode, DisplayMode::Notch);
        assert!(s.click_through());
        apply_to_settings(TrayAction::ToggleClickThrough, &mut s);
        assert!(!s.click_through());
        apply_to_settings(TrayAction::ModePanel, &mut s);
        assert_eq!(s.display_mode, DisplayMode::Panel);
        assert!(!s.click_through());
    }

    #[test]
    fn icon_is_32x32_rgba() {
        let alpha_at = |px: &[u8], x: u32, y: u32| px[((y * 32 + x) * 4 + 3) as usize];
        let (px, w, h) = icon_rgba(0x000000, 1.0);
        assert_eq!((w, h), (32, 32));
        assert_eq!(px.len(), 32 * 32 * 4);
        // Pojok transparan, bingkai kiri & bar aktif terisi, celah di antara
        // bingkai dan bar kosong.
        assert_eq!(alpha_at(&px, 0, 0), 0);
        assert_eq!(alpha_at(&px, 3, 16), 255);
        assert_eq!(alpha_at(&px, 16, 13), 255);
        assert_eq!(alpha_at(&px, 16, 10), 0);
        // Saat disembunyikan: redup.
        let (dim, _, _) = icon_rgba(0xFFFFFF, theme::TRAY_HIDDEN_ALPHA);
        assert_eq!(alpha_at(&dim, 16, 13), 115);
        assert_eq!(&dim[..3], &[0xFF, 0xFF, 0xFF]);
    }
}
