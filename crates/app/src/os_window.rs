//! Penyesuaian window native yang tidak disediakan egui/winit.
//!
//! Dipanggil ulang setelah perintah viewport yang bisa menimpa style
//! (mis. MousePassthrough di Windows menulis ulang style & extended style),
//! dan secara berkala karena winit bisa menulis ulang style kapan saja.

use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
pub fn apply(window: &dyn HasWindowHandle) {
    use raw_window_handle::RawWindowHandle;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE,
        SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_BORDER,
        WS_CAPTION, WS_DLGFRAME, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
    };

    let Ok(handle) = window.window_handle() else {
        log::warn!("window handle tidak tersedia");
        return;
    };
    let RawWindowHandle::Win32(h) = handle.as_raw() else {
        return;
    };
    let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
    unsafe {
        // winit selalu memasang WS_CAPTION | WS_BORDER | WS_SYSMENU (juga di
        // window tanpa dekorasi) lalu menyembunyikannya lewat WM_NCCALCSIZE.
        // Digabung WS_EX_TOOLWINDOW, Windows tetap menggambar bingkai tipis
        // dan tombol × tool window. Jadikan WS_POPUP murni tanpa bingkai.
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let frame = (WS_CAPTION
            | WS_BORDER
            | WS_DLGFRAME
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX)
            .0 as isize;
        let wanted_style = (style & !frame) | WS_POPUP.0 as isize;

        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        // TOOLWINDOW: tidak muncul di Alt+Tab. NOACTIVATE: klik tidak
        // mencuri fokus dari editor.
        let wanted_ex = (ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize)
            & !((WS_EX_APPWINDOW.0 | WS_EX_WINDOWEDGE.0) as isize);

        if wanted_style != style || wanted_ex != ex {
            SetWindowLongPtrW(hwnd, GWL_STYLE, wanted_style);
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted_ex);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            log::debug!("style window: WS_POPUP tanpa bingkai, TOOLWINDOW | NOACTIVATE");
            hide_dwm_frame(hwnd);
        }
    }
}

/// Windows 11 menggambar border 1 px dan sudut membulat di semua window,
/// termasuk window tanpa dekorasi. Karena window MiniLyrics lebih besar dari
/// panel/pill dan sisanya transparan, border itu jadi terlihat sebagai garis
/// tipis di sekeliling area kosong. Matikan keduanya. Di Windows 10 atribut
/// ini tidak dikenal; error-nya diabaikan.
#[cfg(windows)]
fn hide_dwm_frame(hwnd: windows::Win32::Foundation::HWND) {
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
        DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
    };

    let color: u32 = DWMWA_COLOR_NONE;
    let corner = DWMWCP_DONOTROUND;
    unsafe {
        let border = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            &color as *const u32 as *const core::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        let round = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const _ as *const core::ffi::c_void,
            std::mem::size_of_val(&corner) as u32,
        );
        match (border, round) {
            (Ok(()), Ok(())) => log::debug!("border & sudut DWM dimatikan"),
            (b, r) => log::debug!("atribut DWM tidak didukung (Windows 10?): {b:?} {r:?}"),
        }
    }
}

#[cfg(target_os = "macos")]
pub fn apply(window: &dyn HasWindowHandle) {
    use objc2_app_kit::{NSStatusWindowLevel, NSWindowCollectionBehavior};

    let Some(ns_window) = ns_window(window) else {
        log::warn!("NSWindow tidak tersedia");
        return;
    };
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns_window.setHidesOnDeactivate(false);
    // Bayangan bawaan macOS dihitung dari isi window dan tidak ikut
    // diperbarui saat bentuk panel/notch beranimasi, jadi muncul garis tepi
    // ganda. Panel sudah menggambar bayangannya sendiri; mode notch memang
    // tanpa bayangan.
    ns_window.setHasShadow(false);
    // NSStatusWindowLevel (25) > NSMainMenuWindowLevel (24): di atas menu bar
    // (perlu untuk mode notch) dan di atas app full-screen.
    ns_window.setLevel(NSStatusWindowLevel);
    log::trace!("NSWindow: collectionBehavior, tanpa shadow, NSStatusWindowLevel diterapkan");
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn apply(_window: &dyn HasWindowHandle) {}

/// Ambil NSWindow dari window winit.
#[cfg(target_os = "macos")]
fn ns_window(window: &dyn HasWindowHandle) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use objc2_app_kit::NSView;
    use raw_window_handle::RawWindowHandle;

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return None;
    };
    // SAFETY: ns_view adalah NSView milik winit yang masih hidup selama
    // window ada, dan semua pemanggil berjalan di main thread (callback egui).
    let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
    view.window()
}

