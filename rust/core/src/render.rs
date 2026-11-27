//! SVG renderer (port of `render.js`): the scene string and standalone SVG export.

use crate::js::{len16, n, num, slice16};
use crate::layout::{self, Extra, Icon, Item, Layout, Orient};
use crate::model::{Doc, Settings};
use crate::themes::{self, Theme};
use std::f64::consts::PI;
use std::fmt::Write;

pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub struct Tile {
    pub size: f64,
    pub h: f64,
    pub body: String,
}

/// One tile of a background pattern.
pub fn pattern_tile(id: &str, ink: &str) -> Option<Tile> {
    let t = |size: f64, h: f64, body: String| Some(Tile { size, h, body });
    match id {
        "dots" => t(22.0, 22.0, format!(r#"<circle cx="11" cy="11" r="1.4" fill="{ink}"/>"#)),
        "grid" => t(24.0, 24.0, format!(r#"<path d="M24 0H0V24" fill="none" stroke="{ink}" stroke-width="1"/>"#)),
        "graph" => t(
            100.0,
            100.0,
            format!(
                r#"<path d="M20 0V100M40 0V100M60 0V100M80 0V100M0 20H100M0 40H100M0 60H100M0 80H100" stroke="{ink}" stroke-opacity="0.55" stroke-width="0.6" fill="none"/><path d="M100 0H0V100" fill="none" stroke="{ink}" stroke-width="1.4"/>"#
            ),
        ),
        "lines" => t(30.0, 30.0, format!(r#"<path d="M0 29.5H30" stroke="{ink}" stroke-width="1"/>"#)),
        "cross" => t(28.0, 28.0, format!(r#"<path d="M14 9V19M9 14H19" stroke="{ink}" stroke-width="1.3" stroke-linecap="round"/>"#)),
        "diagonal" => t(18.0, 18.0, format!(r#"<path d="M-2 2L2 -2M0 18L18 0M16 20L20 16" stroke="{ink}" stroke-width="1"/>"#)),
        "isometric" => t(
            24.0,
            42.0,
            format!(
                r#"<g fill="{ink}"><circle cx="0" cy="0" r="1.3"/><circle cx="24" cy="0" r="1.3"/><circle cx="12" cy="21" r="1.3"/><circle cx="0" cy="42" r="1.3"/><circle cx="24" cy="42" r="1.3"/></g>"#
            ),
        ),
        _ => None,
    }
}

/// Points of the cloud shape (start, then arcs of radius `r` to each point).
pub fn cloud_points(w: f64, h: f64) -> Vec<(f64, f64, f64)> {
    let (cx, cy, rx, ry, k) = (w / 2.0, h / 2.0, w / 2.0 - 6.0, h / 2.0 - 6.0, 11usize);
    let pts: Vec<(f64, f64)> = (0..k)
        .map(|i| {
            let a = (i as f64 / k as f64) * PI * 2.0 - PI / 2.0;
            (cx + a.cos() * rx, cy + a.sin() * ry)
        })
        .collect();
    let mut out = vec![(pts[0].0, pts[0].1, 0.0)];
    for i in 1..=k {
        let (p, q) = (pts[i % k], pts[i - 1]);
        out.push((p.0, p.1, (p.0 - q.0).hypot(p.1 - q.1) * 0.62));
    }
    out
}
fn cloud_path(w: f64, h: f64) -> String {
    let pts = cloud_points(w, h);
    let mut d = format!("M {} {}", n(pts[0].0), n(pts[0].1));
    for &(x, y, r) in &pts[1..] {
        let _ = write!(d, " A {} {} 0 0 1 {} {}", n(r), n(r), n(x), n(y));
    }
    d + " Z"
}

fn shape_svg(t: &Item, is_root: bool) -> String {
    let st = &t.b.style;
    let (w, h) = (num(t.w), num(t.h));
    let (wf, hf) = (t.w, t.h);
    let fill = if st.fill.is_empty() { "transparent" } else { &st.fill };
    let stroke = if st.border.is_empty() { "none" } else { &st.border };
    let sw = st.border_width;
    let dash = match st.border_dash.as_str() {
        "dashed" => r#" stroke-dasharray="6 4""#,
        "dotted" => r#" stroke-dasharray="1.5 3" stroke-linecap="round""#,
        _ => "",
    };
    let common = format!(r#"fill="{fill}" stroke="{}" stroke-width="{}"{dash} class="shape""#, if sw != 0.0 { stroke } else { "none" }, num(sw));
    let ns = t.node.style.as_ref();
    match st.shape.as_str() {
        "rect" => format!(r#"<rect width="{w}" height="{h}" rx="2" {common}/>"#),
        "pill" => format!(r#"<rect width="{w}" height="{h}" rx="{}" {common}/>"#, n(hf / 2.0)),
        "ellipse" => format!(r#"<ellipse cx="{}" cy="{}" rx="{}" ry="{}" {common}/>"#, n(wf / 2.0), n(hf / 2.0), n(wf / 2.0), n(hf / 2.0)),
        "circle" => format!(r#"<circle cx="{}" cy="{}" r="{}" {common}/>"#, n(wf / 2.0), n(hf / 2.0), n(wf / 2.0)),
        "diamond" => format!(r#"<polygon points="{},0 {w},{} {},{h} 0,{}" {common}/>"#, n(wf / 2.0), n(hf / 2.0), n(wf / 2.0), n(hf / 2.0)),
        "hexagon" => {
            let k = hf * 0.35;
            format!(r#"<polygon points="{},0 {},0 {w},{} {},{h} {},{h} 0,{}" {common}/>"#, n(k), n(wf - k), n(hf / 2.0), n(wf - k), n(k), n(hf / 2.0))
        }
        "parallelogram" => {
            let k = hf * 0.3;
            format!(r#"<polygon points="{},0 {w},0 {},{h} 0,{h}" {common}/>"#, n(k), n(wf - k))
        }
        "cloud" => format!(r#"<path d="{}" {common}/>"#, cloud_path(wf, hf)),
        "underline" => {
            let bg = ns.and_then(|s| crate::js::some_str(&s.fill)).unwrap_or("transparent");
            let bw = if st.border_width != 0.0 { st.border_width } else { 1.2 };
            let lw = 1.5f64.max(bw + 0.8);
            let col = if ns.and_then(|s| crate::js::some_str(&s.border)).is_some() { &st.border } else { &st.line };
            format!(
                r#"<rect width="{w}" height="{h}" rx="4" fill="{bg}" class="shape"/><path d="M0 {} H{w}" stroke="{col}" stroke-width="{}" stroke-linecap="round"/>"#,
                n(hf),
                num(lw)
            )
        }
        "none" => {
            let bg = ns.and_then(|s| crate::js::some_str(&s.fill)).unwrap_or("transparent");
            format!(r#"<rect width="{w}" height="{h}" rx="4" fill="{bg}" class="shape"/>"#)
        }
        _ => format!(r#"<rect width="{w}" height="{h}" rx="{}" {common}/>"#, if is_root { "12".into() } else { n(8f64.min(hf / 3.0)) }),
    }
}

fn star(cx: f64, cy: f64, r: f64) -> String {
    (0..10)
        .map(|i| {
            let rr = if i % 2 == 1 { r * 0.45 } else { r };
            let a = -PI / 2.0 + (i as f64 * PI) / 5.0;
            format!("{},{}", n(cx + a.cos() * rr), n(cy + a.sin() * rr))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn lookup(table: &[(&str, &'static str)], k: &str) -> Option<&'static str> {
    table.iter().find(|(a, _)| *a == k).map(|(_, v)| *v)
}

/// A marker, task box, link or note icon at (x, y), size s.
pub fn icon_svg(icon: &Icon, x: f64, y: f64, s: f64, color: &str) -> String {
    let (cx, cy, r) = (x + s / 2.0, y + s / 2.0, s / 2.0);
    match icon {
        Icon::Task(v) if v == "done" => format!(
            r##"<g data-act="task" class="icon"><rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="#2f9e44"/><path d="M{} {} l{} {} l{} {}" stroke="#fff" stroke-width="{}" fill="none" stroke-linecap="round" stroke-linejoin="round"/></g>"##,
            n(x + 1.0),
            n(y + 1.0),
            n(s - 2.0),
            n(s - 2.0),
            n(x + s * 0.27),
            n(cy),
            n(s * 0.16),
            n(s * 0.17),
            n(s * 0.32),
            n(-s * 0.34),
            n(s * 0.12)
        ),
        Icon::Task(_) => format!(
            r##"<g data-act="task" class="icon"><rect x="{}" y="{}" width="{}" height="{}" rx="4" fill="#fff" stroke="{color}" stroke-width="1.6"/></g>"##,
            n(x + 1.5),
            n(y + 1.5),
            n(s - 3.0),
            n(s - 3.0)
        ),
        Icon::Link(v) => format!(
            r##"<g data-act="link" class="icon"><title>{}</title><rect x="{}" y="{}" width="{}" height="{}" fill="transparent"/><path d="M{} {} l{} {} M{} {} l{} {} a{} {} 0 0 1 {} {} l{} {} a{} {} 0 0 1 {} 0 M{} {} l{} {} a{} {} 0 0 1 {} {} l{} {} a{} {} 0 0 1 {} 0" stroke="#1c7ed6" stroke-width="{}" fill="none" stroke-linecap="round"/></g>"##,
            esc(v),
            num(x),
            num(y),
            num(s),
            num(s),
            n(x + s * 0.42),
            n(y + s * 0.58),
            n(s * 0.16),
            n(-s * 0.16),
            n(x + s * 0.5),
            n(y + s * 0.3),
            n(s * 0.08),
            n(-s * 0.08),
            n(s * 0.16),
            n(s * 0.16),
            n(s * 0.23),
            n(s * 0.23),
            n(-s * 0.16),
            n(s * 0.16),
            n(s * 0.16),
            n(s * 0.16),
            n(-s * 0.23),
            n(x + s * 0.5),
            n(y + s * 0.7),
            n(-s * 0.08),
            n(s * 0.08),
            n(s * 0.16),
            n(s * 0.16),
            n(-s * 0.23),
            n(-s * 0.23),
            n(s * 0.16),
            n(-s * 0.16),
            n(s * 0.16),
            n(s * 0.16),
            n(s * 0.23),
            n(s * 0.1)
        ),
        Icon::Note(v) => format!(
            r##"<g data-act="note" class="icon"><title>{}</title><rect x="{}" y="{}" width="{}" height="{}" rx="2" fill="#fff3bf" stroke="#f59f00" stroke-width="1.2"/><path d="M{} {}h{}M{} {}h{}M{} {}h{}" stroke="#e67700" stroke-width="1.2"/></g>"##,
            esc(slice16(v, 300)),
            n(x + s * 0.15),
            n(y + s * 0.08),
            n(s * 0.7),
            n(s * 0.84),
            n(x + s * 0.3),
            n(y + s * 0.35),
            n(s * 0.4),
            n(x + s * 0.3),
            n(y + s * 0.52),
            n(s * 0.4),
            n(x + s * 0.3),
            n(y + s * 0.69),
            n(s * 0.25)
        ),
        Icon::Marker(m) => marker_svg(m, x, y, s, cx, cy, r),
    }
}

fn marker_svg(m: &str, x: f64, y: f64, s: f64, cx: f64, cy: f64, r: f64) -> String {
    let b = m.as_bytes();
    if b.len() == 2 && b[0] == b'p' && b[1].is_ascii_digit() {
        let d = (b[1] - b'0') as i64;
        let c = if d >= 1 { themes::PRIORITY_COLORS[((d - 1) % 6) as usize] } else { "undefined" };
        return format!(
            r##"<g class="icon" data-act="marker" data-marker="{m}"><circle cx="{}" cy="{}" r="{}" fill="{c}"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}" font-weight="700" fill="#fff" font-family="-apple-system, 'Segoe UI', Arial, sans-serif">{d}</text></g>"##,
            n(cx),
            n(cy),
            n(r),
            n(cx),
            n(cy + 0.5),
            n(s * 0.62)
        );
    }
    if let Some(pct) = m.strip_prefix("pr").filter(|p| !p.is_empty() && p.bytes().all(|c| c.is_ascii_digit())) {
        let frac = pct.parse::<f64>().unwrap_or(0.0).min(100.0) / 100.0;
        let pie = if frac >= 1.0 {
            format!(
                r##"<circle cx="{}" cy="{}" r="{}" fill="#2f9e44"/><path d="M{} {} l{} {} l{} {}" stroke="#fff" stroke-width="{}" fill="none" stroke-linecap="round"/>"##,
                n(cx),
                n(cy),
                n(r - 1.0),
                n(x + s * 0.28),
                n(cy),
                n(s * 0.15),
                n(s * 0.16),
                n(s * 0.3),
                n(-s * 0.32),
                n(s * 0.11)
            )
        } else if frac > 0.0 {
            let a = frac * PI * 2.0 - PI / 2.0;
            let (ex, ey) = (cx + a.cos() * (r - 1.0), cy + a.sin() * (r - 1.0));
            format!(
                r##"<path d="M{} {} L{} {} A{} {} 0 {} 1 {} {} Z" fill="#1c7ed6"/>"##,
                n(cx),
                n(cy),
                n(cx),
                n(cy - r + 1.0),
                n(r - 1.0),
                n(r - 1.0),
                if frac > 0.5 { 1 } else { 0 },
                n(ex),
                n(ey)
            )
        } else {
            String::new()
        };
        return format!(
            r##"<g class="icon" data-act="marker" data-marker="{m}"><circle cx="{}" cy="{}" r="{}" fill="#fff" stroke="#1c7ed6" stroke-width="1.5"/>{pie}</g>"##,
            n(cx),
            n(cy),
            n(r - 1.0)
        );
    }
    if let Some(k) = m.strip_prefix("flag-") {
        let c = lookup(themes::FLAG_COLORS, k).unwrap_or("#e03131");
        return format!(
            r##"<g class="icon" data-act="marker" data-marker="{m}"><path d="M{} {} V{}" stroke="#555" stroke-width="1.4" stroke-linecap="round"/><path d="M{} {} H{} L{} {} L{} {} H{} Z" fill="{c}"/></g>"##,
            n(x + s * 0.22),
            n(y + s * 0.95),
            n(y + s * 0.08),
            n(x + s * 0.24),
            n(y + s * 0.1),
            n(x + s * 0.88),
            n(x + s * 0.72),
            n(y + s * 0.33),
            n(x + s * 0.88),
            n(y + s * 0.56),
            n(x + s * 0.24)
        );
    }
    if let Some(k) = m.strip_prefix("star-") {
        let c = lookup(themes::STAR_COLORS, k).unwrap_or("#f59f00");
        return format!(r#"<g class="icon" data-act="marker" data-marker="{m}"><polygon points="{}" fill="{c}"/></g>"#, star(cx, cy + 0.5, r));
    }
    format!(
        r#"<g class="icon" data-act="marker" data-marker="{}"><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}" font-family="'Apple Color Emoji','Segoe UI Emoji','Noto Color Emoji',sans-serif">{}</text></g>"#,
        esc(m),
        n(cx),
        n(cy + 1.0),
        n(s * 0.86),
        esc(m)
    )
}

fn count_all(node: &crate::model::Node) -> usize {
    node.children.iter().map(|c| 1 + count_all(c)).sum()
}

/// Where the collapse badge / expand handle sits, relative to the node.
pub fn toggle_pos(t: &Item) -> (f64, f64) {
    use crate::layout::Dir;
    let yb = if t.b.style.shape == "underline" { t.h } else { t.h / 2.0 };
    match t.dir {
        Dir::L => (-12.0, yb),
        Dir::D => (t.w / 2.0, t.h + 12.0),
        Dir::U => (t.w / 2.0, -12.0),
        Dir::Tree | Dir::TreeUp | Dir::FishRow | Dir::FishUp | Dir::FishDown => (-12.0, t.h / 2.0),
        Dir::Radial | Dir::Both => (t.w + 12.0, t.h / 2.0),
        _ => (t.w + 12.0, yb),
    }
}

pub struct SceneOpts<'a> {
    pub theme: &'a Theme,
    pub settings: &'a Settings,
    pub selected: &'a [String],
    pub matches: &'a [String],
    pub selected_rel: Option<&'a str>,
    pub export: bool,
}

fn node_svg(t: &Item, o: &SceneOpts, out: &mut String) {
    let b = &t.b;
    let st = &b.style;
    let is_root = t.depth == 0 && !t.floating;
    let _ = write!(
        out,
        r#"<g class="node{}" data-id="{}" transform="translate({} {})">"#,
        if t.floating && t.depth == 0 { " floating" } else { "" },
        t.id,
        n(t.x),
        n(t.y)
    );
    out.push_str(&shape_svg(t, is_root));
    if let (Some((iw, ih)), Some((ix, iy)), Some(im)) = (b.img, b.img_pos, t.node.image.as_ref()) {
        let _ = write!(
            out,
            r#"<image href="{}" x="{}" y="{}" width="{}" height="{}" preserveAspectRatio="xMidYMid meet"/>"#,
            esc(&im.src),
            n(ix),
            n(iy),
            n(iw),
            n(ih)
        );
    }
    let row = b.row;
    let icon_y = row.y + (row.h - b.icon_size) / 2.0;
    let mut ix = row.x;
    for ic in &b.lead {
        out.push_str(&icon_svg(ic, ix, icon_y, b.icon_size, &st.color));
        ix += b.icon_size + b.icon_gap;
    }
    let tx0 = row.x + row.text_x;
    let (anchor, tx) = match st.align.as_str() {
        "left" => ("start", tx0),
        "right" => ("end", tx0 + row.text_w),
        _ => ("middle", tx0 + row.text_w / 2.0),
    };
    let ty0 = row.y + (row.h - row.text_h) / 2.0;
    let deco = [if st.underline { "underline" } else { "" }, if st.strike { "line-through" } else { "" }]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ");
    let placeholder = t.node.text.is_empty();
    let _ = write!(
        out,
        r#"<text class="label" font-family="{}" font-size="{}" font-weight="{}"{}{} fill="{}"{} text-anchor="{anchor}">"#,
        esc(st.font),
        num(st.font_size),
        if st.bold { 700 } else { 400 },
        if st.italic { r#" font-style="italic""# } else { "" },
        if deco.is_empty() { String::new() } else { format!(r#" text-decoration="{deco}""#) },
        st.color,
        if placeholder { r#" opacity="0.35""# } else { "" }
    );
    let ph = [if o.export { String::new() } else { "Topic".to_string() }];
    let lines: &[String] = if placeholder { &ph } else { &b.lines };
    for (i, ln) in lines.iter().enumerate() {
        let e = esc(ln);
        let _ = write!(
            out,
            r#"<tspan x="{}" y="{}" dominant-baseline="central">{}</tspan>"#,
            n(tx),
            n(ty0 + i as f64 * b.lh + b.lh / 2.0),
            if e.is_empty() { " " } else { &e }
        );
    }
    out.push_str("</text>");
    ix = row.x + row.text_x + row.text_w + b.icon_gap;
    for ic in &b.trail {
        out.push_str(&icon_svg(ic, ix, icon_y, b.icon_size, &st.color));
        ix += b.icon_size + b.icon_gap;
    }
    let mut gx = b.tags_x;
    for tg in &b.tags {
        let _ = write!(
            out,
            r#"<g class="tag"><rect x="{}" y="{}" width="{}" height="18" rx="9" fill="{}" fill-opacity="0.18"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="11" font-weight="600" font-family="{}" fill="{}" fill-opacity="0.85">{}</text></g>"#,
            n(gx),
            n(b.tags_y),
            num(tg.w),
            st.line,
            n(gx + tg.w / 2.0),
            n(b.tags_y + 9.5),
            esc(st.font),
            st.color,
            esc(&tg.text)
        );
        gx += tg.w + 4.0;
    }
    if !t.node.children.is_empty() {
        let (px, py) = toggle_pos(t);
        let bg = o.theme.bg;
        if t.node.is_collapsed() {
            let cnt = count_all(t.node);
            let _ = write!(
                out,
                r#"<g class="badge" data-act="toggle"><circle cx="{}" cy="{}" r="10" fill="{bg}" stroke="{}" stroke-width="1.6"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="{}" font-weight="700" fill="{}" font-family="-apple-system,'Segoe UI',Arial,sans-serif">{cnt}</text></g>"#,
                n(px),
                n(py),
                st.line,
                n(px),
                n(py + 0.5),
                if cnt > 99 { 8 } else { 10 },
                st.line
            );
        } else if !o.export && !(is_root && o.settings.layout == "mindmap") {
            let _ = write!(
                out,
                r#"<g class="toggler" data-act="toggle"><circle cx="{}" cy="{}" r="7" fill="{bg}" stroke="{}" stroke-width="1.4"/><path d="M{} {} H{}" stroke="{}" stroke-width="1.6" stroke-linecap="round"/></g>"#,
                n(px),
                n(py),
                st.line,
                n(px - 3.5),
                n(py),
                n(px + 3.5),
                st.line
            );
        }
    }
    out.push_str("</g>");
}

/// Relationship arrowhead corners (tip, left, right).
pub fn arrow_head(rp: &layout::RelPath) -> [(f64, f64); 3] {
    let (ah, ang) = (9.0, rp.ang);
    let (ax, ay) = rp.p2;
    [(ax, ay), (ax - (ang - 0.45).cos() * ah, ay - (ang - 0.45).sin() * ah), (ax - (ang + 0.45).cos() * ah, ay - (ang + 0.45).sin() * ah)]
}

/// The whole map as SVG elements (no `<svg>` wrapper).
pub fn scene(l: &Layout, doc: &Doc, o: &SceneOpts) -> String {
    let theme = o.theme;
    let s = o.settings;
    let mut out = String::with_capacity(l.items.len() * 900);
    let font = esc(themes::get_font(&s.font));
    for b in &l.boundaries {
        let _ = write!(
            out,
            r#"<g class="boundary" data-boundary="{}"><rect x="{}" y="{}" width="{}" height="{}" rx="14" fill="{}" fill-opacity="0.07" stroke="{}" stroke-width="1.5" stroke-dasharray="6 5"/>"#,
            l.items[b.item].id,
            n(b.x),
            n(b.y),
            n(b.w),
            n(b.h),
            b.color,
            b.color
        );
        if !b.label.is_empty() {
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="20" rx="10" fill="{}"/><text x="{}" y="{}" dominant-baseline="central" font-size="12" font-weight="600" fill="{}" font-family="{font}">{}</text>"#,
                n(b.x + 10.0),
                n(b.y + 6.0),
                n((b.w - 20.0).min(len16(&b.label) as f64 * 7.5 + 20.0)),
                b.color,
                n(b.x + 20.0),
                n(b.y + 16.5),
                themes::contrast_text(&b.color),
                esc(&b.label)
            );
        }
        out.push_str("</g>");
    }
    for e in &l.extras {
        match *e {
            Extra::Spine { x1, x2, y } => {
                let _ = write!(
                    out,
                    r#"<path d="M{} {} H{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/>"#,
                    n(x1),
                    n(y),
                    n(x2),
                    theme.line,
                    num(s.line_width * 2.4)
                );
            }
            Extra::Axis { x1, x2, y } => {
                let _ = write!(
                    out,
                    r#"<path d="M{} {} H{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/><path d="M{} {} L{} {} L{} {} Z" fill="{}"/>"#,
                    n(x1),
                    n(y),
                    n(x2),
                    theme.line,
                    num(s.line_width * 2.0),
                    n(x2 - 2.0),
                    n(y - 7.0),
                    n(x2 + 10.0),
                    n(y),
                    n(x2 - 2.0),
                    n(y + 7.0),
                    theme.line
                );
            }
        }
    }
    out.push_str(r#"<g class="edges">"#);
    for e in &l.edges {
        if e.orient == Orient::AxisDot {
            let _ = write!(out, r#"<circle cx="{}" cy="{}" r="{}" fill="{}"/>"#, n(e.p1.0), n(e.p1.1), num(s.line_width + 2.5), e.color);
            continue;
        }
        let p = layout::edge_path(e, if e.orient == Orient::Bone { "straight" } else { &s.line_style });
        let w = if e.orient == Orient::Bone { e.width * 1.4 } else { e.width };
        if p.filled {
            let _ = write!(out, r#"<path d="{}" fill="{}"/>"#, p.d, e.color);
        } else {
            let _ =
                write!(out, r#"<path d="{}" fill="none" stroke="{}" stroke-width="{}" stroke-linecap="round" stroke-linejoin="round"/>"#, p.d, e.color, n(w));
        }
    }
    out.push_str("</g>");
    out.push_str(r#"<g class="nodes">"#);
    for t in &l.items {
        node_svg(t, o, &mut out);
    }
    out.push_str("</g>");
    out.push_str(r#"<g class="relations">"#);
    for r in &doc.relations {
        let (Some(a), Some(b)) = (l.get(&r.from), l.get(&r.to)) else { continue };
        let rp = layout::relation_path(a, b, r.bend);
        let col = crate::js::some_str(&r.color).unwrap_or(if theme.dark { "#f8f9fa" } else { "#5c677d" });
        let sel = o.selected_rel == Some(r.id.as_str());
        let [tip, p1, p2] = arrow_head(&rp);
        let c = if sel { "#3b82f6" } else { col };
        let _ = write!(
            out,
            r#"<g class="relation{}" data-rel="{}"><path d="{}" fill="none" stroke="transparent" stroke-width="12"/><path d="{}" fill="none" stroke="{c}" stroke-width="{}" stroke-dasharray="7 5"/><polygon points="{},{} {},{} {},{}" fill="{c}"/>"#,
            if sel { " selected" } else { "" },
            r.id,
            rp.d,
            rp.d,
            if sel { "2.4" } else { "1.7" },
            n(tip.0),
            n(tip.1),
            n(p1.0),
            n(p1.1),
            n(p2.0),
            n(p2.1)
        );
        if !r.label.is_empty() {
            let lw = len16(&r.label) as f64 * 7.0 + 16.0;
            let _ = write!(
                out,
                r#"<rect x="{}" y="{}" width="{}" height="22" rx="11" fill="{}" stroke="{col}" stroke-width="1"/><text x="{}" y="{}" text-anchor="middle" dominant-baseline="central" font-size="12" fill="{col}" font-family="{font}">{}</text>"#,
                n(rp.mid.0 - lw / 2.0),
                n(rp.mid.1 - 11.0),
                n(lw),
                theme.bg,
                n(rp.mid.0),
                n(rp.mid.1 + 0.5),
                esc(&r.label)
            );
        }
        out.push_str("</g>");
    }
    out.push_str("</g>");
    if !o.export {
        out.push_str(r#"<g class="overlay">"#);
        for id in o.matches {
            if let Some(t) = l.get(id) {
                let _ = write!(
                    out,
                    r##"<rect x="{}" y="{}" width="{}" height="{}" rx="10" fill="none" stroke="#fab005" stroke-width="3" stroke-dasharray="5 3"/>"##,
                    n(t.x - 5.0),
                    n(t.y - 5.0),
                    n(t.w + 10.0),
                    n(t.h + 10.0)
                );
            }
        }
        for id in o.selected {
            if let Some(t) = l.get(id) {
                let _ = write!(
                    out,
                    r##"<rect class="sel" x="{}" y="{}" width="{}" height="{}" rx="10" fill="none" stroke="#3b82f6" stroke-width="2.5"/>"##,
                    n(t.x - 4.5),
                    n(t.y - 4.5),
                    n(t.w + 9.0),
                    n(t.h + 9.0)
                );
            }
        }
        out.push_str("</g>");
    }
    out
}

pub struct Exported {
    pub svg: String,
    pub width: f64,
    pub height: f64,
}

/// A standalone SVG document of the whole map.
pub fn export_svg(l: &Layout, doc: &Doc, theme: &Theme, settings: &Settings, transparent: bool, padding: Option<f64>) -> Exported {
    let pad = padding.unwrap_or(40.0);
    let b = l.bounds;
    let (x, y) = ((b.min_x - pad).floor(), (b.min_y - pad).floor());
    let (w, h) = ((b.w + pad * 2.0).ceil(), (b.h + pad * 2.0).ceil());
    let (xs, ys, ws, hs) = (num(x), num(y), num(w), num(h));
    let tile = if transparent { None } else { pattern_tile(&settings.pattern, theme.ink) };
    let mut defs = String::new();
    let mut bg = if transparent { String::new() } else { format!(r#"<rect x="{xs}" y="{ys}" width="{ws}" height="{hs}" fill="{}"/>"#, theme.bg) };
    if let Some(t) = tile {
        defs = format!(r#"<defs><pattern id="ml-bg" patternUnits="userSpaceOnUse" width="{}" height="{}">{}</pattern></defs>"#, num(t.size), num(t.h), t.body);
        let _ = write!(bg, r#"<rect x="{xs}" y="{ys}" width="{ws}" height="{hs}" fill="url(#ml-bg)"/>"#);
    }
    let body = scene(l, doc, &SceneOpts { theme, settings, selected: &[], matches: &[], selected_rel: None, export: true });
    Exported {
        svg: format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{ws}" height="{hs}" viewBox="{xs} {ys} {ws} {hs}">{defs}{bg}{body}</svg>"#
        ),
        width: w,
        height: h,
    }
}
