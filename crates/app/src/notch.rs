//! Geometri & animasi mode tampilan "Notch".
//!
//! Di Mac ber-notch, bentuk hitam digambar persis menutupi notch kamera saat
//! collapsed lalu memanjang ke bawah (dan melebar) saat expanded; lirik satu
//! baris tampil di bawah area kamera. Di layar tanpa notch dan Windows,
//! bentuknya pill hitam di tengah atas dengan perilaku yang sama.

use eframe::egui::{
    epaint::{Mesh, Vertex, WHITE_UV},
    pos2, vec2, Color32, CornerRadius, Painter, Pos2, Rect, Shape, Vec2,
};

/// Ukuran notch kamera (points), hasil deteksi OS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotchMetrics {
    pub width: f32,
    pub height: f32,
}

/// Radius inverse fillet di sudut atas notch.
pub const TOP_FILLET: f32 = 6.0;
/// Tinggi tambahan di bawah area kamera untuk satu baris lirik.
pub const LYRIC_STRIP: f32 = 40.0;
/// Pill (tanpa notch): ukuran collapsed & expanded.
pub const PILL_COLLAPSED: Vec2 = vec2(120.0, 22.0);
pub const PILL_EXPANDED: Vec2 = vec2(520.0, 40.0);
/// Jarak pill dari tepi atas layar.
#[cfg(target_os = "macos")]
pub const PILL_TOP_MARGIN: f32 = 1.0;
#[cfg(not(target_os = "macos"))]
pub const PILL_TOP_MARGIN: f32 = 8.0;
/// Ruang kosong di sekeliling bentuk di dalam window.
const PAD: f32 = 2.0;

/// Bentuk yang digambar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Notch(NotchMetrics),
    Pill,
}

/// Override untuk mengetes bentuk notch di layar tanpa notch:
/// `MINI_LYRICS_FAKE_NOTCH=185x32` (lebar x tinggi, points).
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

    pub fn expanded_size(&self) -> Vec2 {
        match self {
            Kind::Notch(m) => vec2((m.width + 300.0).max(500.0), m.height + LYRIC_STRIP),
            Kind::Pill => PILL_EXPANDED,
        }
    }

    pub fn collapsed_size(&self) -> Vec2 {
        match self {
            Kind::Notch(m) => vec2(m.width, m.height),
            Kind::Pill => PILL_COLLAPSED,
        }
    }

    /// Jarak bentuk dari tepi atas layar (notch menempel ke tepi atas).
    pub fn top_margin(&self) -> f32 {
        match self {
            Kind::Notch(_) => 0.0,
            Kind::Pill => PILL_TOP_MARGIN,
        }
    }

    /// Ukuran window (tetap, sebesar bentuk expanded) supaya animasi tidak
    /// perlu me-resize window native tiap frame.
    pub fn window_size(&self, extra_height: f32) -> Vec2 {
        let e = self.expanded_size();
        let ears = match self {
            Kind::Notch(_) => 2.0 * TOP_FILLET,
            Kind::Pill => 0.0,
        };
        vec2(e.x + ears + 2.0 * PAD, e.y + PAD + extra_height)
    }

    /// Kotak bentuk (tanpa "telinga" fillet) di dalam window untuk progress
    /// animasi `t` (0 = collapsed, 1 = expanded). `window_width` = lebar
    /// window dalam points; bentuk selalu di tengah dan menempel ke atas.
    pub fn body_rect(&self, origin: Pos2, window_width: f32, t: f32) -> Rect {
        let t = ease(t);
        let (c, e) = (self.collapsed_size(), self.expanded_size());
        let size = c + (e - c) * t;
        let x = origin.x + (window_width - size.x) / 2.0;
        Rect::from_min_size(pos2(x, origin.y), size)
    }

    /// Area untuk satu baris lirik: di bawah area kamera (notch) atau seluruh
    /// pill.
    pub fn text_rect(&self, body: Rect) -> Rect {
        match self {
            Kind::Notch(m) => Rect::from_min_max(pos2(body.min.x, body.min.y + m.height), body.max)
                .shrink2(vec2(18.0, 2.0)),
            Kind::Pill => body.shrink2(vec2(18.0, 2.0)),
        }
    }

    /// Area yang dianggap "panel" untuk efek redup saat kursor masuk.
    pub fn hit_rect(&self, body: Rect) -> Rect {
        match self {
            Kind::Notch(_) => body.expand2(vec2(TOP_FILLET, 0.0)),
            Kind::Pill => body,
        }
    }
}

/// Smoothstep supaya animasi tinggi tidak kaku.
pub fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Opacity teks: mulai muncul setelah bentuk cukup terbuka.
pub fn text_alpha(t: f32) -> f32 {
    ((t - 0.45) / 0.55).clamp(0.0, 1.0)
}