// ---------------------------------------------------------------------------
// Mode notch: deteksi notch, penempatan di tengah atas, posisi kursor global.

#[cfg(target_os = "macos")]
mod top {
    use eframe::egui::{pos2, Pos2, Vec2};
    use objc2::rc::Retained;
    use objc2::runtime::NSObjectProtocol;
    use objc2::{sel, MainThreadMarker};
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::{NSPoint, NSRect, NSSize};

    use crate::notch::NotchMetrics;

    /// Notch sebuah layar + pusat-x-nya (koordinat global Cocoa).
    #[derive(Clone, Copy)]
    struct ScreenNotch {
        metrics: NotchMetrics,
        center_x: f64,
    }

    /// Layar target: layar pertama yang punya notch, kalau tidak ada layar
    /// utama (yang punya menu bar = `screens[0]`).
    fn pick_screen() -> Option<(Retained<NSScreen>, Option<ScreenNotch>)> {
        let mtm = MainThreadMarker::new()?;
        let screens = NSScreen::screens(mtm);
        for screen in screens.iter() {
            if let Some(n) = notch_of(&screen) {
                return Some((screen, Some(n)));
            }
        }
        screens.firstObject().map(|s| (s, None))
    }

    fn notch_of(screen: &NSScreen) -> Option<ScreenNotch> {
        // safeAreaInsets & auxiliaryTop*Area baru ada sejak macOS 12.
        if !screen.respondsToSelector(sel!(safeAreaInsets)) {
            return None;
        }
        let top = screen.safeAreaInsets().top;
        if top <= 0.0 {
            return None;
        }
        let frame = screen.frame();
        let has_aux = screen.respondsToSelector(sel!(auxiliaryTopLeftArea))
            && screen.respondsToSelector(sel!(auxiliaryTopRightArea));
        let (width, center_x) = if has_aux {
            let left = screen.auxiliaryTopLeftArea();
            let right = screen.auxiliaryTopRightArea();
            let w = frame.size.width - left.size.width - right.size.width;
            let cx = frame.origin.x + left.size.width + w / 2.0;
            (w, cx)
        } else {
            (0.0, 0.0)
        };
        let (width, center_x) = if width > 0.0 {
            (width, center_x)
        } else {
            // Perkiraan lebar notch MacBook kalau area auxiliary tidak ada.
            (200.0, frame.origin.x + frame.size.width / 2.0)
        };
        Some(ScreenNotch {
            metrics: NotchMetrics {
                width: width as f32,
                height: top as f32,
            },
            center_x,
        })
    }

    pub fn notch_metrics() -> Option<NotchMetrics> {
        pick_screen().and_then(|(_, n)| n.map(|n| n.metrics))
    }

    /// Taruh window di tengah atas layar target, di atas menu bar. Frame
    /// diset langsung lewat `setFrame:display:` (window borderless tidak
    /// di-constrain ke bawah menu bar); hasilnya dicek ulang.
    pub fn place_top(window: &dyn raw_window_handle::HasWindowHandle, size: Vec2, margin: f32) {
        let Some(ns_window) = super::ns_window(window) else {
            return;
        };
        let Some((screen, notch)) = pick_screen() else {
            return;
        };
        let frame = screen.frame();
        let center_x = notch.map_or(frame.origin.x + frame.size.width / 2.0, |n| n.center_x);
        let (w, h) = (size.x as f64, size.y as f64);
        let target = NSRect::new(
            NSPoint::new(
                (center_x - w / 2.0).round(),
                frame.origin.y + frame.size.height - margin as f64 - h,
            ),
            NSSize::new(w, h),
        );
        let close = |a: NSRect, b: NSRect| {
            (a.origin.x - b.origin.x).abs() < 0.5
                && (a.origin.y - b.origin.y).abs() < 0.5
                && (a.size.width - b.size.width).abs() < 0.5
                && (a.size.height - b.size.height).abs() < 0.5
        };
        if close(ns_window.frame(), target) {
            return;
        }
        ns_window.setFrame_display(target, true);
        let actual = ns_window.frame();
        if close(actual, target) {
            log::debug!("window notch ditempatkan: {target:?}");
        } else {
            log::warn!("frame window di-constrain oleh macOS: minta {target:?}, dapat {actual:?}");
        }
    }

