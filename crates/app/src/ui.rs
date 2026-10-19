//! UI lirik melayang (egui).

use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, vec2, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Sense,
    ViewportCommand,
};
use mini_lyrics_core::karaoke;
use mini_lyrics_core::lrclib::LrclibClient;
use mini_lyrics_core::{FetchOutcome, LyricsController, LyricsProvider, LyricsStatus, TrackQuery};

use crate::now_playing::{self, SharedNowPlaying};
use crate::os_window;
use crate::settings::{Paths, Settings};
use crate::tray::{self, Tray, TrayAction};

pub const WIDTH: f32 = 560.0;
pub const LYRICS_HEIGHT: f32 = 120.0;
pub const DEBUG_HEIGHT: f32 = 118.0;

const HIGHLIGHT: Color32 = Color32::from_rgb(0xFF, 0xD6, 0x4D);
const ACTIVE_SIZE: f32 = 22.0;
const SIDE_SIZE: f32 = 14.0;
const PANEL_ALPHA: f32 = 0.55;

pub fn window_size(debug: bool) -> egui::Vec2 {
    vec2(
        WIDTH,
        LYRICS_HEIGHT + if debug { DEBUG_HEIGHT } else { 0.0 },
    )
}

pub fn version_label() -> String {
    format!(
        "MiniLyrics v{} ({})",
        env!("CARGO_PKG_VERSION"),
        env!("MINI_LYRICS_COMMIT")
    )
}

/// Provider LRCLIB yang dibuat malas: kalau pembuatan HTTP client gagal, app
/// tetap jalan dan menampilkan status error jaringan.
struct Lrclib(Result<LrclibClient, String>);

impl LyricsProvider for Lrclib {
    fn fetch(&self, query: &TrackQuery) -> FetchOutcome {
        match &self.0 {
            Ok(c) => c.fetch(query),
            Err(e) => FetchOutcome::NetworkError(e.clone()),
        }
    }
}

pub struct MiniLyricsApp {
    paths: Paths,
    settings: Settings,
    now_playing: SharedNowPlaying,
    lyrics: LyricsController,

    tray: Option<Tray>,
    tray_tried: bool,

    /// Jumlah frame lagi sampai penyesuaian window native diterapkan ulang.
    os_apply_in: Option<u8>,
    applied_passthrough: Option<bool>,
    applied_debug_size: Option<bool>,
    /// Posisi window sudah diatur (dari setting atau default).
    positioned: bool,
    position_attempts: u32,
    last_moved: Option<Instant>,
}

impl MiniLyricsApp {
    pub fn new(cc: &eframe::CreationContext<'_>, paths: Paths, settings: Settings) -> Self {
        crate::fonts::install(&cc.egui_ctx);

        let ctx = cc.egui_ctx.clone();
        let now_playing = now_playing::spawn(move || ctx.request_repaint());

        let ctx = cc.egui_ctx.clone();
        let repaint = move || ctx.request_repaint();
        let lyrics = if now_playing::demo::enabled() {
            log::info!("mode demo aktif (MINI_LYRICS_DEMO)");
            LyricsController::new(now_playing::demo::Provider, repaint)
        } else {
            let client = LrclibClient::new(env!("CARGO_PKG_VERSION"));
            if let Err(e) = &client {
                log::error!("{e}");
            }
            LyricsController::new(Lrclib(client), repaint)
        };

        Self {
            positioned: settings.window_pos.is_some(),
            paths,
            settings,
            now_playing,
            lyrics,
            tray: None,
            tray_tried: false,
            os_apply_in: Some(0),
            applied_passthrough: None,
            applied_debug_size: None,
            position_attempts: 0,
            last_moved: None,
        }
    }

    fn save_settings(&self) {
        self.settings.save(&self.paths.config_file);
    }

