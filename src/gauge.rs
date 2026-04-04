use eframe::egui;
use egui::{Color32, FontId, Painter, Pos2, Sense, Stroke, Ui, Vec2};
use std::f32::consts::PI;

// ── '95 XJ colour palette ────────────────────────────────────────────────────
const BEZEL_DARK: Color32 = Color32::from_rgb(18, 16, 14);
const FACE_BG: Color32 = Color32::from_rgb(22, 20, 18);
const FACE_INNER: Color32 = Color32::from_rgb(12, 11, 10);
const TICK_WHITE: Color32 = Color32::from_rgb(220, 215, 200);
const TICK_DIM: Color32 = Color32::from_rgb(130, 125, 110);
const NEEDLE_BODY: Color32 = Color32::from_rgb(255, 120, 20);
const NEEDLE_TIP: Color32 = Color32::from_rgb(255, 200, 60);
const RED_ZONE: Color32 = Color32::from_rgb(180, 30, 20);
const HUB_DARK: Color32 = Color32::from_rgb(40, 38, 34);
const HUB_RIM: Color32 = Color32::from_rgb(90, 85, 75);
const CHROME: Color32 = Color32::from_rgb(160, 155, 140);
const LABEL_DIM: Color32 = Color32::from_rgb(100, 95, 80);
const LABEL_UNIT: Color32 = Color32::from_rgb(80, 76, 64);
const WINDOW_BG: Color32 = Color32::from_rgb(10, 9, 8);
const CTRL_BG: Color32 = Color32::from_rgb(18, 16, 14);
const CTRL_BORDER: Color32 = Color32::from_rgb(60, 56, 48);

// ── Gauge descriptor ─────────────────────────────────────────────────────────
pub struct Gauge {
    pub label: &'static str,
    pub unit: &'static str,
    pub min: f32,
    pub max: f32,
    /// CCW from 3-o'clock, degrees. Low-value end of sweep.
    pub start_deg: f32,
    /// High-value end of sweep.
    pub end_deg: f32,
    pub red_start: Option<f32>,
    pub major_interval: f32,
    pub minor_per_major: u32,
}

impl Gauge {
    fn angle(&self, value: f32) -> f32 {
        let t = (value - self.min) / (self.max - self.min);
        (self.start_deg + t * (self.end_deg - self.start_deg)).to_radians()
    }

    /// Allocate space in the UI and draw the gauge at the given value.
    /// `label_fn` maps a tick value to its display string.
    pub fn draw(&self, ui: &mut Ui, sz: Vec2, val: f32, label_fn: impl Fn(f32) -> String) {
        let (rect, _) = ui.allocate_exact_size(sz, Sense::hover());
        if !ui.is_rect_visible(rect) {
            return;
        }
        let p = ui.painter_at(rect);
        let c = rect.center();
        let r = sz.x.min(sz.y) * 0.5 - 4.0;
        self.paint(&p, c, r, val, label_fn);
    }

