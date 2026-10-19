//! Font fallback dari sistem. Font bawaan egui tidak punya glyph CJK dan
//! banyak simbol, jadi saat startup kita muat font sistem yang ada (tanpa
//! membundel font besar ke binary).
//!
//! Font UI sistem (Segoe UI / SF Pro) dipasang paling depan supaya teks Latin
//! memakai font sistem sesuai desain, bukan Ubuntu-Light bawaan egui.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use eframe::egui::epaint::text::VariationCoords;
use eframe::egui::{self, FontData, FontDefinitions, FontFamily};

/// Font UI utama punya sumbu `wght` (variable), jadi weight bisa diatur
/// langsung tanpa faux-bold.
static VARIABLE_WEIGHT: AtomicBool = AtomicBool::new(false);

/// Pergeseran faux-bold untuk weight 600 kalau font utama statis.
const FAUX_BOLD_AT_600: f32 = 0.7;

/// Font UI sistem: (nama, path). Yang pertama ada dipakai.
fn primary_ui() -> Vec<(&'static str, PathBuf)> {
    #[cfg(windows)]
    return vec![
        // Windows 11: Segoe UI Variable (punya sumbu wght).
        (
            "segoe-ui-variable",
            windows_fonts_dir().join("SegUIVar.ttf"),
        ),
        ("segoe-ui", windows_fonts_dir().join("segoeui.ttf")),
    ];
    #[cfg(target_os = "macos")]
    return vec![("sf-pro", PathBuf::from("/System/Library/Fonts/SFNS.ttf"))];
    #[allow(unreachable_code)]
    Vec::new()
}

/// Font monospace sistem untuk overlay debug.
fn primary_mono() -> Vec<(&'static str, PathBuf)> {
    #[cfg(windows)]
    return vec![("consolas", windows_fonts_dir().join("consola.ttf"))];
    #[cfg(target_os = "macos")]
    return vec![(
        "sf-mono",
        PathBuf::from("/System/Library/Fonts/SFNSMono.ttf"),
    )];
    #[allow(unreachable_code)]
    Vec::new()
}

#[cfg(windows)]
fn windows_fonts_dir() -> PathBuf {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts")
}

/// (nama, path, index face untuk .ttc)
fn candidates() -> Vec<(&'static str, PathBuf, u32)> {
    #[allow(unused_mut)]
    let mut list: Vec<(&'static str, PathBuf, u32)> = Vec::new();

    #[cfg(windows)]
    {
        let fonts = windows_fonts_dir();
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

/// Pasang font UI sistem + fallback. Font yang filenya tidak ada dilewati.
pub fn install(ctx: &egui::Context) {
    let mut defs = FontDefinitions::default();
    let mut loaded = Vec::new();
    let mut primary_paths = Vec::new();
    for (family, list) in [
        (FontFamily::Proportional, primary_ui()),
        (FontFamily::Monospace, primary_mono()),
    ] {
        let Some((name, path, bytes)) = list
            .into_iter()
            .find_map(|(name, path)| std::fs::read(&path).ok().map(|b| (name, path, b)))
        else {
            continue;
        };
        if family == FontFamily::Proportional {
            VARIABLE_WEIGHT.store(has_table(&bytes, b"fvar"), Ordering::Relaxed);
        }
        defs.font_data
            .insert(name.to_string(), Arc::new(FontData::from_owned(bytes)));
        defs.families
            .entry(family)
            .or_default()
            .insert(0, name.to_string());
        primary_paths.push(path);
        loaded.push(name);
    }
    for (name, path, index) in candidates() {
        if primary_paths.contains(&path) {
            continue;
        }
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
    log::info!(
        "font dimuat: {loaded:?} (weight variable: {})",
        VARIABLE_WEIGHT.load(Ordering::Relaxed)
    );
    ctx.set_fonts(defs);
}

/// Koordinat variasi untuk weight CSS (400, 500, 600, …). Font statis
/// mengabaikannya; untuk itu ada [`faux_bold_px`].
pub fn weight_coords(weight: f32) -> VariationCoords {
    VariationCoords::new([(b"wght", weight)])
}

/// Kalau font utama statis, weight > 400 ditiru dengan menggambar teks dua
/// kali bergeser sejauh nilai ini (px). 0 untuk font variable.
pub fn faux_bold_px(weight: f32) -> f32 {
    if VARIABLE_WEIGHT.load(Ordering::Relaxed) {
        0.0
    } else {
        ((weight - 400.0) / 200.0 * FAUX_BOLD_AT_600).max(0.0)
    }
}

/// Cek tabel di direktori tabel font sfnt (.ttf/.otf, bukan .ttc).
fn has_table(font: &[u8], tag: &[u8; 4]) -> bool {
    let Some(count) = font.get(4..6).map(|b| u16::from_be_bytes([b[0], b[1]])) else {
        return false;
    };
    (0..count as usize).any(|i| font.get(12 + i * 16..16 + i * 16) == Some(tag.as_slice()))
}