    fn handle_tray(&mut self, ctx: &egui::Context) {
        if !self.tray_tried {
            // Frame pertama: event loop sudah berjalan, aman membuat tray.
            self.tray_tried = true;
            match Tray::new(&version_label(), &self.settings) {
                Ok(t) => self.tray = Some(t),
                Err(e) => log::warn!("{e}"),
            }
        }
        let Some(tray) = &self.tray else {
            return;
        };
        let actions = tray.poll();
        if actions.is_empty() {
            return;
        }
        let mut changed = false;
        for action in actions {
            log::info!("menu tray: {action:?}");
            match action {
                TrayAction::Quit => {
                    self.save_settings();
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                TrayAction::OpenLogFolder => os_window::open_folder(&self.paths.log_dir),
                other => changed |= tray::apply_to_settings(other, &mut self.settings),
            }
        }
        if changed {
            tray.sync(&self.settings);
            self.save_settings();
        }
    }

    fn sync_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        // JANGAN pakai ViewportCommand::Visible(false): update() bisa berhenti
        // dipanggil sehingga menu tray tidak terbaca. "Sembunyi" = tidak
        // menggambar apa pun + mouse passthrough.
        let passthrough = self.settings.hidden || self.settings.click_through;
        if self.applied_passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.applied_passthrough = Some(passthrough);
            // Di Windows winit menulis ulang extended style; terapkan lagi.
            self.os_apply_in = Some(2);
        }

        if self.applied_debug_size != Some(self.settings.debug) {
            let size = window_size(self.settings.debug);
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            // Overlay debug memanjangkan window ke bawah; geser ke atas kalau
            // jadi keluar layar (hanya bisa dicek di monitor utama).
            let (outer, monitor) =
                ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
            if let (Some(outer), Some(m)) = (outer, monitor) {
                let on_primary =
                    (0.0..m.x).contains(&outer.min.x) && (0.0..m.y).contains(&outer.min.y);
                if self.applied_debug_size.is_some() && on_primary && outer.min.y + size.y > m.y {
                    let y = (m.y - size.y - 40.0).max(0.0);
                    ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(outer.min.x, y)));
                }
            }
            self.applied_debug_size = Some(self.settings.debug);
        }

        if let Some(n) = self.os_apply_in {
            if n == 0 {
                if let Some(w) = frame.winit_window() {
                    os_window::apply(w.as_ref());
                }
                self.os_apply_in = None;
            } else {
                self.os_apply_in = Some(n - 1);
            }
        }

        let (outer, monitor, pointer_down) = ctx.input(|i| {
            (
                i.viewport().outer_rect,
                i.viewport().monitor_size,
                i.pointer.any_down(),
            )
        });

        if !self.positioned {
            // Default: tengah bawah monitor utama.
            self.position_attempts += 1;
            if let Some(m) = monitor.filter(|m| m.x > 0.0 && m.y > 0.0) {
                let size = window_size(self.settings.debug);
                let pos = pos2((m.x - size.x) / 2.0, m.y - size.y - 90.0);
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos));
                self.settings.window_pos = Some([pos.x, pos.y]);
                self.positioned = true;
                self.save_settings();
            } else if self.position_attempts > 30 {
                self.positioned = true;
            }
            return;
        }

        // Simpan posisi setelah window selesai di-drag.
        if let Some(outer) = outer {
            let cur = [outer.min.x, outer.min.y];
            let moved = self
                .settings
                .window_pos
                .is_none_or(|p| (p[0] - cur[0]).abs() > 0.5 || (p[1] - cur[1]).abs() > 0.5);
            if moved {
                let since = *self.last_moved.get_or_insert_with(Instant::now);
                if !pointer_down && since.elapsed() > Duration::from_millis(400) {
                    self.settings.window_pos = Some(cur);
                    self.last_moved = None;
                    self.save_settings();
                }
            } else {
                self.last_moved = None;
            }
        }
    }
}