    /// Posisi kursor relatif ke pojok kiri atas window (points). Dibaca
    /// global karena event mouse tidak diterima saat passthrough.
    pub fn cursor_in_window(window: &dyn raw_window_handle::HasWindowHandle) -> Option<Pos2> {
        let ns_window = super::ns_window(window)?;
        let p = NSEvent::mouseLocation();
        let f = ns_window.frame();
        Some(pos2(
            (p.x - f.origin.x) as f32,
            (f.origin.y + f.size.height - p.y) as f32,
        ))
    }
}

#[cfg(not(target_os = "macos"))]
mod top {
    use eframe::egui::{Pos2, Vec2};
    use winit::dpi::{LogicalSize, PhysicalPosition};
    use winit::window::Window;

    use crate::notch::NotchMetrics;

    /// Hanya Mac yang punya notch.
    pub fn notch_metrics() -> Option<NotchMetrics> {
        None
    }

    /// Taruh window di tengah atas monitor utama.
    pub fn place_top(window: &Window, size: Vec2, margin: f32) {
        let Some(monitor) = window
            .primary_monitor()
            .or_else(|| window.current_monitor())
        else {
            return;
        };
        let scale = monitor.scale_factor();
        let mpos = monitor.position();
        let msize = monitor.size();
        let x = mpos.x + ((msize.width as f64 - size.x as f64 * scale) / 2.0).round() as i32;
        let y = mpos.y + (margin as f64 * scale).round() as i32;

        let cur = window.inner_size().to_logical::<f32>(window.scale_factor());
        if (cur.width - size.x).abs() > 0.5 || (cur.height - size.y).abs() > 0.5 {
            let _ = window.request_inner_size(LogicalSize::new(size.x, size.y));
        }
        let target = PhysicalPosition::new(x, y);
        if window.outer_position().ok() != Some(target) {
            window.set_outer_position(target);
            log::debug!("window pill ditempatkan di {target:?}");
        }
    }

    #[cfg(windows)]
    pub fn cursor_in_window(window: &Window) -> Option<Pos2> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows::Win32::Foundation::{HWND, POINT};
        use windows::Win32::Graphics::Gdi::ScreenToClient;
        use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

        let handle = window.window_handle().ok()?;
        let RawWindowHandle::Win32(h) = handle.as_raw() else {
            return None;
        };
        let hwnd = HWND(h.hwnd.get() as *mut core::ffi::c_void);
        let mut pt = POINT::default();
        unsafe {
            GetCursorPos(&mut pt).ok()?;
            if !ScreenToClient(hwnd, &mut pt).as_bool() {
                return None;
            }
        }
        let scale = window.scale_factor() as f32;
        Some(eframe::egui::pos2(pt.x as f32 / scale, pt.y as f32 / scale))
    }

    #[cfg(not(windows))]
    pub fn cursor_in_window(_window: &Window) -> Option<Pos2> {
        None
    }
}

pub use top::{cursor_in_window, notch_metrics, place_top};

/// Buka folder di file manager OS.
pub fn open_folder(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(not(any(windows, target_os = "macos")))]
    let cmd = "xdg-open";
    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        log::warn!("gagal membuka {}: {e}", path.display());
    }
}

/// Taskbar Windows bertema terang? (untuk memilih versi ikon tray). Default
/// gelap kalau nilai registry tidak terbaca.
#[cfg(windows)]
pub fn taskbar_is_light() -> bool {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};

    let mut value: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    result.is_ok() && value != 0
}
