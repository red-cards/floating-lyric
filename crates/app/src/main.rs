// Release Windows: tanpa jendela console.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod fonts;
mod logging;
mod notch;
mod now_playing;
mod os_window;
mod settings;
mod theme;
mod tray;
mod ui;

use eframe::egui::{self, WindowLevel};

use crate::settings::{Paths, Settings};

fn main() {
    let paths = Paths::resolve();
    logging::init(&paths.log_dir);
    log::info!(
        "{} mulai | {} {} | config {} | log {}",
        ui::version_label(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        paths.config_file.display(),
        paths.log_dir.display()
    );
    for var in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "NO_PROXY",
    ] {
        if let Ok(v) = std::env::var(var) {
            log::info!("env {var}={}", redact_userinfo(&v));
        }
    }

    let settings = Settings::load(&paths.config_file);
    log::info!("setting: {settings:?}");

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("MiniLyrics")
        .with_inner_size(ui::window_size(settings.debug))
        .with_decorations(false)
        .with_transparent(true)
        .with_window_level(WindowLevel::AlwaysOnTop)
        .with_taskbar(false)
        .with_resizable(false)
        // Jangan mencuri fokus dari editor.
        .with_active(false);
    // Mode notch ditempatkan sendiri di frame pertama (tengah atas layar).
    if let (settings::DisplayMode::Panel, Some([x, y])) =
        (settings.display_mode, settings.window_pos)
    {
        viewport = viewport.with_position([x, y]);
    }

    #[allow(unused_mut)]
    let mut options = eframe::NativeOptions {
        viewport,
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };

    #[cfg(target_os = "macos")]
    {
        // App aksesori: tanpa ikon Dock dan tanpa menu bar app.
        options.event_loop_builder = Some(Box::new(|builder| {
            use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
            builder.with_activation_policy(ActivationPolicy::Accessory);
        }));
    }

    let result = eframe::run_native(
        "MiniLyrics",
        options,
        Box::new(move |cc| Ok(Box::new(ui::MiniLyricsApp::new(cc, paths, settings)))),
    );
    if let Err(e) = result {
        let msg = format!("MiniLyrics gagal jalan: {e}");
        log::error!("{msg}");
        fatal_dialog(&msg);
        std::process::exit(1);
    }
}

/// Sembunyikan `user:password@` di URL proxy sebelum ditulis ke log.
fn redact_userinfo(url: &str) -> String {
    match (url.find("://"), url.rfind('@')) {
        (Some(scheme), Some(at)) if at > scheme => {
            format!("{}://***@{}", &url[..scheme], &url[at + 1..])
        }
        (None, Some(at)) => format!("***@{}", &url[at + 1..]),
        _ => url.to_string(),
    }
}

/// Tampilkan error fatal ke user (release Windows tidak punya console).
fn fatal_dialog(msg: &str) {
    eprintln!("{msg}");
    #[cfg(windows)]
    unsafe {
        use windows::core::HSTRING;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        MessageBoxW(
            None,
            &HSTRING::from(msg),
            &HSTRING::from("MiniLyrics"),
            MB_OK | MB_ICONERROR,
        );
    }
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "display alert \"MiniLyrics\" message \"{}\"",
            msg.replace('\\', "\\\\").replace('"', "\\\"")
        );
        let _ = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", &script])
            .status();
    }
}

#[cfg(test)]
mod tests {
    use super::redact_userinfo;

    #[test]
    fn redacts_proxy_credentials() {
        assert_eq!(
            redact_userinfo("http://budi:rahasia@proxy.kantor:8080"),
            "http://***@proxy.kantor:8080"
        );
        assert_eq!(redact_userinfo("budi:x@proxy:3128"), "***@proxy:3128");
        assert_eq!(redact_userinfo("http://proxy:3128"), "http://proxy:3128");
    }
}
