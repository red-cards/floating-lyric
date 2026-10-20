//! Geometri & animasi mode tampilan "Notch" (handoff Claude Design arah
//! gaya **D · Notch mode**, putaran 2).
//!
//! Panel #000000 pekat yang tumbuh dari notch fisik. Area kamera di tengah
//! atas selalu kosong; sudut atas melengkung ke dalam menyatu dengan tepi
//! layar, sudut bawah membulat. Collapsed cuma sedikit lebih lebar dari notch
//! (♪ di kiri, equalizer di kanan). Expanded: "telinga" judul/artis di kiri
//! & kanan kamera, satu baris lirik di bawahnya. Di layar tanpa notch dan
//! Windows: pill hitam yang menggantung dari tepi atas.

use eframe::egui::{
    epaint::{Mesh, Vertex, WHITE_UV},
    pos2, vec2, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Vec2,
};

use crate::theme;

/// Ukuran notch kamera (points), hasil deteksi OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotchMetrics {
    pub width: f32,
    pub height: f32,
}

/// Ruang kosong di bawah/samping bentuk di dalam window.
const PAD: f32 = 2.0;

/// Bentuk yang digambar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Notch(NotchMetrics),
    Pill,
}

/// Override untuk mengetes bentuk notch di layar tanpa notch:
/// `MINI_LYRICS_FAKE_NOTCH=200x32` (lebar x tinggi, points).
pub fn fake_from_env() -> Option<NotchMetrics> {
    parse_fake(&std::env::var("MINI_LYRICS_FAKE_NOTCH").ok()?)
}

fn parse_fake(v: &str) -> Option<NotchMetrics> {
    let (w, h) = v.trim().split_once(['x', 'X'])?;
    Some(NotchMetrics {
        width: w.trim().parse().ok()?,
        height: h.trim().parse().ok()?,
    })
}

impl Kind {
    pub fn from_metrics(m: Option<NotchMetrics>) -> Self {
        match m {
            Some(m) if m.width > 0.0 && m.height > 0.0 => Kind::Notch(m),
            _ => Kind::Pill,
        }
    }

    /// Lebar area kamera yang harus kosong (0 untuk pill).
    pub fn camera_width(&self) -> f32 {
        match self {
            Kind::Notch(m) => m.width,
            Kind::Pill => 0.0,
        }
    }

    /// Tinggi pita atas sejajar kamera, tempat telinga (0 untuk pill).
    pub fn band(&self) -> f32 {
        match self {
            Kind::Notch(m) => m.height,
            Kind::Pill => 0.0,
        }
    }

    /// Tinggi baris lirik/status di bawah pita.
    pub fn row_height(&self) -> f32 {
        match self {
            Kind::Notch(_) => theme::NOTCH_ROW_HEIGHT,
            Kind::Pill => theme::PILL_EXPANDED.y,
        }
    }

    pub fn collapsed_size(&self) -> Vec2 {
        match self {
            Kind::Notch(m) => vec2(m.width + 2.0 * theme::NOTCH_COLLAPSED_EAR, m.height),
            Kind::Pill => theme::PILL_COLLAPSED,
        }
    }

    /// Ukuran expanded; `extra` = tambahan tinggi (overlay debug).
    pub fn expanded_size(&self, extra: f32) -> Vec2 {
        match self {
            Kind::Notch(m) => vec2(
                theme::NOTCH_EXPANDED_WIDTH.max(m.width + 2.0 * theme::NOTCH_MIN_EAR),
                m.height + theme::NOTCH_ROW_HEIGHT + extra,
            ),
            Kind::Pill => theme::PILL_EXPANDED + vec2(0.0, extra),
        }
    }

    /// Radius sudut bawah untuk progress bentuk `t` (0 collapsed, 1 expanded).
    pub fn bottom_radius(&self, t: f32) -> f32 {
        let (c, e) = match self {
            Kind::Notch(_) => (theme::NOTCH_RADIUS_COLLAPSED, theme::NOTCH_RADIUS_EXPANDED),
            Kind::Pill => (theme::PILL_RADIUS_COLLAPSED, theme::PILL_RADIUS_EXPANDED),
        };
        lerp(c, e, t)
    }