    /// Core painter — draws everything onto `p` centred at `c` with radius `r`.
    fn paint(&self, p: &Painter, c: Pos2, r: f32, val: f32, lbl: impl Fn(f32) -> String) {
        // ── bezel ────────────────────────────────────────────────────────────
        p.circle_filled(c, r + 6.0, BEZEL_DARK);
        p.circle_stroke(c, r + 5.5, Stroke::new(2.5, CHROME));
        p.circle_stroke(c, r + 3.5, Stroke::new(1.0, Color32::from_rgb(60, 55, 48)));

        // ── face ─────────────────────────────────────────────────────────────
        p.circle_filled(c, r, FACE_BG);
        // vignette: a thick ring painted at mid-radius fades the centre darker
        p.circle_stroke(c, r * 0.45, Stroke::new(r * 0.5, FACE_INNER));

        // ── red zone arc ─────────────────────────────────────────────────────
        // If red_start is above the midpoint → danger at high end (tach, temp).
        // If red_start is below the midpoint → danger at low end (fuel).
        if let Some(rs) = self.red_start {
            let (a0, a1) = if rs > (self.min + self.max) * 0.5 {
                (self.angle(rs), self.angle(self.max))
            } else {
                (self.angle(self.min), self.angle(rs))
            };
            arc_band(p, c, r * 0.82, r * 0.91, a0, a1, RED_ZONE);
        }

        // ── tick marks + labels ───────────────────────────────────────────────
        let n_maj = ((self.max - self.min) / self.major_interval).round() as u32;
        let total = n_maj * self.minor_per_major;
        for i in 0..=total {
            let t = i as f32 / total as f32;
            let v = self.min + t * (self.max - self.min);
            let a = self.angle(v);
            let maj = i % self.minor_per_major == 0;
            let (tlen, tw, col) = if maj {
                (r * 0.17, r * 0.021, TICK_WHITE)
            } else {
                (r * 0.09, r * 0.013, TICK_DIM)
            };
            p.line_segment(
                [pol(c, r * 0.91, a), pol(c, r * 0.91 - tlen, a)],
                Stroke::new(tw, col),
            );
            if maj {
                p.text(
                    pol(c, r * 0.68, a),
                    egui::Align2::CENTER_CENTER,
                    lbl(v),
                    FontId::monospace(r * 0.105),
                    TICK_WHITE,
                );
            }
        }

        // ── name + unit ───────────────────────────────────────────────────────
        p.text(
            Pos2::new(c.x, c.y + r * 0.62),
            egui::Align2::CENTER_CENTER,
            self.label,
            FontId::monospace(r * 0.10),
            LABEL_DIM,
        );
        if !self.unit.is_empty() {
            p.text(
                Pos2::new(c.x, c.y + r * 0.45),
                egui::Align2::CENTER_CENTER,
                self.unit,
                FontId::monospace(r * 0.085),
                LABEL_UNIT,
            );
        }

        // ── needle ────────────────────────────────────────────────────────────
        draw_needle(p, c, r, self.angle(val.clamp(self.min, self.max)));

        // ── hub cap ───────────────────────────────────────────────────────────
        p.circle_filled(c, r * 0.09, HUB_DARK);
        p.circle_stroke(c, r * 0.09, Stroke::new(r * 0.021, HUB_RIM));
        p.circle_filled(c, r * 0.04, CHROME);
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn sld(ui: &mut Ui, lbl: &str, v: &mut f32, lo: f32, hi: f32, sfx: &str) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(format!("{:<8}", lbl))
                .size(10.0)
                .color(Color32::from_rgb(140, 135, 115))
                .monospace(),
        );
        ui.add(
            egui::Slider::new(v, lo..=hi)
                .show_value(true)
                .suffix(sfx)
                .trailing_fill(true),
        );
    });
}

fn draw_needle(p: &Painter, c: Pos2, r: f32, a: f32) {
    // drop shadow
    let off = Vec2::new(1.5, 1.5);
    p.add(egui::Shape::convex_polygon(
        vec![
            pol(c, r * 0.83, a) + off,
            pol(c, r * 0.025, a + PI * 0.5) + off,
            pol(c, r * 0.18, a + PI) + off,
            pol(c, r * 0.025, a - PI * 0.5) + off,
        ],
        Color32::from_rgba_premultiplied(0, 0, 0, 80),
        Stroke::NONE,
    ));
    // body
    p.add(egui::Shape::convex_polygon(
        vec![
            pol(c, r * 0.83, a),
            pol(c, r * 0.025, a + PI * 0.5),
            pol(c, r * 0.18, a + PI),
            pol(c, r * 0.025, a - PI * 0.5),
        ],
        NEEDLE_BODY,
        Stroke::NONE,
    ));
    // tip highlight
    p.line_segment(
        [pol(c, r * 0.60, a), pol(c, r * 0.83, a)],
        Stroke::new(1.2, NEEDLE_TIP),
    );
}

fn arc_band(p: &Painter, c: Pos2, ri: f32, ro: f32, a0: f32, a1: f32, col: Color32) {
    // Dense radial spokes — avoids convex_polygon's curvature limitation.
    let band_w = ro - ri;
    let arc_px = (ri + ro) * 0.5 * (a1 - a0).abs();
    let n = ((arc_px / 1.5) as usize).max(8);
    let stroke = Stroke::new(band_w + 1.0, col);
    for i in 0..=n {
        let a = a0 + (i as f32 / n as f32) * (a1 - a0);
        p.line_segment([pol(c, ri, a), pol(c, ro, a)], stroke);
    }
}

/// Polar → screen coords (Y flipped for screen space).
fn pol(c: Pos2, r: f32, a: f32) -> Pos2 {
    Pos2::new(c.x + r * a.cos(), c.y - r * a.sin())
}
