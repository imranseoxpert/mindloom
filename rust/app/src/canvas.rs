//! Paints a laid-out map with egui, the native counterpart of the SVG renderer in `mindloom-core`
//! (same geometry, colours and sizes), and records where the clickable parts are.

use crate::fonts::MapFonts;
use egui::epaint::{Mesh, PathShape, PathStroke, Vertex, WHITE_UV};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, FontFamily, FontId, Painter, Pos2, Rect, Shape, Stroke, StrokeKind, TextureHandle};
use lyon_tessellation::{geometry_builder::simple_builder, math::point, path::Path, FillOptions, FillTessellator, VertexBuffers};
use mindloom_core::js::len16;
use mindloom_core::layout::{self, Extra, Icon, Item, Layout, Orient};
use mindloom_core::model::{Doc, Settings};
use mindloom_core::render;
use mindloom_core::themes::{self, Theme};
use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

pub const ACCENT: Color32 = Color32::from_rgb(0x3b, 0x82, 0xf6);

// ---------- colours ----------

/// A CSS colour as used in themes and documents (`#rgb`, `#rrggbb`, `rgba(…)`, `transparent`).
pub fn color(s: &str) -> Color32 {
    let s = s.trim();
    if let Some(h) = s.strip_prefix('#') {
        let hex = |a: &str| u8::from_str_radix(a, 16).unwrap_or(0);
        return match h.len() {
            3 => {
                let c: Vec<String> = h.chars().map(|c| format!("{c}{c}")).collect();
                Color32::from_rgb(hex(&c[0]), hex(&c[1]), hex(&c[2]))
            }
            6 => Color32::from_rgb(hex(&h[0..2]), hex(&h[2..4]), hex(&h[4..6])),
            8 => Color32::from_rgba_unmultiplied(hex(&h[0..2]), hex(&h[2..4]), hex(&h[4..6]), hex(&h[6..8])),
            _ => Color32::BLACK,
        };
    }
    if let Some(inner) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let v: Vec<f32> = inner.split(',').map(|x| x.trim().parse().unwrap_or(0.0)).collect();
        let a = v.get(3).copied().unwrap_or(1.0);
        return Color32::from_rgba_unmultiplied(
            v[0] as u8,
            v.get(1).copied().unwrap_or(0.0) as u8,
            v.get(2).copied().unwrap_or(0.0) as u8,
            (a * 255.0).round() as u8,
        );
    }
    match s {
        "white" => Color32::WHITE,
        "black" => Color32::BLACK,
        "red" => Color32::from_rgb(255, 0, 0),
        "blue" => Color32::from_rgb(0, 0, 255),
        "green" => Color32::from_rgb(0, 128, 0),
        _ => Color32::TRANSPARENT,
    }
}
pub fn alpha(c: Color32, a: f32) -> Color32 {
    let [r, g, b, a0] = c.to_srgba_unmultiplied();
    Color32::from_rgba_unmultiplied(r, g, b, (a0 as f32 * a).round() as u8)
}

// ---------- view ----------

/// World → screen: `screen = off + world × zoom`.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub zoom: f32,
    pub off: egui::Vec2,
}
impl View {
    pub fn p(&self, x: f64, y: f64) -> Pos2 {
        pos2(self.off.x + x as f32 * self.zoom, self.off.y + y as f32 * self.zoom)
    }
    pub fn s(&self, v: f64) -> f32 {
        v as f32 * self.zoom
    }
    pub fn rect(&self, x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::from_min_size(self.p(x, y), vec2(self.s(w), self.s(h)))
    }
    pub fn to_world(self, p: Pos2) -> (f64, f64) {
        (((p.x - self.off.x) / self.zoom) as f64, ((p.y - self.off.y) / self.zoom) as f64)
    }
}

// ---------- paths ----------