    /// Jarak bentuk dari tepi atas layar: notch dan pill sama-sama menggantung
    /// dari tepi atas.
    pub fn top_margin(&self) -> f32 {
        0.0
    }

    /// Ukuran window (tetap, sebesar bentuk expanded + fillet) supaya animasi
    /// tidak perlu me-resize window native tiap frame.
    pub fn window_size(&self, extra: f32) -> Vec2 {
        let e = self.expanded_size(extra);
        vec2(e.x + 2.0 * (theme::NOTCH_TOP_FILLET + PAD), e.y + PAD)
    }

    /// Kotak bentuk (tanpa fillet) untuk progress `t`; selalu di tengah dan
    /// menempel ke atas window.
    pub fn body_rect(&self, origin: Pos2, window_width: f32, t: f32, extra: f32) -> Rect {
        let (c, e) = (self.collapsed_size(), self.expanded_size(extra));
        let size = c + (e - c) * t;
        let x = origin.x + (window_width - size.x) / 2.0;
        Rect::from_min_size(pos2(x, origin.y), size)
    }

    /// Telinga kiri & kanan: pita atas di samping area kamera. Untuk pill
    /// (tanpa pita) dua-duanya kosong.
    pub fn ears(&self, body: Rect) -> (Rect, Rect) {
        let band = self.band();
        let cam = self.camera_width();
        let cx = body.center().x;
        let left = Rect::from_min_max(body.min, pos2(cx - cam / 2.0, body.min.y + band));
        let right = Rect::from_min_max(
            pos2(cx + cam / 2.0, body.min.y),
            pos2(body.max.x, body.min.y + band),
        );
        (left, right)
    }

    /// Baris lirik/status di bawah pita, tinggi tetap.
    pub fn row_rect(&self, body: Rect) -> Rect {
        let top = body.min.y + self.band();
        Rect::from_min_max(
            pos2(body.min.x, top),
            pos2(body.max.x, top + self.row_height()),
        )
    }

    /// Area yang dianggap "panel" (termasuk fillet) untuk hover.
    pub fn hit_rect(&self, body: Rect) -> Rect {
        body.expand2(vec2(theme::NOTCH_TOP_FILLET, 0.0))
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// `cubic-bezier(x1, y1, x2, y2)` CSS: nilai y untuk progress waktu `x`.
pub fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let bez = |a: f32, b: f32, t: f32| {
        let u = 1.0 - t;
        3.0 * u * u * t * a + 3.0 * u * t * t * b + t * t * t
    };
    // Cari t sehingga bez_x(t) = x (monoton untuk 0 ≤ x1, x2 ≤ 1).
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if bez(x1, x2, mid) < x {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bez(y1, y2, (lo + hi) / 2.0)
}

/// Easing bentuk dari desain: `cubic-bezier(0.32, 0.72, 0, 1)`.
pub fn shape_ease(x: f32) -> f32 {
    cubic_bezier(0.32, 0.72, 0.0, 1.0, x)
}

/// Keadaan animasi collapsed ↔ expanded.
///
/// Membuka: lebar & tinggi tumbuh bersamaan ([`theme::NOTCH_SHAPE_MS`]),
/// isi fade-in ([`theme::NOTCH_CONTENT_IN_MS`]) dengan delay
/// [`theme::NOTCH_CONTENT_DELAY_MS`]. Menutup: urutan dibalik — isi fade-out
/// ([`theme::NOTCH_CONTENT_OUT_MS`]) dulu, baru bentuknya menyusut.
/// Kalau arah berbalik di tengah jalan, animasi lanjut dari nilai saat itu.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transition {
    pub open: bool,
    /// Detik sejak arah terakhir berubah.
    started: f64,
    shape_from: f32,
    content_from: f32,
}

/// Nilai animasi pada satu saat.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    /// 0 = collapsed, 1 = expanded (sudah di-easing).
    pub shape: f32,
    /// Opacity isi expanded (lirik, telinga).
    pub content: f32,
    pub settled: bool,
}

