//! Font fallback dari sistem. Font bawaan egui tidak punya glyph CJK dan
//! banyak simbol, jadi saat startup kita muat font sistem yang ada (tanpa
//! membundel font besar ke binary).

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// (nama, path, index face untuk .ttc)
fn candidates() -> Vec<(&'static str, PathBuf, u32)> {
    #[allow(unused_mut)]
    let mut list: Vec<(&'static str, PathBuf, u32)> = Vec::new();

    #[cfg(windows)]
    {
        let fonts = std::env::var_os("WINDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
            .join("Fonts");
        list.push(("segoe-ui", fonts.join("segoeui.ttf"), 0));
        list.push(("segoe-ui-symbol", fonts.join("seguisym.ttf"), 0));
        // Jepang: Yu Gothic, kalau tidak ada pakai Meiryo.
        let yu = fonts.join("YuGothR.ttc");
        if yu.exists() {
            list.push(("yu-gothic", yu, 0));
        } else {
            list.push(("meiryo", fonts.join("meiryo.ttc"), 0));
        }
        list.push(("malgun-gothic", fonts.join("malgun.ttf"), 0));
        list.push(("microsoft-yahei", fonts.join("msyh.ttc"), 0));
    }

    #[cfg(target_os = "macos")]
    {
        let sys = PathBuf::from("/System/Library/Fonts");
        let hiragino = [
            sys.join("ヒラギノ角ゴシック W3.ttc"),
            sys.join("Hiragino Sans GB.ttc"),
        ];
        if let Some(p) = hiragino.into_iter().find(|p| p.exists()) {
            list.push(("hiragino-sans", p, 0));
        }
        list.push(("apple-sd-gothic-neo", sys.join("AppleSDGothicNeo.ttc"), 0));
        let pingfang = [
            sys.join("PingFang.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/PingFang.ttc"),
            PathBuf::from("/Library/Fonts/PingFang.ttc"),
        ];
        if let Some(p) = pingfang.into_iter().find(|p| p.exists()) {
            list.push(("pingfang", p, 0));
        }
        list.push(("apple-symbols", sys.join("Apple Symbols.ttf"), 0));
    }

    list
}

/// Pasang font fallback. Font yang filenya tidak ada dilewati.
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut loaded = Vec::new();
    for (name, path, index) in candidates() {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let mut data = FontData::from_owned(bytes);
                data.index = index;
                defs.font_data.insert(name.to_string(), Arc::new(data));
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    defs.families
                        .entry(family)
                        .or_default()
                        .push(name.to_string());
                }
                loaded.push(name);
            }
            Err(_) => log::debug!("font tidak ada: {}", path.display()),
        }
    }
    log::info!("font fallback dimuat: {loaded:?}");
    ctx.set_fonts(defs);
}
