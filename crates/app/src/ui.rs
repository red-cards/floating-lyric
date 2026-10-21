//! UI lirik melayang (egui). Gaya visual mode "Panel bawah" mengikuti arah
//! desain 1a · Kalem; semua tokennya ada di [`crate::theme`].

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, pos2, text::LayoutJob, vec2, Align2, Color32, CornerRadius, FontId, Galley, Painter,
    Pos2, Rect, Sense, Shadow, Stroke, StrokeKind, TextFormat, ViewportCommand,
};
use mini_lyrics_core::karaoke;
use mini_lyrics_core::lrclib::LrclibClient;
use mini_lyrics_core::{
    Alternatives, FetchOutcome, LyricLine, LyricsCandidate, LyricsController, LyricsProvider,
    LyricsStatus, LyricsView, PlayerState, TrackQuery,
};

use crate::fonts;
use crate::notch::{self, Kind};
use crate::now_playing::{self, SharedNowPlaying};
use crate::os_window;
use crate::settings::{DisplayMode, Paths, Settings};
use crate::theme::{self, TextStyle, Tint};
use crate::tray::{self, Tray, TrayAction};

/// Seberapa sering posisi/layar target mode notch dicek ulang.
const TOP_RECHECK: Duration = Duration::from_secs(2);
/// Seberapa sering style window native dicek & diterapkan ulang (murah:
/// hanya ditulis kalau berbeda).
const OS_REAPPLY: Duration = Duration::from_secs(1);

/// Tata letak window yang sedang diterapkan.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Layout {
    Panel { debug: bool },
    Notch { kind: Kind, debug: bool },
}

/// Ukuran window mode panel: kotak panel + margin transparan untuk shadow,
/// ditambah area overlay debug di bawahnya kalau aktif.
pub fn window_size(debug: bool) -> egui::Vec2 {
    vec2(
        theme::PANEL_SIZE.x + 2.0 * theme::WINDOW_MARGIN_X,
        theme::WINDOW_MARGIN_TOP
            + theme::PANEL_SIZE.y
            + theme::WINDOW_MARGIN_BOTTOM
            + if debug { theme::DEBUG_HEIGHT } else { 0.0 },
    )
}