impl Transition {
    pub fn new(open: bool, now: f64) -> Self {
        let v = if open { 1.0 } else { 0.0 };
        Self {
            open,
            started: now - 10.0,
            shape_from: v,
            content_from: v,
        }
    }

    pub fn set(&mut self, open: bool, now: f64) {
        if open != self.open {
            let f = self.frame(now);
            *self = Self {
                open,
                started: now,
                shape_from: f.shape,
                content_from: f.content,
            };
        }
    }

    pub fn frame(&self, now: f64) -> Frame {
        let ms = ((now - self.started) * 1000.0).max(0.0) as f32;
        let clamp = |v: f32| v.clamp(0.0, 1.0);
        let shape_ms = theme::NOTCH_SHAPE_MS;
        if self.open {
            let s = shape_ease(ms / shape_ms);
            let c = clamp((ms - theme::NOTCH_CONTENT_DELAY_MS) / theme::NOTCH_CONTENT_IN_MS);
            let end = shape_ms.max(theme::NOTCH_CONTENT_DELAY_MS + theme::NOTCH_CONTENT_IN_MS);
            Frame {
                shape: lerp(self.shape_from, 1.0, s),
                content: lerp(self.content_from, 1.0, c),
                settled: ms >= end,
            }
        } else {
            let out = theme::NOTCH_CONTENT_OUT_MS;
            let c = clamp(ms / out);
            let s = shape_ease((ms - out) / shape_ms);
            Frame {
                shape: lerp(self.shape_from, 0.0, s),
                content: lerp(self.content_from, 0.0, c),
                settled: ms >= out + shape_ms,
            }
        }
    }
}

/// Gambar bentuk: hitam, sudut atas inverse fillet (melebar ke luar menyatu
/// dengan tepi atas layar), sudut bawah membulat. Dipakai notch dan pill.
pub fn paint_shape(painter: &Painter, body: Rect, bottom_radius: f32, color: Color32) {
    let rb = bottom_radius.min(body.height()).min(body.width() / 2.0);
    painter.rect_filled(
        body,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: rb.round() as u8,
            se: rb.round() as u8,
        },
        color,
    );
    let rt = theme::NOTCH_TOP_FILLET.min(body.height());
    painter.add(Shape::mesh(fillet_mesh(body.left_top(), rt, -1.0, color)));
    painter.add(Shape::mesh(fillet_mesh(body.right_top(), rt, 1.0, color)));
}

/// "Telinga" inverse fillet di luar sudut atas `corner`. `side` = -1 untuk
/// kiri, 1 untuk kanan. Area = persegi rt×rt di luar sudut dikurangi
/// seperempat lingkaran; bentuknya tidak konveks tapi bisa di-fan dari titik
/// sudut (semua titik busur terlihat dari sana).
fn fillet_mesh(corner: Pos2, rt: f32, side: f32, color: Color32) -> Mesh {
    const STEPS: usize = 12;
    let center = pos2(corner.x + side * rt, corner.y + rt);
    let mut mesh = Mesh::default();
    let v = |p: Pos2| Vertex {
        pos: p,
        uv: WHITE_UV,
        color,
    };
    mesh.vertices.push(v(corner));
    for i in 0..=STEPS {
        let a = (i as f32 / STEPS as f32) * std::f32::consts::FRAC_PI_2;
        // Dari titik di tepi atas (center.x, top) ke titik di sisi bentuk
        // (corner.x, center.y).
        let p = pos2(center.x - side * rt * a.sin(), center.y - rt * a.cos());
        mesh.vertices.push(v(p));
    }
    for i in 1..=STEPS as u32 {
        mesh.indices.extend_from_slice(&[0, i, i + 1]);
    }
    mesh
}

