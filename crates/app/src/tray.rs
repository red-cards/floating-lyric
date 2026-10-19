// Di OS tanpa tray (Linux, untuk development) sebagian besar isi modul ini
// tidak terpakai.
#![cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]

//! Ikon tray + menu (Windows & macOS).
//!
//! Tray harus dibuat SETELAH event loop berjalan (wajib di macOS), jadi
//! [`Tray::new`] dipanggil dari frame pertama, bukan dari `main`.

use crate::settings::Settings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    ToggleHidden,
    ToggleClickThrough,
    OffsetEarlier,
    OffsetLater,
    ResetOffset,
    ToggleDebug,
    OpenLogFolder,
    Quit,
}

pub fn offset_label(offset: f64) -> String {
    format!("Offset: {offset:+.1} dtk").replace('.', ",")
}

/// Ikon 32×32 RGBA digambar di kode: lingkaran kuning dengan not balok.
pub fn icon_rgba() -> (Vec<u8>, u32, u32) {
    const N: u32 = 32;
    let mut px = vec![0u8; (N * N * 4) as usize];
    let yellow = [0xFF, 0xD6, 0x4D];
    let dark = [0x1E, 0x1E, 0x1E];
    for y in 0..N {
        for x in 0..N {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let d = ((fx - 16.0).powi(2) + (fy - 16.0).powi(2)).sqrt();
            let i = ((y * N + x) * 4) as usize;
            if d <= 15.5 {
                // Anti-alias tepi lingkaran.
                let alpha = ((15.5 - d).clamp(0.0, 1.0) * 255.0) as u8;
                let ellipse =
                    |cx: f32, cy: f32| ((fx - cx) / 3.6).powi(2) + ((fy - cy) / 2.8).powi(2) <= 1.0;
                let note = ellipse(11.0, 22.0)
                    || ellipse(21.0, 20.0)
                    || (13.5..15.3).contains(&fx) && (9.0..22.0).contains(&fy)
                    || (23.5..25.3).contains(&fx) && (7.0..20.0).contains(&fy)
                    || (13.5..25.3).contains(&fx) && (fy - (9.5 - (fx - 13.5) * 0.17)).abs() < 1.6;
                let c = if note { dark } else { yellow };
                px[i..i + 4].copy_from_slice(&[c[0], c[1], c[2], alpha]);
            }
        }
    }
    (px, N, N)
}

#[cfg(any(windows, target_os = "macos"))]
mod imp {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use super::{icon_rgba, offset_label, TrayAction};
    use crate::settings::Settings;

    pub struct Tray {
        _icon: TrayIcon,
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
            let hide = MenuItem::new(hide_text(settings.hidden), true, None);
            let click_through =
                CheckMenuItem::new("Click-through", true, settings.click_through, None);
            let earlier = MenuItem::new("Lirik lebih cepat 0,5 dtk", true, None);
            let later = MenuItem::new("Lirik lebih lambat 0,5 dtk", true, None);
            let reset = MenuItem::new("Reset offset", true, None);
            let offset = MenuItem::new(offset_label(settings.offset_secs), false, None);
            let debug = CheckMenuItem::new("Mode debug", true, settings.debug, None);
            let logs = MenuItem::new("Buka folder log", true, None);
            let quit = MenuItem::new("Keluar", true, None);

            let sep = PredefinedMenuItem::separator;
            menu.append_items(&[
                &version,
                &sep(),
                &hide,
                &click_through,
                &sep(),
                &earlier,
                &later,
                &reset,
                &offset,
                &sep(),
                &debug,
                &logs,
                &sep(),
                &quit,
            ])
            .map_err(|e| err(&e))?;

            let (rgba, w, h) = icon_rgba();
            let icon = Icon::from_rgba(rgba, w, h).map_err(|e| err(&e))?;
            let tray = TrayIconBuilder::new()
                .with_menu(Box::new(menu))
                .with_icon(icon)
                .with_tooltip("MiniLyrics")
                .build()
                .map_err(|e| err(&e))?;

            let actions = vec![
                (hide.id().clone(), TrayAction::ToggleHidden),
                (click_through.id().clone(), TrayAction::ToggleClickThrough),
                (earlier.id().clone(), TrayAction::OffsetEarlier),
                (later.id().clone(), TrayAction::OffsetLater),
                (reset.id().clone(), TrayAction::ResetOffset),
                (debug.id().clone(), TrayAction::ToggleDebug),
                (logs.id().clone(), TrayAction::OpenLogFolder),
                (quit.id().clone(), TrayAction::Quit),
            ];
            Ok(Self {
                _icon: tray,
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
            self.hide.set_text(hide_text(settings.hidden));
            self.click_through.set_checked(settings.click_through);
            self.offset.set_text(offset_label(settings.offset_secs));
            self.debug.set_checked(settings.debug);
        }
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
        TrayAction::ToggleHidden => s.hidden = !s.hidden,
        TrayAction::ToggleClickThrough => s.click_through = !s.click_through,
        TrayAction::OffsetEarlier => s.offset_secs = round_offset(s.offset_secs + 0.5),
        TrayAction::OffsetLater => s.offset_secs = round_offset(s.offset_secs - 0.5),
        TrayAction::ResetOffset => s.offset_secs = 0.0,
        TrayAction::ToggleDebug => s.debug = !s.debug,
        TrayAction::OpenLogFolder | TrayAction::Quit => return false,
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
        assert_eq!(offset_label(-0.5), "Offset: -0,5 dtk");
    }

    #[test]
    fn icon_is_32x32_rgba() {
        let (px, w, h) = icon_rgba();
        assert_eq!((w, h), (32, 32));
        assert_eq!(px.len(), 32 * 32 * 4);
        // Pojok transparan, tengah tidak.
        assert_eq!(px[3], 0);
        assert!(px[((16 * 32 + 16) * 4 + 3) as usize] > 0);
    }
}