/// The sub-paths of an SVG path string (M L H V Q C Z, absolute), flattened to points.
pub fn flatten(d: &str, steps_per: impl Fn(f64) -> usize) -> Vec<(Vec<(f64, f64)>, bool)> {
    let mut toks: Vec<String> = vec![];
    let mut cur = String::new();
    for ch in d.chars() {
        if ch.is_ascii_alphabetic() && ch != 'e' {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
            toks.push(ch.to_string());
        } else if ch == ' ' || ch == ',' {
            if !cur.is_empty() {
                toks.push(std::mem::take(&mut cur));
            }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    let mut out: Vec<(Vec<(f64, f64)>, bool)> = vec![];
    let mut pts: Vec<(f64, f64)> = vec![];
    let (mut i, mut cmd, mut p) = (0usize, 'M', (0.0, 0.0));
    let num = |i: &mut usize| -> f64 {
        let v = toks.get(*i).and_then(|t| t.parse().ok()).unwrap_or(0.0);
        *i += 1;
        v
    };
    while i < toks.len() {
        if toks[i].len() == 1 && toks[i].chars().all(|c| c.is_ascii_alphabetic()) {
            cmd = toks[i].chars().next().unwrap_or('M');
            i += 1;
            if cmd == 'Z' || cmd == 'z' {
                if !pts.is_empty() {
                    out.push((std::mem::take(&mut pts), true));
                }
                continue;
            }
        }
        match cmd {
            'M' => {
                if pts.len() > 1 {
                    out.push((std::mem::take(&mut pts), false));
                }
                pts.clear();
                p = (num(&mut i), num(&mut i));
                pts.push(p);
                cmd = 'L';
            }
            'L' => {
                p = (num(&mut i), num(&mut i));
                pts.push(p);
            }
            'H' => {
                p = (num(&mut i), p.1);
                pts.push(p);
            }
            'V' => {
                p = (p.0, num(&mut i));
                pts.push(p);
            }
            'Q' => {
                let c = (num(&mut i), num(&mut i));
                let e = (num(&mut i), num(&mut i));
                let n = steps_per((c.0 - p.0).hypot(c.1 - p.1) + (e.0 - c.0).hypot(e.1 - c.1)).max(2);
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    pts.push((u * u * p.0 + 2.0 * u * t * c.0 + t * t * e.0, u * u * p.1 + 2.0 * u * t * c.1 + t * t * e.1));
                }
                p = e;
            }
            'C' => {
                let c1 = (num(&mut i), num(&mut i));
                let c2 = (num(&mut i), num(&mut i));
                let e = (num(&mut i), num(&mut i));
                let n = steps_per((c1.0 - p.0).hypot(c1.1 - p.1) + (c2.0 - c1.0).hypot(c2.1 - c1.1) + (e.0 - c2.0).hypot(e.1 - c2.1)).max(2);
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let u = 1.0 - t;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    pts.push((a * p.0 + b * c1.0 + c * c2.0 + d * e.0, a * p.1 + b * c1.1 + c * c2.1 + d * e.1));
                }
                p = e;
            }
            _ => i += 1,
        }
    }
    if pts.len() > 1 {
        out.push((pts, false));
    }
    out
}

/// Any simple polygon, filled (triangulated with lyon) and outlined thinly so its edges are smooth.
pub fn fill_poly(painter: &Painter, pts: &[Pos2], fill: Color32) {
    if pts.len() < 3 || fill.a() == 0 {
        return;
    }
    let mut b = Path::builder();
    b.begin(point(pts[0].x, pts[0].y));
    for p in &pts[1..] {
        b.line_to(point(p.x, p.y));
    }
    b.end(true);
    let path = b.build();
    let mut buf: VertexBuffers<lyon_tessellation::math::Point, u16> = VertexBuffers::new();
    if FillTessellator::new().tessellate_path(&path, &FillOptions::tolerance(0.2), &mut simple_builder(&mut buf)).is_err() {
        return;
    }
    let mesh = Mesh {
        vertices: buf.vertices.iter().map(|v| Vertex { pos: pos2(v.x, v.y), uv: WHITE_UV, color: fill }).collect(),
        indices: buf.indices.iter().map(|i| *i as u32).collect(),
        ..Default::default()
    };
    painter.add(Shape::mesh(mesh));
    painter.add(Shape::closed_line(pts.to_vec(), Stroke::new(0.7, fill)));
}