/// Gambar bentuk notch: hitam, sudut atas inverse fillet (melebar ke luar
/// menyatu dengan tepi atas), sudut bawah membulat.
pub fn paint_notch(painter: &Painter, body: Rect, bottom_radius: f32, color: Color32) {
    let rb = bottom_radius
        .min(body.height() / 2.0)
        .min(body.width() / 2.0);
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
    let rt = TOP_FILLET.min(body.height());
    painter.add(Shape::mesh(fillet_mesh(body.left_top(), rt, -1.0, color)));
    painter.add(Shape::mesh(fillet_mesh(body.right_top(), rt, 1.0, color)));
}

/// "Telinga" inverse fillet di luar sudut atas `corner`. `side` = -1 untuk
/// kiri, 1 untuk kanan. Area = persegi rt×rt di luar sudut dikurangi
/// seperempat lingkaran; bentuknya tidak konveks tapi bisa di-fan dari titik
/// sudut (semua titik busur terlihat dari sana).
fn fillet_mesh(corner: Pos2, rt: f32, side: f32, color: Color32) -> Mesh {
    const STEPS: usize = 10;
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

/// Gambar pill hitam.
pub fn paint_pill(painter: &Painter, body: Rect, color: Color32) {
    let r = (body.height() / 2.0).min(255.0) as u8;
    painter.rect_filled(body, CornerRadius::same(r), color);
}

/// Radius bawah notch: kecil saat collapsed (mirip notch asli), lebih bulat
/// saat expanded.
pub fn notch_bottom_radius(t: f32) -> f32 {
    9.0 + (16.0 - 9.0) * ease(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: NotchMetrics = NotchMetrics {
        width: 185.0,
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
    fn collapsed_notch_matches_camera_area_exactly() {
        let k = Kind::Notch(M);
        let w = k.window_size(0.0).x;
        let body = k.body_rect(Pos2::ZERO, w, 0.0);
        assert_eq!(body.size(), vec2(185.0, 32.0));
        assert_eq!(body.min.y, 0.0);
        assert!((body.center().x - w / 2.0).abs() < 1e-4);
    }

    #[test]
    fn expanded_notch_fits_window_with_ears() {
        let k = Kind::Notch(M);
        let ws = k.window_size(0.0);
        let body = k.body_rect(Pos2::ZERO, ws.x, 1.0);
        assert_eq!(body.size(), k.expanded_size());
        let hit = k.hit_rect(body);
        assert!(hit.min.x >= 0.0 && hit.max.x <= ws.x);
        assert!(body.max.y <= ws.y);
        // Teks di bawah area kamera.
        assert!(k.text_rect(body).min.y >= M.height);
    }

    #[test]
    fn pill_sizes_and_debug_extra() {
        let k = Kind::Pill;
        assert_eq!(k.body_rect(Pos2::ZERO, 600.0, 0.0).size(), PILL_COLLAPSED);
        assert_eq!(k.body_rect(Pos2::ZERO, 600.0, 1.0).size(), PILL_EXPANDED);
        assert_eq!(k.window_size(100.0).y, k.window_size(0.0).y + 100.0);
    }

    #[test]
    fn parses_fake_notch() {
        assert_eq!(
            parse_fake("185x32"),
            Some(NotchMetrics {
                width: 185.0,
                height: 32.0
            })
        );
        assert_eq!(parse_fake("abc"), None);
    }

    #[test]
    fn easing_and_fade() {
        assert_eq!(ease(0.0), 0.0);
        assert_eq!(ease(1.0), 1.0);
        assert!((ease(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(text_alpha(0.3), 0.0);
        assert_eq!(text_alpha(1.0), 1.0);
        assert!(notch_bottom_radius(0.0) < notch_bottom_radius(1.0));
    }

    #[test]
    fn fillet_mesh_spans_from_top_edge_to_side() {
        let corner = pos2(100.0, 0.0);
        let m = fillet_mesh(corner, 6.0, -1.0, Color32::BLACK);
        let first = m.vertices[1].pos;
        let last = m.vertices.last().unwrap().pos;
        assert!((first - pos2(94.0, 0.0)).length() < 1e-4);
        assert!((last - pos2(100.0, 6.0)).length() < 1e-4);
        // Semua titik busur di dalam persegi telinga.
        for v in &m.vertices {
            assert!(v.pos.x >= 94.0 - 1e-4 && v.pos.x <= 100.0 + 1e-4);
            assert!(v.pos.y >= -1e-4 && v.pos.y <= 6.0 + 1e-4);
        }
        let right = fillet_mesh(pos2(200.0, 0.0), 6.0, 1.0, Color32::BLACK);
        assert!((right.vertices[1].pos - pos2(206.0, 0.0)).length() < 1e-4);
    }
}