impl eframe::App for MiniLyricsApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        // Transparan penuh di luar panel.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.handle_tray(ctx);
        self.sync_window(ctx, frame);

        let player = self.now_playing.lock().ok().and_then(|s| s.player.clone());
        self.lyrics.update(player.as_ref(), Instant::now());

        let playing = player.as_ref().is_some_and(|p| p.is_playing);
        let animate = playing && !self.settings.hidden;
        ctx.request_repaint_after(Duration::from_millis(if animate { 16 } else { 250 }));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.settings.hidden {
            return;
        }
        let now = Instant::now();
        let np = self
            .now_playing
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default();
        let player = np.player.as_ref();
        let view = self.lyrics.view();

        let full = ui.ctx().content_rect();
        let panel = Rect::from_min_size(full.min, vec2(full.width(), LYRICS_HEIGHT)).shrink(2.0);
        let painter = ui.painter().with_clip_rect(panel);

        let paused = player.is_some_and(|p| !p.is_playing);
        let dim = if paused { 0.5 } else { 1.0 };
        painter.rect_filled(
            panel,
            CornerRadius::same(14),
            Color32::from_black_alpha((PANEL_ALPHA * 255.0) as u8),
        );

        // Drag: tekan tombol kiri di panel → biarkan OS memindahkan window.
        let resp = ui.interact(
            panel,
            ui.scope_id().with("drag-panel"),
            Sense::click_and_drag(),
        );
        if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let cx = panel.center().x;
        let max_w = panel.width() - 28.0;
        let y_prev = panel.min.y + 22.0;
        let y_cur = panel.center().y;
        let y_next = panel.max.y - 22.0;
        let side = |painter: &Painter, y: f32, text: &str, alpha: f32| {
            draw_line(
                painter,
                pos2(cx, y),
                max_w,
                text,
                SIDE_SIZE,
                white(alpha * dim),
            );
        };
        let song_line = player.map(|p| {
            if p.artist.is_empty() {
                p.title.clone()
            } else {
                format!("{} — {}", p.title, p.artist)
            }
        });

        match (player, &view.status) {
            (None, _) => {
                draw_line(
                    &painter,
                    pos2(cx, y_cur),
                    max_w,
                    "Putar lagu dulu ♪",
                    18.0,
                    white(0.85),
                );
            }
            (Some(p), LyricsStatus::Ready(lines)) => {
                let pos = p.position_at(now) + self.settings.offset_secs;
                let f = karaoke::frame_at(lines, pos);
                if let Some(prev) = f.prev {
                    side(&painter, y_prev, prev, 0.45);
                }
                match f.current {
                    Some(text) if !text.trim().is_empty() => {
                        draw_karaoke(&painter, pos2(cx, y_cur), max_w, text, f.progress, dim);
                    }
                    _ => {
                        let label = if f.current.is_none() {
                            song_line.clone().unwrap_or_else(|| "♪".into())
                        } else {
                            "♪".into()
                        };
                        draw_line(
                            &painter,
                            pos2(cx, y_cur),
                            max_w,
                            &label,
                            18.0,
                            white(0.7 * dim),
                        );
                    }
                }
                if let Some(next) = f.next {
                    side(&painter, y_next, next, 0.7);
                }
            }
            (Some(_), status) => {
                let msg = match status {
                    LyricsStatus::NotFound => "Lirik sinkron nggak ketemu",
                    LyricsStatus::NetworkError(_) => "Gagal konek ke server lirik",
                    _ => "Nyari lirik…",
                };
                draw_line(
                    &painter,
                    pos2(cx, y_cur - 10.0),
                    max_w,
                    msg,
                    18.0,
                    white(0.85 * dim),
                );
                if let Some(s) = &song_line {
                    side(&painter, y_cur + 18.0, s, 0.55);
                }
            }
        }

        if self.settings.debug {
            self.draw_debug(ui, &np, now);
        }
    }

    fn save(&mut self, _storage: &mut dyn eframe::Storage) {
        self.save_settings();
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        log::info!("keluar");
        self.save_settings();
    }
}

impl MiniLyricsApp {
    fn draw_debug(&self, ui: &egui::Ui, np: &now_playing::NowPlaying, now: Instant) {
        let full = ui.ctx().content_rect();
        let rect = Rect::from_min_max(
            pos2(full.min.x + 2.0, full.min.y + LYRICS_HEIGHT + 2.0),
            full.max - vec2(2.0, 2.0),
        );
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, CornerRadius::same(8), Color32::from_black_alpha(200));

        let view = self.lyrics.view();
        let mut lines = vec![version_label()];
        match &np.player {
            Some(p) => {
                let interp = p.position_at(now);
                lines.push(format!("src: {}", p.source_app));
                lines.push(format!("lagu: {} — {} [{}]", p.title, p.artist, p.album));
                lines.push(format!(
                    "pos: mentah {:.2}s | interp {:.2}s | +offset {:.2}s | offset {:+.1}s | {}",
                    p.position_secs,
                    interp,
                    interp + self.settings.offset_secs,
                    self.settings.offset_secs,
                    if p.is_playing { "playing" } else { "paused" }
                ));
                lines.push(format!(
                    "umur sampel: {:.1}s | durasi {:.1}s | poll {} ms",
                    p.sample_age(now).as_secs_f64(),
                    p.duration_secs,
                    np.poll_duration.map_or(0, |d| d.as_millis())
                ));
            }
            None => lines.push(format!(
                "player: - | poll {} ms",
                np.poll_duration.map_or(0, |d| d.as_millis())
            )),
        }
        let mut lyr = format!(
            "lirik: {} | latency LRCLIB {} | percobaan {}",
            view.status.label(),
            view.latency
                .map_or("-".to_string(), |d| format!("{} ms", d.as_millis())),
            view.attempts
        );
        if let Some(at) = view.retry_at {
            lyr.push_str(&format!(
                " | retry {:.0}s",
                at.saturating_duration_since(now).as_secs_f64()
            ));
        }
        lines.push(lyr);
        if let LyricsStatus::NetworkError(e) = &view.status {
            lines.push(format!("net: {e}"));
        }
        if let Some(e) = &np.last_error {
            lines.push(format!("now-playing: {e}"));
        }