/// Tinggi tiga bar equalizer pada waktu `time` (detik). Diam (2 px) saat
/// lagu dijeda atau belum diputar.
pub fn equalizer_heights(playing: bool, time: f64) -> [f32; 3] {
    if !playing {
        return [theme::EQ_FLAT_HEIGHT; 3];
    }
    let mut out = theme::EQ_HEIGHTS;
    for (i, h) in out.iter_mut().enumerate() {
        // Tiap bar berayun dengan kecepatan beda supaya tidak seragam.
        let phase = time * theme::EQ_SPEEDS[i] + i as f64 * 1.7;
        let k = 0.45 + 0.55 * (0.5 + 0.5 * phase.sin() as f32);
        *h = (*h * k).clamp(theme::EQ_FLAT_HEIGHT, theme::EQ_MAX_HEIGHT);
    }
    out
}

/// Gambar equalizer tiga bar, rata bawah, dengan pojok kiri bawah di
/// `left_bottom`.
pub fn paint_equalizer(painter: &Painter, left_bottom: Pos2, heights: [f32; 3], color: Color32) {
    let w = theme::EQ_BAR_WIDTH;
    for (i, h) in heights.into_iter().enumerate() {
        let x = left_bottom.x + i as f32 * (w + theme::EQ_BAR_GAP);
        painter.rect_filled(
            Rect::from_min_max(pos2(x, left_bottom.y - h), pos2(x + w, left_bottom.y)),
            CornerRadius::same(1),
            color,
        );
    }
}

/// Lebar total equalizer.
pub fn equalizer_width() -> f32 {
    3.0 * theme::EQ_BAR_WIDTH + 2.0 * theme::EQ_BAR_GAP
}

/// Tanda jeda ❚❚ kecil (kuning); `center_left` = tepi kiri, tengah vertikal.
pub fn paint_pause(painter: &Painter, center_left: Pos2, color: Color32) {
    let s = theme::NOTCH_PAUSE_BAR;
    for i in 0..2 {
        let x = center_left.x + i as f32 * (s.x + theme::NOTCH_PAUSE_GAP);
        painter.rect_filled(
            Rect::from_min_size(pos2(x, center_left.y - s.y / 2.0), s),
            CornerRadius::same(1),
            color,
        );
    }
}