/// A pie slice from angle `a0` to `a1` (radians), as a triangle fan.
fn fan(painter: &Painter, c: Pos2, r: f32, a0: f64, a1: f64, fill: Color32) {
    let n = (((a1 - a0).abs() / (PI / 24.0)).ceil() as usize).max(2);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, fill);
    for k in 0..=n {
        let a = a0 + (a1 - a0) * k as f64 / n as f64;
        mesh.colored_vertex(c + vec2(a.cos() as f32, a.sin() as f32) * r, fill);
        if k > 0 {
            mesh.add_triangle(0, k as u32, k as u32 + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn stroke_line(painter: &Painter, pts: Vec<Pos2>, width: f32, color: Color32, round: bool) {
    if pts.len() < 2 || color.a() == 0 || width <= 0.0 {
        return;
    }
    if round && width > 2.0 {
        let r = width / 2.0;
        painter.circle_filled(pts[0], r, color);
        painter.circle_filled(*pts.last().unwrap_or(&pts[0]), r, color);
    }
    painter.add(Shape::line(pts, PathStroke::new(width, color)));
}

fn dashed(painter: &Painter, pts: &[Pos2], width: f32, color: Color32, dash: f32, gap: f32, closed: bool) {
    let mut v = pts.to_vec();
    if closed && !v.is_empty() {
        v.push(v[0]);
    }
    if v.len() < 2 || dash <= 0.2 {
        return;
    }
    painter.extend(Shape::dashed_line(&v, Stroke::new(width, color), dash, gap));
}

// ---------- shapes ----------

/// The outline of a node shape (world coordinates relative to the node), for fills and dashed borders.
pub fn outline(shape: &str, w: f64, h: f64, is_root: bool) -> Vec<(f64, f64)> {
    let rr = |r: f64| -> Vec<(f64, f64)> {
        let r = r.min(w / 2.0).min(h / 2.0);
        let mut v = vec![];
        for (cx, cy, a0) in [(w - r, r, -PI / 2.0), (w - r, h - r, 0.0), (r, h - r, PI / 2.0), (r, r, PI)] {
            for k in 0..=6 {
                let a = a0 + PI / 2.0 * k as f64 / 6.0;
                v.push((cx + a.cos() * r, cy + a.sin() * r));
            }
        }
        v
    };
    let ell = |rx: f64, ry: f64| (0..48).map(|k| (k as f64 / 48.0 * PI * 2.0).sin_cos()).map(|(s, c)| (w / 2.0 + c * rx, h / 2.0 + s * ry)).collect();
    match shape {
        "rect" => rr(2.0),
        "pill" => rr(h / 2.0),
        "ellipse" => ell(w / 2.0, h / 2.0),
        "circle" => ell(w / 2.0, w / 2.0),
        "diamond" => vec![(w / 2.0, 0.0), (w, h / 2.0), (w / 2.0, h), (0.0, h / 2.0)],
        "hexagon" => {
            let k = h * 0.35;
            vec![(k, 0.0), (w - k, 0.0), (w, h / 2.0), (w - k, h), (k, h), (0.0, h / 2.0)]
        }
        "parallelogram" => {
            let k = h * 0.3;
            vec![(k, 0.0), (w, 0.0), (w - k, h), (0.0, h)]
        }
        "cloud" => {
            let pts = render::cloud_points(w, h);
            let mut v = vec![(pts[0].0, pts[0].1)];
            for win in pts.windows(2) {
                let ((x0, y0, _), (x1, y1, r)) = (win[0], win[1]);
                // arc of radius r from (x0,y0) to (x1,y1), bulging outward (sweep = 1)
                let (mx, my) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                let half = ((x1 - x0).hypot(y1 - y0) / 2.0).min(r);
                let dist = (r * r - half * half).max(0.0).sqrt();
                let (dx, dy) = ((x1 - x0) / (2.0 * half.max(1e-9)), (y1 - y0) / (2.0 * half.max(1e-9)));
                let (cx, cy) = (mx - dy * dist, my + dx * dist);
                let (a0, mut a1) = ((y0 - cy).atan2(x0 - cx), (y1 - cy).atan2(x1 - cx));
                if a1 < a0 {
                    a1 += PI * 2.0;
                }
                for k in 1..=8 {
                    let a = a0 + (a1 - a0) * k as f64 / 8.0;
                    v.push((cx + a.cos() * r, cy + a.sin() * r));
                }
            }
            v
        }
        _ => rr(if is_root { 12.0 } else { 8f64.min(h / 3.0) }),
    }
}

fn paint_shape(c: &Canvas, t: &Item, is_root: bool, fade: f32) {
    let st = &t.b.style;
    let v = &c.view;
    let ns = t.node.style.as_ref();
    let fill = alpha(color(&st.fill), fade);
    let sw = st.border_width;
    let border = alpha(color(&st.border), fade);
    let rect = v.rect(t.x, t.y, t.w, t.h);
    let shape = st.shape.as_str();
    let painter = c.painter;
    match shape {
        "underline" | "none" => {
            let bg = ns.and_then(|s| mindloom_core::js::some_str(&s.fill)).map(color).unwrap_or(Color32::TRANSPARENT);
            painter.rect_filled(rect, CornerRadius::same(v.s(4.0).min(255.0) as u8), alpha(bg, fade));
            if shape == "underline" {
                let bw = if st.border_width != 0.0 { st.border_width } else { 1.2 };
                let lw = 1.5f64.max(bw + 0.8);
                let col = if ns.and_then(|s| mindloom_core::js::some_str(&s.border)).is_some() { border } else { alpha(color(&st.line), fade) };
                stroke_line(painter, vec![v.p(t.x, t.y + t.h), v.p(t.x + t.w, t.y + t.h)], v.s(lw), col, true);
            }
            return;
        }
        _ => {}
    }
    let solid = st.border_dash != "dashed" && st.border_dash != "dotted";
    let stroke = if sw > 0.0 && border.a() > 0 && solid { Stroke::new(v.s(sw), border) } else { Stroke::NONE };
    let pts = || outline(shape, t.w, t.h, is_root).into_iter().map(|(x, y)| v.p(t.x + x, t.y + y)).collect::<Vec<_>>();
    match shape {
        "rounded" | "rect" | "pill" => {
            let r = match shape {
                "rect" => 2.0,
                "pill" => t.h / 2.0,
                _ if is_root => 12.0,
                _ => 8f64.min(t.h / 3.0),
            };
            painter.rect(rect, CornerRadius::same(v.s(r).clamp(0.0, 255.0) as u8), fill, stroke, StrokeKind::Middle);
        }
        "ellipse" => {
            painter.add(Shape::ellipse_filled(rect.center(), rect.size() / 2.0, fill));
            if stroke.width > 0.0 {
                painter.add(Shape::ellipse_stroke(rect.center(), rect.size() / 2.0, stroke));
            }
        }
        "circle" => {
            let p = v.p(t.x + t.w / 2.0, t.y + t.h / 2.0);
            painter.circle(p, v.s(t.w / 2.0), fill, stroke);
        }
        "diamond" | "hexagon" | "parallelogram" => {
            painter.add(Shape::Path(PathShape::convex_polygon(pts(), fill, stroke)));
        }
        _ => {
            let p = pts();
            fill_poly(painter, &p, fill);
            if stroke.width > 0.0 {
                painter.add(Shape::closed_line(p, stroke));
            }
        }
    }
    if sw > 0.0 && !solid {
        let (dash, gap) = if st.border_dash == "dashed" { (6.0, 4.0) } else { (1.5, 3.0) };
        dashed(painter, &pts(), v.s(sw), border, v.s(dash), v.s(gap), true);
    }
}

// ---------- icons ----------

/// Where a node's icons sit (world coordinates): `(icon, x, y, size)`, lead icons then trailing ones.
pub fn icon_slots<'i>(t: &'i Item) -> Vec<(&'i Icon, f64, f64, f64)> {
    let b = &t.b;
    let iy = t.y + b.row.y + (b.row.h - b.icon_size) / 2.0;
    let mut out = vec![];
    let mut ix = t.x + b.row.x;
    for ic in &b.lead {
        out.push((ic, ix, iy, b.icon_size));
        ix += b.icon_size + b.icon_gap;
    }
    ix = t.x + b.row.x + b.row.text_x + b.row.text_w + b.icon_gap;
    for ic in &b.trail {
        out.push((ic, ix, iy, b.icon_size));
        ix += b.icon_size + b.icon_gap;
    }
    out
}

fn star_pts(cx: f64, cy: f64, r: f64) -> Vec<(f64, f64)> {
    (0..10)
        .map(|i| {
            let rr = if i % 2 == 1 { r * 0.45 } else { r };
            let a = -PI / 2.0 + (i as f64 * PI) / 5.0;
            (cx + a.cos() * rr, cy + a.sin() * rr)
        })
        .collect()
}

fn lookup(table: &[(&str, &'static str)], k: &str) -> Option<&'static str> {
    table.iter().find(|(a, _)| *a == k).map(|(_, v)| *v)
}

pub fn paint_icon(c: &mut Canvas, icon: &Icon, x: f64, y: f64, s: f64, ink: Color32) {
    let v = &c.view;
    let p = c.painter;
    let (cx, cy, r) = (x + s / 2.0, y + s / 2.0, s / 2.0);
    let line = |pts: &[(f64, f64)], w: f64, col: Color32| stroke_line(p, pts.iter().map(|&(a, b)| v.p(a, b)).collect(), v.s(w), col, true);
    match icon {
        Icon::Task(state) if state == "done" => {
            p.rect_filled(v.rect(x + 1.0, y + 1.0, s - 2.0, s - 2.0), CornerRadius::same(v.s(4.0).min(255.0) as u8), color("#2f9e44"));
            let (x0, y0) = (x + s * 0.27, cy);
            line(&[(x0, y0), (x0 + s * 0.16, y0 + s * 0.17), (x0 + s * 0.48, y0 - s * 0.17)], s * 0.12, Color32::WHITE);
        }
        Icon::Task(_) => {
            p.rect(
                v.rect(x + 1.5, y + 1.5, s - 3.0, s - 3.0),
                CornerRadius::same(v.s(4.0).min(255.0) as u8),
                Color32::WHITE,
                Stroke::new(v.s(1.6), ink),
                StrokeKind::Middle,
            );
        }
        Icon::Link(_) => {
            c.glyph(egui_phosphor::regular::LINK, v.p(cx, cy), v.s(s * 0.95), color("#1c7ed6"));
        }
        Icon::Note(_) => {
            p.rect(
                v.rect(x + s * 0.15, y + s * 0.08, s * 0.7, s * 0.84),
                CornerRadius::same(v.s(2.0).min(255.0) as u8),
                color("#fff3bf"),
                Stroke::new(v.s(1.2), color("#f59f00")),
                StrokeKind::Middle,
            );
            for (yy, ww) in [(0.35, 0.4), (0.52, 0.4), (0.69, 0.25)] {
                stroke_line(p, vec![v.p(x + s * 0.3, y + s * yy), v.p(x + s * (0.3 + ww), y + s * yy)], v.s(1.2), color("#e67700"), false);
            }
        }
        Icon::Marker(m) => {
            let b = m.as_bytes();
            if b.len() == 2 && b[0] == b'p' && b[1].is_ascii_digit() {
                let d = (b[1] - b'0') as usize;
                let col = if d >= 1 { color(themes::PRIORITY_COLORS[(d - 1) % 6]) } else { Color32::GRAY };
                p.circle_filled(v.p(cx, cy), v.s(r), col);
                c.text(
                    &d.to_string(),
                    v.p(cx, cy + 0.5),
                    Align2::CENTER_CENTER,
                    FontId::new(v.s(s * 0.62), FontFamily::Name("ui-bold".into())),
                    Color32::WHITE,
                );
            } else if let Some(pct) = m.strip_prefix("pr").filter(|q| !q.is_empty() && q.bytes().all(|c| c.is_ascii_digit())) {
                let frac = pct.parse::<f64>().unwrap_or(0.0).min(100.0) / 100.0;
                p.circle(v.p(cx, cy), v.s(r - 1.0), Color32::WHITE, Stroke::new(v.s(1.5), color("#1c7ed6")));
                if frac >= 1.0 {
                    p.circle_filled(v.p(cx, cy), v.s(r - 1.0), color("#2f9e44"));
                    let x0 = x + s * 0.28;
                    line(&[(x0, cy), (x0 + s * 0.15, cy + s * 0.16), (x0 + s * 0.45, cy - s * 0.16)], s * 0.11, Color32::WHITE);
                } else if frac > 0.0 {
                    fan(p, v.p(cx, cy), v.s(r - 1.0), -PI / 2.0, frac * PI * 2.0 - PI / 2.0, color("#1c7ed6"));
                }
            } else if let Some(k) = m.strip_prefix("flag-") {
                line(&[(x + s * 0.22, y + s * 0.95), (x + s * 0.22, y + s * 0.08)], 1.4, color("#555555"));
                let pts: Vec<Pos2> =
                    [(0.24, 0.1), (0.88, 0.1), (0.72, 0.33), (0.88, 0.56), (0.24, 0.56)].iter().map(|&(a, b)| v.p(x + s * a, y + s * b)).collect();
                fill_poly(p, &pts, color(lookup(themes::FLAG_COLORS, k).unwrap_or("#e03131")));
            } else if let Some(k) = m.strip_prefix("star-") {
                let pts: Vec<Pos2> = star_pts(cx, cy + 0.5, r).into_iter().map(|(a, b)| v.p(a, b)).collect();
                fill_poly(p, &pts, color(lookup(themes::STAR_COLORS, k).unwrap_or("#f59f00")));
            } else {
                let ctx = p.ctx().clone();
                match c.images.emoji(&ctx, m) {
                    Some(tex) => {
                        let r = egui::Rect::from_center_size(v.p(cx, cy), egui::vec2(v.s(s), v.s(s)));
                        p.image(tex.id(), r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    None => {
                        c.text(m, v.p(cx, cy + 1.0), Align2::CENTER_CENTER, FontId::new(v.s(s * 0.86), c.emoji_family()), ink);
                    }
                }
            }
        }
    }
}

// ---------- painting a whole map ----------

pub struct Canvas<'a> {
    pub painter: &'a Painter,
    pub view: View,
    pub fonts: &'a MapFonts,
    pub images: &'a mut ImageCache,
}

impl Canvas<'_> {
    fn text(&self, s: &str, at: Pos2, align: Align2, font: FontId, col: Color32) -> Rect {
        if font.size < 0.5 {
            return Rect::NOTHING;
        }
        self.painter.text(at, align, s, font, col)
    }
    fn glyph(&self, g: &str, at: Pos2, size: f32, col: Color32) {
        self.text(g, at, Align2::CENTER_CENTER, FontId::new(size, FontFamily::Name("icons".into())), col);
    }
    fn emoji_family(&self) -> FontFamily {
        self.fonts.families[0][0].clone()
    }
}

pub struct PaintOpts<'a> {
    pub theme: &'a Theme,
    pub settings: &'a Settings,
    pub selected: &'a [String],
    pub matches: &'a [String],
    pub selected_rel: Option<&'a str>,
    /// Topics being dragged (drawn slightly faded).
    pub faded: &'a HashSet<String>,
    pub hover: Option<&'a str>,
    /// Hide the label of the topic being edited (the text box sits on top).
    pub editing: Option<&'a str>,
    /// Export look: no expand handles, no placeholder text, no selection.
    pub export: bool,
}

fn count_all(n: &mindloom_core::model::Node) -> usize {
    n.children.iter().map(|c| 1 + count_all(c)).sum()
}

fn paint_node(c: &mut Canvas, t: &Item, o: &PaintOpts) {
    let b = &t.b;
    let st = &b.style;
    let v = c.view;
    let is_root = t.depth == 0 && !t.floating;
    let fade = if o.faded.contains(t.id) { 0.82 } else { 1.0 };
    paint_shape(c, t, is_root, fade);
    if let (Some((iw, ih)), Some((ix, iy)), Some(im)) = (b.img, b.img_pos, t.node.image.as_ref()) {
        let r = v.rect(t.x + ix, t.y + iy, iw, ih);
        match c.images.get(c.painter.ctx(), &im.src) {
            Some(tex) => {
                let sz = tex.size_vec2();
                let k = (r.width() / sz.x).min(r.height() / sz.y);
                let fit = Rect::from_center_size(r.center(), sz * k);
                c.painter.image(tex.id(), fit, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), alpha(Color32::WHITE, fade));
            }
            None => {
                c.painter.rect_stroke(r, CornerRadius::same(4), Stroke::new(1.0, Color32::GRAY), StrokeKind::Inside);
            }
        }
    }
    let ink = alpha(color(&st.color), fade);
    for (ic, x, y, s) in icon_slots(t) {
        paint_icon(c, ic, x, y, s, ink);
    }
    if o.editing != Some(t.id) {
        let row = b.row;
        let tx0 = t.x + row.x + row.text_x;
        let (align, tx) = match st.align.as_str() {
            "left" => (Align2::LEFT_CENTER, tx0),
            "right" => (Align2::RIGHT_CENTER, tx0 + row.text_w),
            _ => (Align2::CENTER_CENTER, tx0 + row.text_w / 2.0),
        };
        let ty0 = t.y + row.y + (row.h - row.text_h) / 2.0;
        let placeholder = t.node.text.is_empty();
        let ph = [if o.export { String::new() } else { "Topic".to_string() }];
        let lines: &[String] = if placeholder { &ph } else { &b.lines };
        let font = FontId::new(v.s(st.font_size), c.fonts.family(&b.font));
        let col = if placeholder { alpha(ink, 0.35) } else { ink };
        for (i, ln) in lines.iter().enumerate() {
            let cy = ty0 + i as f64 * b.lh + b.lh / 2.0;
            let r = c.text(ln, v.p(tx, cy), align, font.clone(), col);
            if r.is_positive() {
                let lw = v.s(st.font_size / 14.0).max(0.6);
                if st.underline {
                    c.painter.line_segment(
                        [pos2(r.left(), r.center().y + v.s(st.font_size * 0.42)), pos2(r.right(), r.center().y + v.s(st.font_size * 0.42))],
                        Stroke::new(lw, col),
                    );
                }
                if st.strike {
                    c.painter.line_segment(
                        [pos2(r.left(), r.center().y + v.s(st.font_size * 0.05)), pos2(r.right(), r.center().y + v.s(st.font_size * 0.05))],
                        Stroke::new(lw, col),
                    );
                }
            }
        }
    }
    let mut gx = t.x + b.tags_x;
    let tag_font = FontId::new(v.s(11.0), c.fonts.family(&layout::FontSpec { size: 11.0, weight: 700, italic: false, family: st.font }));
    for tg in &b.tags {
        c.painter.rect_filled(v.rect(gx, t.y + b.tags_y, tg.w, 18.0), CornerRadius::same(v.s(9.0).min(255.0) as u8), alpha(color(&st.line), 0.18 * fade));
        c.text(&tg.text, v.p(gx + tg.w / 2.0, t.y + b.tags_y + 9.5), Align2::CENTER_CENTER, tag_font.clone(), alpha(ink, 0.85));
        gx += tg.w + 4.0;
    }
    if !t.node.children.is_empty() {
        let (px, py) = render::toggle_pos(t);
        let (px, py) = (t.x + px, t.y + py);
        let bg = color(o.theme.bg);
        let line = color(&st.line);
        if t.node.is_collapsed() {
            let cnt = count_all(t.node);
            c.painter.circle(v.p(px, py), v.s(10.0), bg, Stroke::new(v.s(1.6), line));
            c.text(
                &cnt.to_string(),
                v.p(px, py + 0.5),
                Align2::CENTER_CENTER,
                FontId::new(v.s(if cnt > 99 { 8.0 } else { 10.0 }), FontFamily::Name("ui-bold".into())),
                line,
            );
        } else if !o.export && !(is_root && o.settings.layout == "mindmap") && (o.hover == Some(t.id) || o.selected.iter().any(|s| s == t.id)) {
            c.painter.circle(v.p(px, py), v.s(7.0), bg, Stroke::new(v.s(1.4), line));
            stroke_line(c.painter, vec![v.p(px - 3.5, py), v.p(px + 3.5, py)], v.s(1.6), line, true);
        }
    }
}

/// Paints the map: boundaries, extras, connectors, topics, relationships and (unless exporting) the overlay.
pub fn paint(c: &mut Canvas, l: &Layout, doc: &Doc, o: &PaintOpts) {
    let theme = o.theme;
    let s = o.settings;
    let v = c.view;
    let zoom = v.zoom as f64;
    let steps = |len: f64| ((len * zoom) / 5.0).clamp(4.0, 80.0) as usize;
    let map_font = |size: f64, bold: bool| {
        FontId::new(
            v.s(size),
            c.fonts.family(&layout::FontSpec { size, weight: if bold { 700 } else { 400 }, italic: false, family: themes::get_font(&s.font) }),
        )
    };
    for b in &l.boundaries {
        let col = color(&b.color);
        let r = v.rect(b.x, b.y, b.w, b.h);
        c.painter.rect_filled(r, CornerRadius::same(v.s(14.0).min(255.0) as u8), alpha(col, 0.07));
        let ring: Vec<Pos2> = rounded_ring(b.w, b.h, 14.0).into_iter().map(|(x, y)| v.p(b.x + x, b.y + y)).collect();
        dashed(c.painter, &ring, v.s(1.5), col, v.s(6.0), v.s(5.0), true);
        if !b.label.is_empty() {
            let lw = (b.w - 20.0).min(len16(&b.label) as f64 * 7.5 + 20.0);
            c.painter.rect_filled(v.rect(b.x + 10.0, b.y + 6.0, lw, 20.0), CornerRadius::same(v.s(10.0).min(255.0) as u8), col);
            let clip = c.painter.with_clip_rect(v.rect(b.x + 10.0, b.y + 6.0, lw, 20.0).intersect(c.painter.clip_rect()));
            clip.text(v.p(b.x + 20.0, b.y + 16.5), Align2::LEFT_CENTER, &b.label, map_font(12.0, true), color(themes::contrast_text(&b.color)));
        }
    }
    let line = color(theme.line);
    for e in &l.extras {
        match *e {
            Extra::Spine { x1, x2, y } => stroke_line(c.painter, vec![v.p(x1, y), v.p(x2, y)], v.s(s.line_width * 2.4), line, true),
            Extra::Axis { x1, x2, y } => {
                stroke_line(c.painter, vec![v.p(x1, y), v.p(x2, y)], v.s(s.line_width * 2.0), line, true);
                c.painter.add(Shape::convex_polygon(vec![v.p(x2 - 2.0, y - 7.0), v.p(x2 + 10.0, y), v.p(x2 - 2.0, y + 7.0)], line, Stroke::NONE));
            }
        }
    }
    for e in &l.edges {
        let col = color(&e.color);
        let col = if o.faded.contains(l.items[e.to].id) { alpha(col, 0.82) } else { col };
        if e.orient == Orient::AxisDot {
            c.painter.circle_filled(v.p(e.p1.0, e.p1.1), v.s(s.line_width + 2.5), col);
            continue;
        }
        let bone = e.orient == Orient::Bone;
        let p = layout::edge_path(e, if bone { "straight" } else { &s.line_style });
        let w = if bone { e.width * 1.4 } else { e.width };
        for (pts, _) in flatten(&p.d, steps) {
            let sp: Vec<Pos2> = pts.iter().map(|&(x, y)| v.p(x, y)).collect();
            if p.filled {
                fill_poly(c.painter, &sp, col);
            } else {
                stroke_line(c.painter, sp, v.s(w), col, true);
            }
        }
    }
    for t in &l.items {
        paint_node(c, t, o);
    }
    for r in &doc.relations {
        let (Some(a), Some(b)) = (l.get(&r.from), l.get(&r.to)) else { continue };
        let rp = layout::relation_path(a, b, r.bend);
        let rc = color(mindloom_core::js::some_str(&r.color).unwrap_or(if theme.dark { "#f8f9fa" } else { "#5c677d" }));
        let sel = o.selected_rel == Some(r.id.as_str());
        let col = if sel { ACCENT } else { rc };
        for (pts, _) in flatten(&rp.d, steps) {
            let sp: Vec<Pos2> = pts.iter().map(|&(x, y)| v.p(x, y)).collect();
            dashed(c.painter, &sp, v.s(if sel { 2.4 } else { 1.7 }), col, v.s(7.0), v.s(5.0), false);
        }
        let [tip, p1, p2] = render::arrow_head(&rp);
        c.painter.add(Shape::convex_polygon(vec![v.p(tip.0, tip.1), v.p(p1.0, p1.1), v.p(p2.0, p2.1)], col, Stroke::NONE));
        if !r.label.is_empty() {
            let lw = len16(&r.label) as f64 * 7.0 + 16.0;
            c.painter.rect(
                v.rect(rp.mid.0 - lw / 2.0, rp.mid.1 - 11.0, lw, 22.0),
                CornerRadius::same(v.s(11.0).min(255.0) as u8),
                color(theme.bg),
                Stroke::new(v.s(1.0), rc),
                StrokeKind::Middle,
            );
            c.text(&r.label, v.p(rp.mid.0, rp.mid.1 + 0.5), Align2::CENTER_CENTER, map_font(12.0, false), rc);
        }
    }
    if !o.export {
        for id in o.matches {
            if let Some(t) = l.get(id) {
                let ring: Vec<Pos2> = rounded_ring(t.w + 10.0, t.h + 10.0, 10.0).into_iter().map(|(x, y)| v.p(t.x - 5.0 + x, t.y - 5.0 + y)).collect();
                dashed(c.painter, &ring, v.s(3.0), color("#fab005"), v.s(5.0), v.s(3.0), true);
            }
        }
        for id in o.selected {
            if let Some(t) = l.get(id) {
                c.painter.rect_stroke(
                    v.rect(t.x - 4.5, t.y - 4.5, t.w + 9.0, t.h + 9.0),
                    CornerRadius::same(v.s(10.0).min(255.0) as u8),
                    Stroke::new(v.s(2.5).max(1.5), ACCENT),
                    StrokeKind::Middle,
                );
            }
        }
    }
}

/// A rounded rectangle outline from (0,0) to (w,h).
pub fn rounded_ring(w: f64, h: f64, r: f64) -> Vec<(f64, f64)> {
    let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
    let mut v = vec![];
    for (cx, cy, a0) in [(w - r, r, -PI / 2.0), (w - r, h - r, 0.0), (r, h - r, PI / 2.0), (r, r, PI)] {
        for k in 0..=6 {
            let a = a0 + PI / 2.0 * k as f64 / 6.0;
            v.push((cx + a.cos() * r, cy + a.sin() * r));
        }
    }
    v
}

/// The background colour and pattern, aligned with the map so it moves and scales with it.
pub fn paint_background(painter: &Painter, rect: Rect, view: View, pattern: &str, theme: &Theme) {
    painter.rect_filled(rect, CornerRadius::ZERO, color(theme.bg));
    let Some(tile) = render::pattern_tile(pattern, theme.ink) else { return };
    let ink = color(theme.ink);
    let (tw, th) = (view.s(tile.size), view.s(tile.h));
    if tw < 7.0 {
        return;
    }
    let (wx0, wy0) = view.to_world(rect.min);
    let (i0, j0) = ((wx0 / tile.size).floor() as i64, (wy0 / tile.h).floor() as i64);
    let (ni, nj) = ((rect.width() / tw).ceil() as i64 + 2, (rect.height() / th).ceil() as i64 + 2);
    if ni * nj > 40_000 {
        return;
    }
    let p = painter.with_clip_rect(rect);
    let lw = |w: f64| view.s(w).max(0.6);
    let (x0, y0) = (i0 as f64 * tile.size, j0 as f64 * tile.h);
    let (x1, y1) = (x0 + ni as f64 * tile.size, y0 + nj as f64 * tile.h);
    let hline = |y: f64, w: f32, col: Color32| p.line_segment([view.p(x0, y), view.p(x1, y)], Stroke::new(w, col));
    let vline = |x: f64, w: f32, col: Color32| p.line_segment([view.p(x, y0), view.p(x, y1)], Stroke::new(w, col));
    match pattern {
        "dots" | "cross" | "isometric" => {
            for i in 0..ni {
                for j in 0..nj {
                    let (bx, by) = ((i0 + i) as f64 * tile.size, (j0 + j) as f64 * tile.h);
                    match pattern {
                        "dots" => {
                            p.circle_filled(view.p(bx + 11.0, by + 11.0), view.s(1.4).max(0.7), ink);
                        }
                        "cross" => {
                            p.line_segment([view.p(bx + 14.0, by + 9.0), view.p(bx + 14.0, by + 19.0)], Stroke::new(lw(1.3), ink));
                            p.line_segment([view.p(bx + 9.0, by + 14.0), view.p(bx + 19.0, by + 14.0)], Stroke::new(lw(1.3), ink));
                        }
                        _ => {
                            for (dx, dy) in [(0.0, 0.0), (12.0, 21.0)] {
                                p.circle_filled(view.p(bx + dx, by + dy), view.s(1.3).max(0.7), ink);
                            }
                        }
                    }
                }
            }
        }
        "grid" => {
            for i in 0..=ni {
                vline((i0 + i) as f64 * tile.size, lw(1.0), ink);
            }
            for j in 0..=nj {
                hline((j0 + j) as f64 * tile.h, lw(1.0), ink);
            }
        }
        "graph" => {
            let minor = alpha(ink, 0.55);
            for i in 0..=ni * 5 {
                let x = x0 + i as f64 * 20.0;
                vline(x, if i % 5 == 0 { lw(1.4) } else { lw(0.6) }, if i % 5 == 0 { ink } else { minor });
            }
            for j in 0..=nj * 5 {
                let y = y0 + j as f64 * 20.0;
                hline(y, if j % 5 == 0 { lw(1.4) } else { lw(0.6) }, if j % 5 == 0 { ink } else { minor });
            }
        }
        "lines" => {
            for j in 0..=nj {
                hline((j0 + j) as f64 * tile.h + 29.5, lw(1.0), ink);
            }
        }
        "diagonal" => {
            for k in 0..=(ni + nj) {
                let c = x0 + y0 + k as f64 * tile.size;
                p.line_segment([view.p(c - y0, y0), view.p(c - y1, y1)], Stroke::new(lw(1.0), ink));
            }
        }
        _ => {}
    }
}

// ---------- images ----------

/// Decoded node images, by their data URL.
#[derive(Default)]
pub struct ImageCache {
    map: HashMap<u64, Option<TextureHandle>>,
    emoji: HashMap<String, Option<TextureHandle>>,
    /// Fonts for drawing colour emoji (egui's own emoji font is monochrome and incomplete).
    pub db: Option<std::sync::Arc<resvg::usvg::fontdb::Database>>,
}

impl ImageCache {
    pub fn with_fonts(db: std::sync::Arc<resvg::usvg::fontdb::Database>) -> Self {
        ImageCache { db: Some(db), ..Default::default() }
    }
    /// A colour emoji as a texture, rendered once from the system emoji font.
    pub fn emoji(&mut self, ctx: &egui::Context, e: &str) -> Option<TextureHandle> {
        let db = self.db.clone()?;
        self.emoji
            .entry(e.to_string())
            .or_insert_with(|| {
                let svg = format!(
                    "<svg xmlns='http://www.w3.org/2000/svg' width='72' height='72'><text x='36' y='58' font-size='58' text-anchor='middle' font-family=\"Apple Color Emoji, Segoe UI Emoji, Noto Color Emoji, Twemoji Mozilla\">{}</text></svg>",
                    render::esc(e)
                );
                let pm = crate::files::raster(&svg, 72.0, 72.0, 1.0, &db).ok()?;
                let rgba = crate::files::rgba(&pm);
                if !rgba.chunks(4).any(|p| p[3] > 0) {
                    return None;
                }
                let ci = egui::ColorImage::from_rgba_unmultiplied([pm.width() as usize, pm.height() as usize], &rgba);
                Some(ctx.load_texture(format!("emoji-{e}"), ci, egui::TextureOptions::LINEAR))
            })
            .clone()
    }

    pub fn get(&mut self, ctx: &egui::Context, src: &str) -> Option<TextureHandle> {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        src.hash(&mut h);
        let key = h.finish();
        self.map
            .entry(key)
            .or_insert_with(|| {
                let bytes = crate::files::data_url_bytes(src)?;
                let img = image::load_from_memory(&bytes).ok()?.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
                Some(ctx.load_texture(format!("img-{key}"), ci, egui::TextureOptions::LINEAR))
            })
            .clone()
    }
}
