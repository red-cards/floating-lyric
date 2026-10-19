//! Penyesuaian window native yang tidak disediakan egui/winit.
//!
//! Dipanggil ulang setelah perintah viewport yang bisa menimpa style
//! (mis. MousePassthrough di Windows menulis ulang extended style).

use raw_window_handle::HasWindowHandle;

#[cfg(windows)]
pub fn apply(window: &dyn HasWindowHandle) {
    use raw_window_handle::RawWindowHandle;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_APPWINDOW, WS_EX_NOACTIVATE,
        WS_EX_TOOLWINDOW,
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
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        // TOOLWINDOW: tidak muncul di Alt+Tab. NOACTIVATE: klik tidak
        // mencuri fokus dari editor.
        let wanted = (ex | (WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0) as isize)
            & !(WS_EX_APPWINDOW.0 as isize);
        if wanted != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, wanted);
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            log::debug!("WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE diterapkan");
        }
    }
}

#[cfg(target_os = "macos")]
pub fn apply(window: &dyn HasWindowHandle) {
    use objc2_app_kit::{NSStatusWindowLevel, NSView, NSWindowCollectionBehavior};
    use raw_window_handle::RawWindowHandle;

    let Ok(handle) = window.window_handle() else {
        log::warn!("window handle tidak tersedia");
        return;
    };
    let RawWindowHandle::AppKit(h) = handle.as_raw() else {
        return;
    };
    // SAFETY: ns_view adalah NSView milik winit yang masih hidup selama
    // window ada, dan fungsi ini dipanggil dari main thread (callback egui).
    let view: &NSView = unsafe { h.ns_view.cast::<NSView>().as_ref() };
    let Some(ns_window) = view.window() else {
        log::warn!("NSView belum punya NSWindow");
        return;
    };
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    ns_window.setHidesOnDeactivate(false);
    // Di atas app full-screen dan window biasa.
    ns_window.setLevel(NSStatusWindowLevel);
    log::debug!("NSWindow: collectionBehavior + NSStatusWindowLevel diterapkan");
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn apply(_window: &dyn HasWindowHandle) {}

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