pub fn pause_width() -> f32 {
    2.0 * theme::NOTCH_PAUSE_BAR.x + theme::NOTCH_PAUSE_GAP
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: NotchMetrics = NotchMetrics {
        width: 200.0,
        height: 32.0,
    };

    #[test]
    fn kind_from_metrics() {
        assert_eq!(Kind::from_metrics(None), Kind::Pill);
        assert_eq!(
            Kind::from_metrics(Some(NotchMetrics {
                width: 0.0,
                height: 30.0
            })),
            Kind::Pill
        );
        assert_eq!(Kind::from_metrics(Some(M)), Kind::Notch(M));
    }

    #[test]
    fn notch_sizes_match_design() {
        let k = Kind::Notch(M);
        // collapsed 272×32 · expanded 560×80 (pita 32 + baris 48)
        assert_eq!(k.collapsed_size(), vec2(272.0, 32.0));
        assert_eq!(k.expanded_size(0.0), vec2(560.0, 80.0));
        assert_eq!(k.bottom_radius(0.0), 10.0);
        assert_eq!(k.bottom_radius(1.0), 18.0);
        // Notch lebar: telinga tetap minimal 180.
        let wide = Kind::Notch(NotchMetrics {
            width: 240.0,
            height: 38.0,
        });
        assert_eq!(wide.expanded_size(0.0).x, 600.0);
    }

    #[test]
    fn pill_sizes_match_design() {
        let k = Kind::Pill;
        // pill 520×44 r22 · collapsed 112×26 r13
        assert_eq!(k.collapsed_size(), vec2(112.0, 26.0));
        assert_eq!(k.expanded_size(0.0), vec2(520.0, 44.0));
        assert_eq!(k.bottom_radius(0.0), 13.0);
        assert_eq!(k.bottom_radius(1.0), 22.0);
        assert_eq!(k.band(), 0.0);
        assert_eq!(k.top_margin(), 0.0);
    }

    #[test]
    fn layout_keeps_camera_zone_empty() {
        let k = Kind::Notch(M);
        let ws = k.window_size(0.0);
        let body = k.body_rect(Pos2::ZERO, ws.x, 1.0, 0.0);
        let (l, r) = k.ears(body);
        assert_eq!(l.width(), 180.0);
        assert_eq!(r.width(), 180.0);
        assert_eq!(r.min.x - l.max.x, 200.0);
        let row = k.row_rect(body);
        assert_eq!(row.min.y, 32.0);
        assert_eq!(row.height(), 48.0);
        // Fillet muat di dalam window.
        assert!(k.hit_rect(body).min.x >= 0.0 && k.hit_rect(body).max.x <= ws.x);
        assert_eq!(k.window_size(100.0).y, ws.y + 100.0);
    }

    #[test]
    fn cubic_bezier_endpoints_and_shape() {
        assert!(shape_ease(0.0).abs() < 1e-4);
        assert!((shape_ease(1.0) - 1.0).abs() < 1e-4);
        // Kurva cepat di awal: separuh waktu sudah jauh lewat separuh jalan.
        assert!(shape_ease(0.5) > 0.8);
        // linear(0,0,1,1) = identitas
        assert!((cubic_bezier(0.0, 0.0, 1.0, 1.0, 0.3) - 0.3).abs() < 1e-3);
    }

    #[test]
    fn open_grows_shape_then_fades_content_in() {
        let mut tr = Transition::new(false, 0.0);
        tr.set(true, 100.0);
        let f = tr.frame(100.1); // 100 ms
        assert!(f.shape > 0.0 && f.shape < 1.0);
        assert_eq!(f.content, 0.0, "isi baru mulai setelah delay 120 ms");
        let f = tr.frame(100.3); // 300 ms: bentuk final, isi ~100%
        assert!((f.shape - 1.0).abs() < 1e-3);
        assert!((f.content - 1.0).abs() < 1e-3);
        assert!(f.settled);
    }

    #[test]
    fn close_fades_content_out_before_shrinking() {
        let mut tr = Transition::new(true, 0.0);
        tr.set(false, 50.0);
        let f = tr.frame(50.1); // 100 ms
        assert!(f.content < 0.3);
        assert_eq!(f.shape, 1.0, "bentuk belum menyusut selama isi fade-out");
        let f = tr.frame(50.5);
        assert!(f.shape < 1e-3 && f.content == 0.0 && f.settled);
    }

    #[test]
    fn reversal_continues_from_current_value() {
        let mut tr = Transition::new(false, 0.0);
        tr.set(true, 10.0);
        let mid = tr.frame(10.1).shape;
        tr.set(false, 10.1);
        assert!((tr.frame(10.1).shape - mid).abs() < 1e-4);
    }

    #[test]
    fn equalizer_flat_when_not_playing() {
        assert_eq!(equalizer_heights(false, 3.0), [2.0; 3]);
        for t in [0.0, 0.4, 1.3, 7.7] {
            for h in equalizer_heights(true, t) {
                assert!((2.0..=11.0).contains(&h));
            }
        }
    }

    #[test]
    fn parses_fake_notch() {
        assert_eq!(parse_fake("200x32"), Some(M));
        assert_eq!(parse_fake("abc"), None);
    }

    #[test]
    fn fillet_mesh_spans_from_top_edge_to_side() {
        let m = fillet_mesh(pos2(100.0, 0.0), 10.0, -1.0, Color32::BLACK);
        let first = m.vertices[1].pos;
        let last = m.vertices.last().unwrap().pos;
        assert!((first - pos2(90.0, 0.0)).length() < 1e-4);
        assert!((last - pos2(100.0, 10.0)).length() < 1e-4);
        let right = fillet_mesh(pos2(200.0, 0.0), 10.0, 1.0, Color32::BLACK);
        assert!((right.vertices[1].pos - pos2(210.0, 0.0)).length() < 1e-4);
    }
}