        let font = FontId::monospace(10.5);
        let mut y = rect.min.y + 6.0;
        for line in lines {
            let g = painter.layout(
                line,
                font.clone(),
                Color32::from_gray(225),
                rect.width() - 16.0,
            );
            let h = g.size().y;
            painter.galley(pos2(rect.min.x + 8.0, y), g, Color32::WHITE);
            y += h + 1.0;
        }
    }
}

fn white(alpha: f32) -> Color32 {
    Color32::from_white_alpha((alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

fn with_alpha(c: Color32, alpha: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (alpha.clamp(0.0, 1.0) * 255.0) as u8)
}

/// Ukuran font supaya teks muat di `max_w` (minimal 60% dari ukuran asli).
fn fitted_size(painter: &Painter, text: &str, size: f32, max_w: f32) -> f32 {
    let w = painter
        .layout_no_wrap(text.to_string(), FontId::proportional(size), Color32::WHITE)
        .size()
        .x;
    if w > max_w {
        (size * max_w / w).max(size * 0.6)
    } else {
        size
    }
}

fn draw_line(painter: &Painter, center: Pos2, max_w: f32, text: &str, size: f32, color: Color32) {
    let size = fitted_size(painter, text, size, max_w);
    let font = FontId::proportional(size);
    let shadow = Color32::from_black_alpha(color.a() / 2);
    let g = painter.layout_no_wrap(text.to_string(), font.clone(), shadow);
    let rect = Align2::CENTER_CENTER.anchor_size(center, g.size());
    painter.galley(rect.min + vec2(1.0, 1.0), g, shadow);
    let g = painter.layout_no_wrap(text.to_string(), font, color);
    painter.galley(rect.min, g, color);
}

/// Baris aktif dengan efek karaoke: teks putih redup, lalu teks kuning yang
/// di-clip dari kiri selebar `progress`.
fn draw_karaoke(painter: &Painter, center: Pos2, max_w: f32, text: &str, progress: f32, dim: f32) {
    let size = fitted_size(painter, text, ACTIVE_SIZE, max_w);
    let font = FontId::proportional(size);
    let base_color = white(0.55 * dim);
    let fill_color = with_alpha(HIGHLIGHT, dim);
    let shadow_color = Color32::from_black_alpha((160.0 * dim) as u8);

    let shadow = painter.layout_no_wrap(text.to_string(), font.clone(), shadow_color);
    let rect = Align2::CENTER_CENTER.anchor_size(center, shadow.size());
    painter.galley(rect.min + vec2(1.5, 1.5), shadow, shadow_color);

    // egui tidak punya varian bold untuk font default; tebalkan dengan
    // menggambar dua kali bergeser sedikit.
    let bold = vec2(0.7, 0.0);
    let base = painter.layout_no_wrap(text.to_string(), font.clone(), base_color);
    painter.galley(rect.min, base.clone(), base_color);
    painter.galley(rect.min + bold, base, base_color);

    if progress > 0.0 {
        let clip = Rect::from_min_max(
            pos2(rect.min.x - 2.0, rect.min.y - 4.0),
            pos2(
                rect.min.x + rect.width() * progress + bold.x,
                rect.max.y + 4.0,
            ),
        )
        .intersect(painter.clip_rect());
        let p = painter.with_clip_rect(clip);
        let fill = p.layout_no_wrap(text.to_string(), font, fill_color);
        p.galley(rect.min, fill.clone(), fill_color);
        p.galley(rect.min + bold, fill, fill_color);
    }
}