/// Kotak panel di dalam window.
fn panel_rect(origin: Pos2) -> Rect {
    Rect::from_min_size(
        origin + vec2(theme::WINDOW_MARGIN_X, theme::WINDOW_MARGIN_TOP),
        theme::PANEL_SIZE,
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

    fn alternatives(&self, query: &TrackQuery) -> Result<Vec<LyricsCandidate>, String> {
        match &self.0 {
            Ok(c) => c.alternatives(query),
            Err(e) => Err(e.clone()),
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
    /// Kapan penyesuaian window native terakhir diterapkan. Diulang berkala
    /// karena winit bisa menulis ulang style window (mis. bingkai Windows).
    last_os_apply: Option<Instant>,
    applied_passthrough: Option<bool>,
    applied_layout: Option<Layout>,
    /// Bentuk mode notch (notch asli atau pill), dicek ulang berkala.
    notch_kind: Kind,
    last_top_check: Option<Instant>,
    /// Posisi kursor relatif window (dibaca global, karena saat passthrough
    /// event mouse tidak sampai ke window).
    cursor: Option<Pos2>,
    /// Jangan simpan posisi panel sampai waktu ini (habis ganti layout).
    suppress_pos_save_until: Option<Instant>,
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
            last_os_apply: None,
            applied_passthrough: None,
            applied_layout: None,
            notch_kind: Kind::Pill,
            last_top_check: None,
            cursor: None,
            suppress_pos_save_until: None,
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
        self.apply_actions(ctx, actions, "menu tray");
    }

    /// Terapkan aksi dari menu tray atau tombol di panel.
    fn apply_actions(&mut self, ctx: &egui::Context, actions: Vec<TrayAction>, from: &str) {
        let mut changed = false;
        for action in actions {
            log::info!("{from}: {action:?}");
            match action {
                TrayAction::Quit => {
                    self.save_settings();
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
                TrayAction::OpenLogFolder => os_window::open_folder(&self.paths.log_dir),
                TrayAction::NextLyrics => self.lyrics.cycle_alternative(1),
                TrayAction::PrevLyrics => self.lyrics.cycle_alternative(-1),
                other => changed |= tray::apply_to_settings(other, &mut self.settings),
            }
        }
        if changed {
            if let Some(tray) = &self.tray {
                tray.sync(&self.settings);
            }
            self.save_settings();
        }
    }

    fn sync_window(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        // JANGAN pakai ViewportCommand::Visible(false): update() bisa berhenti
        // dipanggil sehingga menu tray tidak terbaca. "Sembunyi" = tidak
        // menggambar apa pun + mouse passthrough.
        let passthrough = self.settings.hidden || self.settings.click_through();
        if self.applied_passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.applied_passthrough = Some(passthrough);
            // Di Windows winit menulis ulang extended style; terapkan lagi.
            self.os_apply_in = Some(2);
        }

        match self.settings.display_mode {
            DisplayMode::Panel => self.sync_panel_layout(ctx),
            DisplayMode::Notch => self.sync_notch_layout(frame),
        }

        let periodic_due = self
            .last_os_apply
            .is_some_and(|t| t.elapsed() >= OS_REAPPLY);
        if periodic_due && self.os_apply_in.is_none() {
            self.os_apply_in = Some(0);
        }
        if let Some(n) = self.os_apply_in {
            if n == 0 {
                if let Some(w) = frame.winit_window() {
                    // Lebih dulu: winit menulis ulang style saat flag shadow
                    // berubah, jadi style kita diterapkan sesudahnya.
                    os_window::disable_undecorated_shadow(w.as_ref());
                    os_window::apply(w.as_ref());
                }
                self.os_apply_in = None;
                self.last_os_apply = Some(Instant::now());
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

        if self.settings.display_mode != DisplayMode::Panel {
            return;
        }
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
        if self
            .suppress_pos_save_until
            .is_some_and(|until| Instant::now() < until)
        {
            return;
        }
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

    fn sync_panel_layout(&mut self, ctx: &egui::Context) {
        let layout = Layout::Panel {
            debug: self.settings.debug,
        };
        if self.applied_layout == Some(layout) {
            return;
        }
        let size = window_size(self.settings.debug);
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        match self.applied_layout {
            // Kembali dari mode notch: pulihkan posisi panel tersimpan.
            Some(Layout::Notch { .. }) => match self.settings.window_pos {
                Some([x, y]) => ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(x, y))),
                None => {
                    self.positioned = false;
                    self.position_attempts = 0;
                }
            },
            // Debug on/off: overlay memanjangkan window ke bawah; geser ke
            // atas kalau jadi keluar layar (hanya bisa dicek di monitor utama).
            Some(Layout::Panel { .. }) => {
                let (outer, monitor) =
                    ctx.input(|i| (i.viewport().outer_rect, i.viewport().monitor_size));
                if let (Some(outer), Some(m)) = (outer, monitor) {
                    let on_primary =
                        (0.0..m.x).contains(&outer.min.x) && (0.0..m.y).contains(&outer.min.y);
                    if on_primary && outer.min.y + size.y > m.y {
                        let y = (m.y - size.y - 40.0).max(0.0);
                        ctx.send_viewport_cmd(ViewportCommand::OuterPosition(pos2(outer.min.x, y)));
                    }
                }
            }
            None => {}
        }
        self.suppress_pos_save_until = Some(Instant::now() + Duration::from_secs(1));
        self.applied_layout = Some(layout);
    }

    fn sync_notch_layout(&mut self, frame: &eframe::Frame) {
        let due = self
            .last_top_check
            .is_none_or(|t| t.elapsed() >= TOP_RECHECK);
        let was_notch = matches!(self.applied_layout, Some(Layout::Notch { .. }));
        if !due && was_notch {
            if let Some(Layout::Notch { debug, .. }) = self.applied_layout {
                if debug == self.settings.debug {
                    return;
                }
            }
        }
        self.last_top_check = Some(Instant::now());

        // Layar bisa berganti (monitor dicolok/dicabut), jadi cek ulang.
        let kind = Kind::from_metrics(notch::fake_from_env().or_else(os_window::notch_metrics));
        if kind != self.notch_kind || !was_notch {
            log::info!("mode notch: {kind:?}");
        }
        self.notch_kind = kind;
        let layout = Layout::Notch {
            kind,
            debug: self.settings.debug,
        };
        let size = kind.window_size(self.notch_debug_height());
        if let Some(w) = frame.winit_window() {
            os_window::place_top(w.as_ref(), size, kind.top_margin());
        }
        if self.applied_layout != Some(layout) {
            // Level/collection behavior dipastikan lagi setelah pindah.
            self.os_apply_in = Some(1);
        }
        self.applied_layout = Some(layout);
    }

    /// Tinggi tambahan panel notch saat mode debug (overlay ada di dalam
    /// bentuk hitam, di bawah baris lirik).
    fn notch_debug_height(&self) -> f32 {
        if self.settings.debug {
            theme::NOTCH_DEBUG_HEIGHT
        } else {
            0.0
        }
    }

    /// Efek redup saat kursor di atas panel hanya berlaku kalau event mouse
    /// tidak diterima (click-through) atau di mode notch.
    fn dims_on_hover(&self) -> bool {
        self.settings.display_mode == DisplayMode::Notch || self.settings.click_through()
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

        self.cursor = if self.dims_on_hover() && !self.settings.hidden {
            frame
                .winit_window()
                .and_then(|w| os_window::cursor_in_window(w.as_ref()))
        } else {
            None
        };

        let playing = player.as_ref().is_some_and(|p| p.is_playing);
        let idle = if self.dims_on_hover() {
            // Kursor dicek dengan polling, jadi jangan terlalu jarang.
            100
        } else {
            250
        };
        let animate = playing && !self.settings.hidden;
        ctx.request_repaint_after(Duration::from_millis(if animate { 16 } else { idle }));
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
        let full = ui.ctx().content_rect();
        match self.settings.display_mode {
            DisplayMode::Panel => {
                let actions = self.ui_panel(ui, &np, now);
                if !actions.is_empty() {
                    self.apply_actions(ui.ctx(), actions, "tombol panel");
                }
                if self.settings.debug {
                    // Overlay debug: kotak terpisah tepat di bawah panel.
                    let panel = panel_rect(full.min);
                    let column = Rect::from_x_y_ranges(panel.x_range(), panel.max.y..=full.max.y);
                    self.draw_debug(ui, &np, now, column);
                }
            }
            DisplayMode::Notch => {
                // Overlay debug mode notch digambar di dalam bentuknya.
                let actions = self.ui_notch(ui, &np, now);
                if !actions.is_empty() {
                    self.apply_actions(ui.ctx(), actions, "tombol notch");
                }
            }
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

/// Isi panel yang sedang tampil. Saat berganti, isi baru fade-in.
#[derive(Debug, Clone, PartialEq)]
enum Content {
    Idle,
    Loading,
    NotFound,
    NetworkError,
    Lyrics { track: Option<String> },
}

/// Ikon kecil di depan judul status.
#[derive(Debug, Clone, Copy, PartialEq)]
enum StatusIcon {
    None,
    Loading,
    NotFound,
    Error,
}

impl MiniLyricsApp {
    /// Opacity tambahan dari efek redup saat kursor di atas `area`.
    fn hover_alpha(&self, ui: &egui::Ui, area: Rect) -> f32 {
        let hovered = self.cursor.is_some_and(|c| area.contains(c));
        let t = ui.ctx().animate_bool_with_time(
            ui.scope_id().with("hover-dim"),
            hovered,
            theme::HOVER_FADE,
        );
        lerp(1.0, theme::HOVER_OPACITY, t)
    }

    /// Mode "Panel bawah" (1a · Kalem): kotak gelap 3 baris, bisa di-drag.
    fn ui_panel(
        &self,
        ui: &egui::Ui,
        np: &now_playing::NowPlaying,
        now: Instant,
    ) -> Vec<TrayAction> {
        let player = np.player.as_ref();
        let view = self.lyrics.view();
        let ctx = ui.ctx();
        let panel = panel_rect(ctx.content_rect().min);

        // Paused meredupkan seluruh panel (latar + isi), seperti opacity
        // CSS di desain.
        let paused = player.is_some_and(|p| !p.is_playing);
        let pause_t =
            ctx.animate_bool_with_time(ui.scope_id().with("paused"), paused, theme::PAUSE_FADE);
        let opacity = lerp(1.0, theme::PAUSED_OPACITY, pause_t) * self.hover_alpha(ui, panel);
        paint_panel(ui.painter(), panel, opacity);

        // Drag: tekan tombol kiri di panel → biarkan OS memindahkan window.
        let resp = ui.interact(
            panel,
            ui.scope_id().with("drag-panel"),
            Sense::click_and_drag(),
        );
        if resp.hovered() && ui.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }

        let content = match (player, &view.status) {
            (None, _) => Content::Idle,
            (Some(_), LyricsStatus::Ready(_)) => Content::Lyrics {
                track: view.track_key.clone(),
            },
            (Some(_), LyricsStatus::NotFound) => Content::NotFound,
            (Some(_), LyricsStatus::NetworkError(_)) => Content::NetworkError,
            (Some(_), _) => Content::Loading,
        };
        let fade = animation_t(ui, "content", content.clone(), now, theme::STATE_FADE);
        let alpha = opacity * fade;
        let painter = ui
            .painter()
            .with_clip_rect(panel.shrink(theme::PANEL_BORDER_WIDTH));

        match (player, &view.status) {
            (Some(p), LyricsStatus::Ready(lines)) => {
                self.draw_lyrics(ui, &painter, panel, p, lines, alpha, now)
            }
            _ => {
                let song = player.map(song_label).unwrap_or_default();
                let (icon, main, sub) = match content {
                    Content::Idle => (
                        StatusIcon::None,
                        "Putar lagu dulu ♪",
                        "Spotify lagi diam".to_string(),
                    ),
                    Content::NotFound => (StatusIcon::NotFound, "Lirik sinkron nggak ketemu", song),
                    Content::NetworkError => (
                        StatusIcon::Error,
                        "Gagal konek ke server lirik",
                        retry_label(view, now),
                    ),
                    _ => (StatusIcon::Loading, "Nyari lirik…", song),
                };
                draw_status(ui, &painter, panel, icon, main, &sub, alpha);
            }
        }

        if pause_t > 0.0 {
            paint_pause_bars(&painter, panel, opacity * pause_t);
        }

        self.panel_controls(ui, panel, view)
    }

    /// Kontrol saat hover: geser sinkron (kiri atas) dan pilih lirik lain
    /// (kanan atas). Hanya muncul kalau event mouse sampai ke window
    /// (click-through mati); selain itu panel tetap tanpa tombol.
    fn panel_controls(&self, ui: &egui::Ui, panel: Rect, view: &LyricsView) -> Vec<TrayAction> {
        let ctx = ui.ctx();
        let hovered = !self.settings.click_through()
            && ctx.pointer_hover_pos().is_some_and(|p| panel.contains(p));
        let t = ctx.animate_bool_with_time(
            ui.scope_id().with("controls"),
            hovered,
            theme::CONTROLS_FADE,
        );
        let mut actions = Vec::new();
        if t <= 0.01 {
            return actions;
        }
        let top = panel.min.y + theme::CONTROLS_TOP;
        let interactive = t > 0.5;

        if matches!(view.status, LyricsStatus::Ready(_)) {
            let offset = self.settings.offset_secs;
            let label = if offset == 0.0 {
                Control::Label("Sinkron".into())
            } else {
                // Klik label = reset; saat di-hover labelnya jadi "Reset".
                Control::Button(tray::offset_text(offset), TrayAction::ResetOffset)
            };
            let items = [
                Control::Icon(Icon::Minus, TrayAction::OffsetLater),
                label,
                Control::Icon(Icon::Plus, TrayAction::OffsetEarlier),
            ];
            let left = pos2(panel.min.x + theme::CONTROLS_INSET_X, top);
            control_group(
                ui,
                "offset",
                left,
                Align2::LEFT_TOP,
                &items,
                t,
                interactive,
                &mut actions,
            );
        }

        if matches!(view.status, LyricsStatus::Ready(_) | LyricsStatus::NotFound) {
            let items = match &view.alternatives {
                Alternatives::Unknown => {
                    vec![Control::Button("Lirik lain".into(), TrayAction::NextLyrics)]
                }
                Alternatives::Failed(_) => {
                    vec![Control::Button(
                        "Gagal, coba lagi".into(),
                        TrayAction::NextLyrics,
                    )]
                }
                Alternatives::Searching => vec![Control::Disabled("Nyari lirik lain…".into())],
                alt @ Alternatives::Ready { index, labels } if alt.has_others() => {
                    let pos = index.map_or("–".into(), |i| (i + 1).to_string());
                    vec![
                        Control::Icon(Icon::ChevronLeft, TrayAction::PrevLyrics),
                        Control::Label(format!("Lirik {pos}/{}", labels.len())),
                        Control::Icon(Icon::ChevronRight, TrayAction::NextLyrics),
                    ]
                }
                Alternatives::Ready { .. } => {
                    vec![Control::Disabled("Nggak ada lirik lain".into())]
                }
            };
            let right = pos2(panel.max.x - theme::CONTROLS_INSET_X, top);
            control_group(
                ui,
                "lyrics",
                right,
                Align2::RIGHT_TOP,
                &items,
                t,
                interactive,
                &mut actions,
            );
        }
        actions
    }

    /// Tiga baris lirik: sebelumnya, aktif (karaoke), berikutnya.
    #[allow(clippy::too_many_arguments)]
    fn draw_lyrics(
        &self,
        ui: &egui::Ui,
        painter: &Painter,
        panel: Rect,
        p: &PlayerState,
        lines: &[LyricLine],
        alpha: f32,
        now: Instant,
    ) {
        let pos = p.position_at(now) + self.settings.offset_secs;
        let f = karaoke::frame_at(lines, pos);

        // Pindah baris: semua baris naik sedikit ke posisinya sambil fade-in.
        let idx = karaoke::active_index(lines, pos);
        let t =
            egui::emath::easing::cubic_out(animation_t(ui, "line", idx, now, theme::LINE_SHIFT));
        let dy = theme::LINE_SHIFT_PX * (1.0 - t);
        let alpha = alpha * lerp(theme::LINE_SHIFT_FROM_ALPHA, 1.0, t);

        let (prev, active, next) = (theme::PREV_TEXT, theme::ACTIVE_TEXT, theme::NEXT_TEXT);
        let total =
            prev.line_height + active.line_height + next.line_height + 2.0 * theme::LINE_GAP;
        let top = panel.center().y - total / 2.0 + dy;
        let y_prev = top + prev.line_height / 2.0;
        let y_active = top + prev.line_height + theme::LINE_GAP + active.line_height / 2.0;
        let y_next = top + total - next.line_height / 2.0;
        let cx = panel.center().x;
        let max_w = panel.width() - 2.0 * theme::PANEL_PAD_X;

        if let Some(text) = f.prev {
            draw_text(
                painter,
                pos2(cx, y_prev),
                max_w,
                text,
                &prev,
                theme::PREV_COLOR,
                alpha,
            );
        }
        // Baris aktif menyala penuh begitu mulai (LRC hanya punya waktu per
        // baris, jadi tidak ada isian per kata yang bisa dipercaya).
        let lit = animation_t(ui, "highlight", idx, now, theme::HIGHLIGHT_FADE);
        match f.current {
            Some(text) if !text.trim().is_empty() => draw_highlight(
                painter,
                pos2(cx, y_active),
                max_w,
                text,
                &active,
                theme::ACTIVE_BASE.faded(alpha),
                theme::ACTIVE_FILL.faded(alpha),
                lit,
            ),
            // Sebelum baris pertama: judul — artis. Jeda instrumental: ♪.
            current => {
                let label = match current {
                    None => song_label(p),
                    Some(_) => "♪".into(),
                };
                let at = pos2(cx, y_active);
                draw_text(
                    painter,
                    at,
                    max_w,
                    &label,
                    &active,
                    theme::ACTIVE_BASE,
                    alpha,
                );
            }
        }
        if let Some(text) = f.next {
            draw_text(
                painter,
                pos2(cx, y_next),
                max_w,
                text,
                &next,
                theme::NEXT_COLOR,
                alpha,
            );
        }
    }

    /// Mode "Notch" (gaya D): bentuk hitam pekat yang tumbuh dari notch
    /// fisik (atau pill di tengah atas). Expanded: telinga judul/artis di
    /// samping kamera + satu baris lirik. Kursor mendekat: kempis ke
    /// collapsed (click-through aktif) atau isinya meredup (click-through
    /// mati, sekaligus memunculkan kontrol di telinga saat kursor di panel).
    fn ui_notch(
        &self,
        ui: &egui::Ui,
        np: &now_playing::NowPlaying,
        now: Instant,
    ) -> Vec<TrayAction> {
        let ctx = ui.ctx();
        let player = np.player.as_ref();
        let view = self.lyrics.view();
        let kind = self.notch_kind;
        let full = ctx.content_rect();
        let time = ui.input(|i| i.time);
        let debug_h = self.notch_debug_height();
        let painter = ui.painter().with_clip_rect(full);

        // Kursor dibaca global (event mouse tidak sampai saat passthrough).
        let expanded_body = kind.body_rect(full.min, full.width(), 1.0, debug_h);
        let zone = kind.hit_rect(expanded_body).expand(theme::NOTCH_NEAR_ZONE);
        let near = self.cursor.is_some_and(|c| zone.contains(c));
        let click_through = self.settings.click_through();

        let id = ui.scope_id().with("notch-state");
        let frame = ctx.data_mut(|d| {
            let st = d.get_temp_mut_or_insert_with(id, || NotchUi {
                transition: notch::Transition::new(true, time),
                last_near: None,
            });
            if near {
                st.last_near = Some(time);
            }
            // Click-through aktif: kursor mendekat → kempis; mekar lagi
            // 600 ms setelah kursor pergi. Click-through mati: tetap terbuka.
            let recently_near = st
                .last_near
                .is_some_and(|t| time - t < theme::NOTCH_REOPEN_DELAY as f64);
            let open = !(click_through && recently_near);
            st.transition.set(open, time);
            (st.transition.frame(time), recently_near && !near)
        });
        let (frame, waiting_reopen) = frame;
        if !frame.settled || waiting_reopen {
            // Animasi berjalan, atau menunggu jeda sebelum mekar lagi.
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        let body = kind.body_rect(full.min, full.width(), frame.shape, debug_h);
        notch::paint_shape(
            &painter,
            body,
            kind.bottom_radius(frame.shape),
            theme::NOTCH_BG.color(),
        );

        let playing = player.is_some_and(|p| p.is_playing);
        let paused = player.is_some_and(|p| !p.is_playing);

        // Isi collapsed (♪/❚❚ + equalizer) memudar saat isi expanded muncul.
        let collapsed_alpha = 1.0 - frame.content;
        if collapsed_alpha > 0.01 {
            self.paint_collapsed_marks(
                &painter,
                kind,
                body,
                playing,
                paused,
                time,
                collapsed_alpha,
            );
        }
        let mut actions = Vec::new();
        if frame.content <= 0.01 {
            return actions;
        }

        // Click-through mati: kursor di atas panel → kontrol di telinga;
        // kursor di sekitar panel → isi meredup.
        let inside = !click_through
            && ctx
                .pointer_hover_pos()
                .is_some_and(|p| kind.hit_rect(body).contains(p));
        let dim_t = ctx.animate_bool_with_time(
            ui.scope_id().with("notch-near"),
            !click_through && near && !inside,
            theme::HOVER_FADE,
        );
        let controls_t = ctx.animate_bool_with_time(
            ui.scope_id().with("notch-controls"),
            inside && matches!(kind, Kind::Notch(_)),
            theme::CONTROLS_FADE,
        );
        let base = frame.content * lerp(1.0, theme::NOTCH_NEAR_OPACITY, dim_t);
        // Lirik & teks telinga meredup ke 45% saat dijeda.
        let pause_t = ctx.animate_bool_with_time(
            ui.scope_id().with("notch-paused"),
            paused,
            theme::PAUSE_FADE,
        );
        let cop = base * lerp(1.0, theme::NOTCH_PAUSED_OPACITY, pause_t);

        // Telinga (hanya notch; pill tidak punya pita atas).
        if let (Kind::Notch(_), Some(p), LyricsStatus::Ready(_)) = (kind, player, &view.status) {
            let ears_alpha = 1.0 - controls_t;
            if ears_alpha > 0.01 {
                self.paint_ears(
                    &painter,
                    kind,
                    body,
                    p,
                    now,
                    time,
                    base * ears_alpha,
                    cop * ears_alpha,
                );
            }
        }
        if controls_t > 0.01 {
            self.ear_controls(
                ui,
                kind,
                body,
                view,
                base * controls_t,
                controls_t > 0.5,
                &mut actions,
            );
        }

        // Baris lirik / status di bawah pita kamera.
        let row = kind.row_rect(body);
        let row_painter = painter.with_clip_rect(row.intersect(body));
        let max_w = row.width() - 2.0 * theme::NOTCH_ROW_PAD_X;
        match (player, &view.status) {
            (Some(p), LyricsStatus::Ready(lines)) => {
                let pos = p.position_at(now) + self.settings.offset_secs;
                let f = karaoke::frame_at(lines, pos);
                let idx = karaoke::active_index(lines, pos);
                let lit = animation_t(ui, "notch-highlight", idx, now, theme::HIGHLIGHT_FADE);
                let style = theme::NOTCH_ACTIVE_TEXT;
                match f.current {
                    Some(text) if !text.trim().is_empty() => draw_highlight(
                        &row_painter,
                        row.center(),
                        max_w,
                        text,
                        &style,
                        theme::NOTCH_ACTIVE_BASE.faded(cop),
                        theme::NOTCH_ACTIVE_FILL.faded(cop),
                        lit,
                    ),
                    // Sebelum baris pertama: judul — artis. Jeda instrumental: ♪.
                    current => {
                        let label = match current {
                            None => song_label(p),
                            Some(_) => "♪".into(),
                        };
                        draw_text(
                            &row_painter,
                            row.center(),
                            max_w,
                            &label,
                            &style,
                            theme::NOTCH_ACTIVE_BASE,
                            cop,
                        );
                    }
                }
            }
            (player, status) => {
                let song = player.map(song_label).unwrap_or_default();
                let (icon, main, sub) = match (player, status) {
                    (None, _) => (
                        StatusIcon::None,
                        "Putar lagu dulu ♪",
                        "Spotify lagi diam".to_string(),
                    ),
                    (_, LyricsStatus::NotFound) => {
                        (StatusIcon::NotFound, "Lirik sinkron nggak ketemu", song)
                    }
                    (_, LyricsStatus::NetworkError(_)) => (
                        StatusIcon::Error,
                        "Gagal konek ke server lirik",
                        retry_label(view, now),
                    ),
                    _ => (StatusIcon::Loading, "Nyari lirik…", song),
                };
                draw_notch_status(ui, &row_painter, row, icon, main, &sub, base);
            }
        }

        if self.settings.debug && debug_h > 0.0 {
            let area = Rect::from_min_max(pos2(body.min.x, row.max.y), body.max);
            self.draw_notch_debug(&painter.with_clip_rect(body), area, np, now, base);
        }
        actions
    }

    /// ♪ (atau ❚❚ saat dijeda) di telinga kiri dan equalizer di telinga kanan
    /// bentuk collapsed. Pill: keduanya di tengah.
    #[allow(clippy::too_many_arguments)]
    fn paint_collapsed_marks(
        &self,
        painter: &Painter,
        kind: Kind,
        body: Rect,
        playing: bool,
        paused: bool,
        time: f64,
        alpha: f32,
    ) {
        let accent = theme::NOTCH_ACCENT.faded(alpha);
        let note = text_galley(
            painter,
            "♪",
            &TextStyle::plain(theme::NOTE_SIZE),
            theme::NOTE_SIZE,
            theme::NOTE_COLOR.faded(alpha),
        );
        let mark_w = if paused {
            notch::pause_width()
        } else {
            note.size().x
        };
        let eq_w = notch::equalizer_width();
        let heights = notch::equalizer_heights(playing, time);
        let (mark_right, eq_left, cy) = match kind {
            Kind::Notch(_) => {
                let (l, r) = kind.ears(body);
                (
                    l.max.x - theme::EAR_PAD_COLLAPSED,
                    r.min.x + theme::EAR_PAD_COLLAPSED,
                    l.center().y,
                )
            }
            Kind::Pill => {
                let total = mark_w + theme::PILL_COLLAPSED_GAP + eq_w;
                let x0 = body.center().x - total / 2.0;
                (
                    x0 + mark_w,
                    x0 + mark_w + theme::PILL_COLLAPSED_GAP,
                    body.center().y,
                )
            }
        };
        if paused {
            notch::paint_pause(painter, pos2(mark_right - mark_w, cy), accent);
        } else {
            let at = Align2::RIGHT_CENTER.anchor_size(pos2(mark_right, cy), note.size());
            painter.galley(at.min, note, theme::NOTE_COLOR.faded(alpha));
        }
        notch::paint_equalizer(
            painter,
            pos2(eq_left, cy + theme::EQ_MAX_HEIGHT / 2.0),
            heights,
            accent,
        );
    }

    /// Telinga expanded: kiri = [❚❚] (Dijeda · ) judul, kanan = artis ·
    /// posisi [equalizer]. `alpha` untuk ikon, `text_alpha` untuk teks (ikut
    /// meredup saat dijeda).
    #[allow(clippy::too_many_arguments)]
    fn paint_ears(
        &self,
        painter: &Painter,
        kind: Kind,
        body: Rect,
        p: &PlayerState,
        now: Instant,
        time: f64,
        alpha: f32,
        text_alpha: f32,
    ) {
        let (left, right) = kind.ears(body);
        let style = theme::EAR_TEXT;
        let color = theme::EAR_COLOR.faded(text_alpha);
        let accent = theme::NOTCH_ACCENT.faded(alpha);
        let galley = |text: &str| text_galley(painter, text, &style, style.size, color);

        // Kiri.
        let cy = left.center().y;
        let mut x = left.min.x + theme::EAR_PAD;
        let paused = !p.is_playing;
        if paused {
            notch::paint_pause(painter, pos2(x, cy), accent);
            x += notch::pause_width() + theme::EAR_GAP_LEFT;
        }
        let title = if paused {
            format!("Dijeda · {}", p.title)
        } else {
            p.title.clone()
        };
        let avail = left.max.x - theme::EAR_PAD - x;
        if avail > 8.0 {
            let text = ellipsize(&title, avail, galley);
            let g = galley(&text);
            let at = Align2::LEFT_CENTER.anchor_size(pos2(x, cy), g.size());
            paint_weighted(painter, at.min, g, style.weight, color);
        }

        // Kanan.
        let cy = right.center().y;
        let mut x_end = right.max.x - theme::EAR_PAD;
        if p.is_playing {
            let eq_w = notch::equalizer_width();
            notch::paint_equalizer(
                painter,
                pos2(x_end - eq_w, cy + theme::EQ_MAX_HEIGHT / 2.0),
                notch::equalizer_heights(true, time),
                accent,
            );
            x_end -= eq_w + theme::EAR_GAP_RIGHT;
        }
        let pos = minutes(p.position_at(now));
        let label = if p.artist.is_empty() {
            pos
        } else {
            format!("{} · {pos}", p.artist)
        };
        let avail = x_end - (right.min.x + theme::EAR_PAD);
        if avail > 8.0 {
            let text = ellipsize(&label, avail, galley);
            let g = galley(&text);
            let at = Align2::RIGHT_CENTER.anchor_size(pos2(x_end, cy), g.size());
            paint_weighted(painter, at.min, g, style.weight, color);
        }
    }

    /// Kontrol di telinga saat kursor di atas panel (click-through mati):
    /// kiri = − offset +, kanan = cari lirik lain.
    #[allow(clippy::too_many_arguments)]
    fn ear_controls(
        &self,
        ui: &egui::Ui,
        kind: Kind,
        body: Rect,
        view: &LyricsView,
        alpha: f32,
        interactive: bool,
        actions: &mut Vec<TrayAction>,
    ) {
        let painter = ui.painter();
        let (left, right) = kind.ears(body);
        let size = theme::EAR_BUTTON_SIZE;
        let gap = theme::EAR_BUTTON_GAP;
        let style = theme::EAR_TEXT;

        // Mengembalikan aksi kalau tombol diklik.
        let button =
            |key: &str, center: Pos2, icon: EarIcon, action: Option<TrayAction>, bg: Tint| {
                let mut clicked = None;
                let rect = Rect::from_center_size(center, vec2(size, size));
                let mut fill = bg;
                if let (Some(action), true) = (action, interactive) {
                    let resp = ui.interact(rect, ui.scope_id().with(("ear", key)), Sense::click());
                    if resp.hovered() {
                        fill = theme::EAR_BUTTON_HOVER_BG;
                    }
                    if resp.clicked() {
                        clicked = Some(action);
                    }
                }
                let a = if action.is_some() {
                    alpha
                } else {
                    alpha * theme::CONTROL_DISABLED_ALPHA
                };
                painter.circle_filled(rect.center(), size / 2.0, fill.faded(a));
                paint_ear_icon(
                    painter,
                    rect.center(),
                    icon,
                    theme::EAR_BUTTON_ICON.faded(a),
                );
                clicked
            };

        if matches!(view.status, LyricsStatus::Ready(_)) {
            let offset = self.settings.offset_secs;
            let label = tray::offset_text(offset);
            let accent = theme::NOTCH_ACCENT.faded(alpha);
            let g = text_galley(painter, &label, &style, style.size, accent);
            let label_w = g.size().x.max(theme::EAR_OFFSET_MIN_WIDTH);
            let total = 2.0 * size + 2.0 * gap + label_w;
            let cy = left.center().y;
            let x0 = left.center().x - total / 2.0;
            actions.extend(button(
                "later",
                pos2(x0 + size / 2.0, cy),
                EarIcon::Minus,
                Some(TrayAction::OffsetLater),
                theme::EAR_BUTTON_BG,
            ));
            let label_rect =
                Rect::from_min_size(pos2(x0 + size + gap, cy - size / 2.0), vec2(label_w, size));
            if offset != 0.0 && interactive {
                let resp = ui.interact(
                    label_rect,
                    ui.scope_id().with(("ear", "reset")),
                    Sense::click(),
                );
                if resp.clicked() {
                    actions.push(TrayAction::ResetOffset);
                }
            }
            let at = Align2::CENTER_CENTER.anchor_size(label_rect.center(), g.size());
            paint_weighted(painter, at.min, g, style.weight, accent);
            actions.extend(button(
                "earlier",
                pos2(x0 + total - size / 2.0, cy),
                EarIcon::Plus,
                Some(TrayAction::OffsetEarlier),
                theme::EAR_BUTTON_BG,
            ));
        }

        if matches!(view.status, LyricsStatus::Ready(_) | LyricsStatus::NotFound) {
            let action = match &view.alternatives {
                Alternatives::Searching => None,
                alt @ Alternatives::Ready { .. } if !alt.has_others() => None,
                _ => Some(TrayAction::NextLyrics),
            };
            actions.extend(button(
                "search",
                right.center(),
                EarIcon::Search,
                action,
                theme::EAR_BUTTON_HOVER_BG,
            ));
            // Posisi pilihan saat ini (mis. 2/5) di kiri tombol.
            if let Alternatives::Ready { index, labels } = &view.alternatives {
                if labels.len() > 1 {
                    let pos = index.map_or("–".into(), |i| (i + 1).to_string());
                    let text = format!("{pos}/{}", labels.len());
                    let color = theme::EAR_COLOR.faded(alpha);
                    let g = text_galley(painter, &text, &style, style.size, color);
                    let at = Align2::RIGHT_CENTER
                        .anchor_size(right.center() - vec2(size / 2.0 + 6.0, 0.0), g.size());
                    paint_weighted(painter, at.min, g, style.weight, color);
                }
            }
        }
    }

    /// Overlay debug di dalam panel notch: garis pemisah tipis, kunci kuning.
    fn draw_notch_debug(
        &self,
        painter: &Painter,
        area: Rect,
        np: &now_playing::NowPlaying,
        now: Instant,
        alpha: f32,
    ) {
        let m = theme::NOTCH_DEBUG_MARGIN;
        let inner = Rect::from_min_max(
            pos2(area.min.x + m.x, area.min.y),
            pos2(area.max.x - m.x, area.max.y - m.y),
        );
        painter.hline(
            inner.x_range(),
            inner.min.y,
            Stroke::new(1.0, theme::NOTCH_DEBUG_DIVIDER.faded(alpha)),
        );
        let rows = self.debug_rows(np, now);
        let top = inner.min.y + theme::NOTCH_DEBUG_PAD_TOP;
        paint_debug_rows(
            painter,
            pos2(inner.min.x, top),
            inner.max.x,
            &rows,
            theme::NOTCH_DEBUG_KEY_COLOR.faded(alpha),
            theme::DEBUG_VALUE_COLOR.faded(alpha),
        );
    }

    /// Overlay debug: kotak monospace kecil di bawah panel, dua kolom
    /// (kunci, nilai). `column` = area di bawah panel/bentuk notch.
    fn draw_debug(&self, ui: &egui::Ui, np: &now_playing::NowPlaying, now: Instant, column: Rect) {
        let rows = self.debug_rows(np, now);
        let pad = theme::DEBUG_PAD;
        let height = 2.0 * pad.y + rows.len() as f32 * theme::DEBUG_ROW_HEIGHT;
        let rect = Rect::from_min_size(
            column.min + vec2(0.0, theme::DEBUG_GAP),
            vec2(column.width(), height),
        );
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(
            rect,
            CornerRadius::same(theme::DEBUG_RADIUS),
            theme::DEBUG_BG.color(),
        );
        paint_debug_rows(
            &painter,
            rect.min + pad,
            rect.max.x - pad.x,
            &rows,
            theme::DEBUG_KEY_COLOR.color(),
            theme::DEBUG_VALUE_COLOR.color(),
        );
    }

    /// Baris overlay debug (kunci, nilai), maksimal [`theme::DEBUG_MAX_ROWS`].
    fn debug_rows(
        &self,
        np: &now_playing::NowPlaying,
        now: Instant,
    ) -> Vec<(&'static str, String)> {
        let view = self.lyrics.view();
        let poll = np.poll_duration.map_or(0, |d| d.as_millis());
        let mut rows: Vec<(&'static str, String)> = Vec::new();
        match &np.player {
            Some(p) => {
                let interp = p.position_at(now);
                rows.push(("src", format!("{} · poll {poll} ms", p.source_app)));
                rows.push(("lagu", song_label(p)));
                rows.push((
                    "posisi",
                    format!(
                        "raw {} → interp {} ({:+.0} ms) · umur {:.1} dtk · {}",
                        clock(p.position_secs),
                        clock(interp),
                        (interp - p.position_secs) * 1000.0,
                        p.sample_age(now).as_secs_f64(),
                        if p.is_playing { "playing" } else { "paused" }
                    ),
                ));
            }
            None => rows.push(("src", format!("— · poll {poll} ms"))),
        }
        rows.push(("offset", tray::offset_text(self.settings.offset_secs)));
        rows.push(("lirik", lyrics_debug_label(view, now)));
        if let LyricsStatus::NetworkError(e) = &view.status {
            rows.push(("net", e.clone()));
        }
        if let Some(e) = &np.last_error {
            rows.push(("player", e.clone()));
        }
        rows.truncate(theme::DEBUG_MAX_ROWS);
        rows
    }
}

/// Keadaan mode notch yang disimpan di memori egui antar frame.
#[derive(Debug, Clone, Copy)]
struct NotchUi {
    transition: notch::Transition,
    /// Kapan (detik egui) kursor terakhir berada di zona dekat panel.
    last_near: Option<f64>,
}

/// Ikon tombol bulat di telinga notch.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EarIcon {
    Minus,
    Plus,
    Search,
}

fn paint_ear_icon(painter: &Painter, c: Pos2, icon: EarIcon, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    match icon {
        EarIcon::Minus => {
            painter.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], stroke);
        }
        EarIcon::Plus => {
            painter.line_segment([c - vec2(5.0, 0.0), c + vec2(5.0, 0.0)], stroke);
            painter.line_segment([c - vec2(0.0, 5.0), c + vec2(0.0, 5.0)], stroke);
        }
        EarIcon::Search => {
            // Kaca pembesar: lingkaran 8 px + gagang 45°.
            let lens = c - vec2(1.5, 1.5);
            painter.circle_stroke(lens, 3.25, stroke);
            let d = std::f32::consts::FRAC_1_SQRT_2;
            let from = lens + vec2(d, d) * 3.25;
            painter.line_segment([from, from + vec2(d, d) * 3.5], stroke);
        }
    }
}

/// Status satu baris di mode notch: [ikon] judul 15/600 + sub 12/400.
fn draw_notch_status(
    ui: &egui::Ui,
    painter: &Painter,
    row: Rect,
    icon: StatusIcon,
    main: &str,
    sub: &str,
    alpha: f32,
) {
    let gap = theme::NOTCH_STATUS_GAP;
    let max_w = row.width() - 2.0 * theme::NOTCH_ROW_PAD_X;
    let icon_w = match icon {
        StatusIcon::None => 0.0,
        StatusIcon::Loading => 3.0 * theme::LOADING_DOT_SIZE + 2.0 * theme::LOADING_DOT_GAP,
        StatusIcon::NotFound => theme::NOT_FOUND_RING_SIZE,
        StatusIcon::Error => theme::ERROR_DOT_SIZE,
    };
    let icon_space = if icon_w > 0.0 { icon_w + gap } else { 0.0 };
    let (main_style, sub_style) = (theme::NOTCH_STATUS_MAIN_TEXT, theme::NOTCH_STATUS_SUB_TEXT);
    let main_color = theme::NOTCH_STATUS_MAIN_COLOR.faded(alpha);
    let sub_color = theme::NOTCH_STATUS_SUB_COLOR.faded(alpha);
    let (main_text, size) = fit_line(
        painter,
        main,
        &main_style,
        max_w - icon_space,
        theme::MIN_FONT_SCALE,
    );
    let main_g = text_galley(painter, &main_text, &main_style, size, main_color);
    // Sub mengisi sisa lebar, dipotong ellipsis kalau tidak muat.
    let sub_avail = max_w - icon_space - main_g.size().x - gap;
    let sub_g = (sub_avail > 24.0 && !sub.is_empty()).then(|| {
        let text = ellipsize(sub, sub_avail, |s| {
            text_galley(painter, s, &sub_style, sub_style.size, sub_color)
        });
        text_galley(painter, &text, &sub_style, sub_style.size, sub_color)
    });
    let total = icon_space + main_g.size().x + sub_g.as_ref().map_or(0.0, |g| gap + g.size().x);
    let cy = row.center().y;
    let mut x = row.center().x - total / 2.0;

    match icon {
        StatusIcon::None => {}
        StatusIcon::Loading => {
            let step = (ui.input(|i| i.time) / theme::LOADING_STEP as f64) as usize % 3;
            let r = theme::LOADING_DOT_SIZE / 2.0;
            for i in 0..3 {
                let a = theme::NOTCH_LOADING_ALPHAS[(i + 3 - step) % 3];
                let dx = r + i as f32 * (theme::LOADING_DOT_SIZE + theme::LOADING_DOT_GAP);
                painter.circle_filled(pos2(x + dx, cy), r, theme::NOTCH_ACCENT.faded(a * alpha));
            }
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f32(theme::LOADING_STEP));
        }
        StatusIcon::NotFound => {
            let w = theme::NOT_FOUND_RING_WIDTH;
            painter.circle_stroke(
                pos2(x + icon_w / 2.0, cy),
                (theme::NOT_FOUND_RING_SIZE - w) / 2.0,
                Stroke::new(w, theme::NOTCH_NOT_FOUND_COLOR.faded(alpha)),
            );
        }
        StatusIcon::Error => {
            painter.circle_filled(
                pos2(x + icon_w / 2.0, cy),
                theme::ERROR_DOT_SIZE / 2.0,
                theme::ERROR_DOT_COLOR.faded(alpha),
            );
        }
    }
    x += icon_space;
    let at = Align2::LEFT_CENTER.anchor_size(pos2(x, cy), main_g.size());
    x += main_g.size().x + gap;
    paint_weighted(painter, at.min, main_g, main_style.weight, main_color);
    if let Some(g) = sub_g {
        let at = Align2::LEFT_CENTER.anchor_size(pos2(x, cy), g.size());
        paint_weighted(painter, at.min, g, sub_style.weight, sub_color);
    }
}

/// Gambar baris debug dua kolom mulai `top_left`, nilai dipotong di `right`.
fn paint_debug_rows(
    painter: &Painter,
    top_left: Pos2,
    right: f32,
    rows: &[(&'static str, String)],
    key_color: Color32,
    value_color: Color32,
) {
    let font = FontId::monospace(theme::DEBUG_FONT_SIZE);
    let value_x = top_left.x + theme::DEBUG_KEY_WIDTH + theme::DEBUG_COL_GAP;
    let value_w = right - value_x;
    for (i, (key, value)) in rows.iter().enumerate() {
        let cy = top_left.y + (i as f32 + 0.5) * theme::DEBUG_ROW_HEIGHT;
        painter.text(
            pos2(top_left.x, cy),
            Align2::LEFT_CENTER,
            key,
            font.clone(),
            key_color,
        );
        let value = ellipsize(value, value_w, |s| {
            painter.layout_no_wrap(s.to_string(), font.clone(), value_color)
        });
        let g = painter.layout_no_wrap(value, font.clone(), value_color);
        let at = Align2::LEFT_CENTER.anchor_size(pos2(value_x, cy), g.size());
        painter.galley(at.min, g, value_color);
    }
}

/// `m:ss`
fn minutes(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Progress animasi (0..1) sejak `value` terakhir berubah. Disimpan di
/// memori egui per `key`; nilai pertama yang dilihat dianggap sudah selesai.
fn animation_t<T>(ui: &egui::Ui, key: &str, value: T, now: Instant, duration: f32) -> f32
where
    T: PartialEq + Clone + Send + Sync + 'static,
{
    let id = ui.scope_id().with(key);
    let changed_at = ui.ctx().data_mut(|d| {
        let entry = d.get_temp_mut_or_insert_with(id, || (value.clone(), None::<Instant>));
        if entry.0 != value {
            *entry = (value, Some(now));
        }
        entry.1
    });
    let Some(at) = changed_at else {
        return 1.0;
    };
    let t = (now.duration_since(at).as_secs_f32() / duration).min(1.0);
    if t < 1.0 {
        ui.ctx().request_repaint();
    }
    t
}

fn song_label(p: &PlayerState) -> String {
    if p.artist.is_empty() {
        p.title.clone()
    } else {
        format!("{} — {}", p.title, p.artist)
    }
}

fn retry_label(view: &LyricsView, now: Instant) -> String {
    match view.retry_at {
        Some(at) => {
            let secs = at.saturating_duration_since(now).as_secs_f32().ceil();
            if secs >= 1.0 {
                format!("Nyoba lagi dalam {secs:.0} dtk…")
            } else {
                "Nyoba lagi…".into()
            }
        }
        None => "Nyoba lagi…".into(),
    }
}

/// `mm:ss.mmm`
fn clock(secs: f64) -> String {
    let ms = (secs.max(0.0) * 1000.0).round() as u64;
    format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000)
}

fn lyrics_debug_label(view: &LyricsView, now: Instant) -> String {
    let mut out = match &view.status {
        LyricsStatus::Ready(lines) => format!("synced · {} baris", lines.len()),
        LyricsStatus::Idle => "idle".into(),
        LyricsStatus::Loading => "nyari…".into(),
        LyricsStatus::NotFound => "nggak ketemu".into(),
        LyricsStatus::NetworkError(_) => "error jaringan".into(),
    };
    out.push_str(if now_playing::demo::enabled() {
        " · demo"
    } else {
        " · lrclib"
    });
    if let Some(d) = view.latency {
        out.push_str(&format!(" · latency {} ms", d.as_millis()));
    }
    out.push_str(&format!(" · percobaan {}", view.attempts));
    if let Some(at) = view.retry_at {
        let secs = at.saturating_duration_since(now).as_secs_f64();
        out.push_str(&format!(" · retry {secs:.0} dtk"));
    }
    if let (
        Alternatives::Ready {
            index: Some(i),
            labels,
        },
        Some(label),
    ) = (&view.alternatives, view.alternatives.current_label())
    {
        out.push_str(&format!(
            " · pilihan {}/{} \"{label}\"",
            i + 1,
            labels.len()
        ));
    }
    out
}

/// Latar panel: satu shadow sederhana, isi hitam transparan, border tipis.
fn paint_panel(painter: &Painter, panel: Rect, opacity: f32) {
    let radius = CornerRadius::same(theme::PANEL_RADIUS);
    let shadow = Shadow {
        offset: theme::PANEL_SHADOW_OFFSET,
        blur: theme::PANEL_SHADOW_BLUR,
        spread: 0,
        color: theme::PANEL_SHADOW.faded(opacity),
    };
    painter.add(shadow.as_shape(panel, radius));
    painter.rect(
        panel,
        radius,
        theme::PANEL_BG.faded(opacity),
        Stroke::new(
            theme::PANEL_BORDER_WIDTH,
            theme::PANEL_BORDER.faded(opacity),
        ),
        StrokeKind::Inside,
    );
}

/// Tanda jeda ❚❚ di kiri panel.
fn paint_pause_bars(painter: &Painter, panel: Rect, alpha: f32) {
    let color = theme::PAUSE_BAR_COLOR.faded(alpha);
    let first = panel.min + theme::PAUSE_BARS_POS;
    for i in 0..2 {
        let x = i as f32 * (theme::PAUSE_BAR_SIZE.x + theme::PAUSE_BAR_GAP);
        painter.rect_filled(
            Rect::from_min_size(first + vec2(x, 0.0), theme::PAUSE_BAR_SIZE),
            CornerRadius::same(theme::PAUSE_BAR_RADIUS),
            color,
        );
    }
}

/// Status di tengah panel: [ikon] judul, lalu sub di bawahnya.
fn draw_status(
    ui: &egui::Ui,
    painter: &Painter,
    panel: Rect,
    icon: StatusIcon,
    main: &str,
    sub: &str,
    alpha: f32,
) {
    let (main_style, sub_style) = (theme::STATUS_MAIN_TEXT, theme::STATUS_SUB_TEXT);
    let max_w = panel.width() - 2.0 * theme::PANEL_PAD_X;
    let icon_w = match icon {
        StatusIcon::None => 0.0,
        StatusIcon::Loading => 3.0 * theme::LOADING_DOT_SIZE + 2.0 * theme::LOADING_DOT_GAP,
        StatusIcon::NotFound => theme::NOT_FOUND_RING_SIZE,
        StatusIcon::Error => theme::ERROR_DOT_SIZE,
    };
    let icon_space = if icon_w > 0.0 {
        icon_w + theme::STATUS_ICON_GAP
    } else {
        0.0
    };

    let main_color = theme::STATUS_MAIN_COLOR.faded(alpha);
    let (text, size) = fit_line(
        painter,
        main,
        &main_style,
        max_w - icon_space,
        theme::MIN_FONT_SCALE,
    );
    let main_g = text_galley(painter, &text, &main_style, size, main_color);

    let total = main_style.line_height + theme::STATUS_GAP + sub_style.line_height;
    let top = panel.center().y - total / 2.0;
    let main_cy = top + main_style.line_height / 2.0;
    let x0 = panel.center().x - (icon_space + main_g.size().x) / 2.0;

    match icon {
        StatusIcon::None => {}
        StatusIcon::Loading => {
            // Titik paling terang berpindah ke kanan tiap LOADING_STEP.
            let step = (ui.input(|i| i.time) / theme::LOADING_STEP as f64) as usize % 3;
            let r = theme::LOADING_DOT_SIZE / 2.0;
            for i in 0..3 {
                let a = theme::LOADING_DOT_ALPHAS[(i + 3 - step) % 3];
                let x = x0 + r + i as f32 * (theme::LOADING_DOT_SIZE + theme::LOADING_DOT_GAP);
                painter.circle_filled(
                    pos2(x, main_cy),
                    r,
                    theme::LOADING_DOT_COLOR.faded(a * alpha),
                );
            }
            ui.ctx()
                .request_repaint_after(Duration::from_secs_f32(theme::LOADING_STEP));
        }
        StatusIcon::NotFound => {
            let w = theme::NOT_FOUND_RING_WIDTH;
            painter.circle_stroke(
                pos2(x0 + icon_w / 2.0, main_cy),
                (theme::NOT_FOUND_RING_SIZE - w) / 2.0,
                Stroke::new(w, theme::NOT_FOUND_RING_COLOR.faded(alpha)),
            );
        }
        StatusIcon::Error => {
            painter.circle_filled(
                pos2(x0 + icon_w / 2.0, main_cy),
                theme::ERROR_DOT_SIZE / 2.0,
                theme::ERROR_DOT_COLOR.faded(alpha),
            );
        }
    }
    let at = Align2::LEFT_CENTER.anchor_size(pos2(x0 + icon_space, main_cy), main_g.size());
    paint_weighted(painter, at.min, main_g, main_style.weight, main_color);

    // Sub (judul — artis, dll.): ukuran tetap, ellipsis kalau kepanjangan.
    let sub_cy = top + total - sub_style.line_height / 2.0;
    draw_text(
        painter,
        pos2(panel.center().x, sub_cy),
        max_w,
        sub,
        &sub_style,
        theme::STATUS_SUB_COLOR,
        alpha,
    );
}

/// Galley satu baris tanpa wrap dengan weight & letter spacing dari `style`.
fn text_galley(
    painter: &Painter,
    text: &str,
    style: &TextStyle,
    size: f32,
    color: Color32,
) -> Arc<Galley> {
    painter.layout_job(LayoutJob::single_section(
        text.to_string(),
        TextFormat {
            font_id: FontId::proportional(size),
            color,
            extra_letter_spacing: style.letter_spacing_em * size,
            coords: fonts::weight_coords(style.weight),
            ..Default::default()
        },
    ))
}

/// Gambar galley; kalau font utama statis, weight > 400 ditiru dengan
/// menggambar dua kali bergeser sedikit (faux-bold).
fn paint_weighted(painter: &Painter, pos: Pos2, galley: Arc<Galley>, weight: f32, color: Color32) {
    let bold = fonts::faux_bold_px(weight);
    if bold > 0.0 {
        painter.galley(pos + vec2(bold, 0.0), galley.clone(), color);
    }
    painter.galley(pos, galley, color);
}

/// Muat teks di `max_w` tanpa wrap: font mengecil sampai `min_scale` × ukuran
/// normal, kalau masih kepanjangan dipotong dengan ellipsis.
fn fit_line(
    painter: &Painter,
    text: &str,
    style: &TextStyle,
    max_w: f32,
    min_scale: f32,
) -> (String, f32) {
    let width = |s: &str, size: f32| {
        text_galley(painter, s, style, size, Color32::WHITE)
            .size()
            .x
    };
    let base = style.size;
    let w = width(text, base);
    if w <= max_w {
        return (text.to_string(), base);
    }
    let size = (base * max_w / w).max(base * min_scale);
    if width(text, size) <= max_w + 0.5 {
        return (text.to_string(), size);
    }
    let text = ellipsize(text, max_w, |s| {
        text_galley(painter, s, style, size, Color32::WHITE)
    });
    (text, size)
}

/// Potong `text` dengan "…" sampai muat di `max_w` (cari biner per karakter).
fn ellipsize(text: &str, max_w: f32, layout: impl Fn(&str) -> Arc<Galley>) -> String {
    if layout(text).size().x <= max_w {
        return text.to_string();
    }
    let ends: Vec<usize> = text.char_indices().map(|(i, c)| i + c.len_utf8()).collect();
    let candidate = |n: usize| {
        let head = if n == 0 { "" } else { &text[..ends[n - 1]] };
        format!("{}…", head.trim_end())
    };
    let (mut lo, mut hi) = (0, ends.len().saturating_sub(1));
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        if layout(&candidate(mid)).size().x <= max_w {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    candidate(lo)
}

/// Satu baris teks di tengah `center`, dimuatkan ke `max_w`.
fn draw_text(
    painter: &Painter,
    center: Pos2,
    max_w: f32,
    text: &str,
    style: &TextStyle,
    tint: Tint,
    alpha: f32,
) {
    let color = tint.faded(alpha);
    let (text, size) = fit_line(painter, text, style, max_w, theme::MIN_FONT_SCALE);
    let g = text_galley(painter, &text, style, size, color);
    let rect = Align2::CENTER_CENTER.anchor_size(center, g.size());
    paint_weighted(painter, rect.min, g, style.weight, color);
}

/// Baris aktif: teks `base` (redup), lalu seluruh baris dalam warna `lit`
/// dengan opacity `t` (0..1, fade-in saat baris baru mulai).
#[allow(clippy::too_many_arguments)]
fn draw_highlight(
    painter: &Painter,
    center: Pos2,
    max_w: f32,
    text: &str,
    style: &TextStyle,
    base: Color32,
    lit: Color32,
    t: f32,
) {
    let (text, size) = fit_line(painter, text, style, max_w, theme::MIN_FONT_SCALE);
    let base_g = text_galley(painter, &text, style, size, base);
    let rect = Align2::CENTER_CENTER.anchor_size(center, base_g.size());
    if t < 1.0 {
        paint_weighted(painter, rect.min, base_g, style.weight, base);
    }
    if t > 0.0 {
        let lit = lit.gamma_multiply(t);
        let lit_g = text_galley(painter, &text, style, size, lit);
        paint_weighted(painter, rect.min, lit_g, style.weight, lit);
    }
}

/// Ikon kecil di tombol kontrol panel, digambar dengan garis.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Icon {
    Minus,
    Plus,
    ChevronLeft,
    ChevronRight,
}

/// Satu isi grup kontrol.
enum Control {
    Icon(Icon, TrayAction),
    Button(String, TrayAction),
    Label(String),
    Disabled(String),
}

/// Grup kontrol berbentuk pil, ditempatkan di `anchor` (pojok sesuai
/// `align`). Klik ditambahkan ke `actions`.
#[allow(clippy::too_many_arguments)]
fn control_group(
    ui: &egui::Ui,
    key: &str,
    anchor: Pos2,
    align: Align2,
    items: &[Control],
    alpha: f32,
    interactive: bool,
    actions: &mut Vec<TrayAction>,
) {
    let painter = ui.painter();
    let style = theme::CONTROL_TEXT;
    let text_color = theme::CONTROL_TEXT_COLOR.faded(alpha);
    let galley = |text: &str| text_galley(painter, text, &style, style.size, text_color);
    let widths: Vec<f32> = items
        .iter()
        .map(|item| match item {
            Control::Icon(..) => theme::CONTROL_ICON_WIDTH,
            Control::Button(t, _) | Control::Label(t) | Control::Disabled(t) => {
                // Tombol reset berganti label jadi "Reset" saat di-hover;
                // lebarnya diambil yang terbesar supaya grup tidak goyang.
                let w = galley(t).size().x;
                let w = match item {
                    Control::Button(..) => w.max(galley("Reset").size().x),
                    _ => w,
                };
                w + 2.0 * theme::CONTROL_TEXT_PAD_X
            }
        })
        .collect();
    let size = vec2(widths.iter().sum(), theme::CONTROL_HEIGHT);
    let group = align.anchor_size(anchor, size);
    let radius = CornerRadius::same(theme::CONTROL_RADIUS);
    painter.rect_filled(group, radius, theme::CONTROL_GROUP_BG.faded(alpha));

    let mut x = group.min.x;
    for (i, (item, w)) in items.iter().zip(widths).enumerate() {
        let rect = Rect::from_min_size(pos2(x, group.min.y), vec2(w, size.y));
        x += w;
        let action = match item {
            Control::Icon(_, a) | Control::Button(_, a) => Some(*a),
            _ => None,
        };
        let mut hovered = false;
        if let (Some(action), true) = (action, interactive) {
            let resp = ui.interact(rect, ui.scope_id().with((key, i)), Sense::click());
            hovered = resp.hovered();
            if resp.hovered() || resp.is_pointer_button_down_on() {
                let bg = if resp.is_pointer_button_down_on() {
                    theme::CONTROL_PRESS_BG
                } else {
                    theme::CONTROL_HOVER_BG
                };
                painter.rect_filled(rect, radius, bg.faded(alpha));
            }
            if resp.clicked() {
                actions.push(action);
            }
        }
        match item {
            Control::Icon(icon, _) => paint_icon(
                painter,
                rect.center(),
                *icon,
                theme::CONTROL_ICON_COLOR.faded(alpha),
            ),
            Control::Button(text, _) | Control::Label(text) | Control::Disabled(text) => {
                let (text, a) = match item {
                    Control::Button(..) if hovered => ("Reset", alpha),
                    Control::Disabled(_) => (text.as_str(), alpha * theme::CONTROL_DISABLED_ALPHA),
                    _ => (text.as_str(), alpha),
                };
                let color = theme::CONTROL_TEXT_COLOR.faded(a);
                let g = text_galley(painter, text, &style, style.size, color);
                let at = Align2::CENTER_CENTER.anchor_size(rect.center(), g.size());
                paint_weighted(painter, at.min, g, style.weight, color);
            }
        }
    }
}

fn paint_icon(painter: &Painter, c: Pos2, icon: Icon, color: Color32) {
    let h = theme::CONTROL_ICON_SIZE / 2.0;
    let stroke = Stroke::new(theme::CONTROL_ICON_STROKE, color);
    match icon {
        Icon::Minus => {
            painter.line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], stroke);
        }
        Icon::Plus => {
            painter.line_segment([c - vec2(h, 0.0), c + vec2(h, 0.0)], stroke);
            painter.line_segment([c - vec2(0.0, h), c + vec2(0.0, h)], stroke);
        }
        Icon::ChevronLeft | Icon::ChevronRight => {
            let dir = if icon == Icon::ChevronLeft { -1.0 } else { 1.0 };
            let tip = c + vec2(dir * h / 2.0, 0.0);
            let back = c - vec2(dir * h / 2.0, 0.0);
            painter.line_segment([back - vec2(0.0, h), tip], stroke);
            painter.line_segment([tip, back + vec2(0.0, h)], stroke);
        }
    }
}
