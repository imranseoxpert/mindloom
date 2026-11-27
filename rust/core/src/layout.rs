//! Measuring, the nine layouts, hand-placed offsets and connector paths (port of `layout.js`).
//! Text measuring is injected, so the same code serves the app and the recorded tests.

use crate::js::{self, len16, num, r1, round, sign1, trim};
use crate::model::{self, Doc, Node, Settings};
use crate::themes::{self, Style, Theme};
use std::collections::HashMap;
use std::f64::consts::PI;

/// The font a piece of text is measured in.
#[derive(Clone, Debug, PartialEq)]
pub struct FontSpec {
    pub size: f64,
    pub weight: u16,
    pub italic: bool,
    /// CSS font stack, as in the theme table.
    pub family: &'static str,
}
pub type Measure<'m> = &'m dyn Fn(&str, &FontSpec) -> f64;

/// The fixed measure of the recordings (ADR-005): UTF-16 length × size × 0.55.
pub fn test_measure(text: &str, f: &FontSpec) -> f64 {
    len16(text) as f64 * f.size * 0.55
}

pub struct Ctx<'a> {
    pub theme: &'a Theme,
    pub settings: &'a Settings,
    pub measure: Measure<'a>,
    pub focus_id: Option<&'a str>,
    pub ignore_offset_id: Option<&'a str>,
    /// Today as `YYYY-MM-DD`, for due dates and "overdue".
    pub today: &'a str,
}

// ---------- text wrapping ----------

/// `para.split(/(\s+)/)`: words and the whitespace runs between them, in order.
fn split_keep_ws(s: &str) -> Vec<&str> {
    let mut out = vec![];
    let mut start = 0;
    let mut in_ws: Option<bool> = None;
    for (i, c) in s.char_indices() {
        let w = js::is_space(c);
        if let Some(prev) = in_ws {
            if prev != w {
                out.push(&s[start..i]);
                start = i;
            }
        } else if w {
            out.push("");
        }
        in_ws = Some(w);
    }
    out.push(&s[start..]);
    if in_ws == Some(true) {
        out.push("");
    }
    out
}

pub fn wrap_text(text: &str, font: &FontSpec, max_w: f64, m: Measure) -> Vec<String> {
    let mut out = vec![];
    for para in text.split('\n') {
        if para.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut line = String::new();
        for w in split_keep_ws(para) {
            let cand = format!("{line}{w}");
            if m(&cand, font) <= max_w || trim(&line).is_empty() {
                if trim(&line).is_empty() && m(&cand, font) > max_w && !trim(w).is_empty() {
                    let mut chunk = line.clone();
                    for ch in w.chars() {
                        let next = format!("{chunk}{ch}");
                        if m(&next, font) > max_w && !chunk.is_empty() {
                            out.push(chunk);
                            chunk = ch.to_string();
                        } else {
                            chunk = next;
                        }
                    }
                    line = chunk;
                } else {
                    line = cand;
                }
            } else {
                out.push(js::trim_end(&line).to_string());
                line = js::trim_start(w).to_string();
            }
        }
        out.push(js::trim_end(&line).to_string());
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

// ---------- node measuring ----------

#[derive(Clone, Debug, PartialEq)]
pub enum Icon {
    Task(String),
    Marker(String),
    Link(String),
    Note(String),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub text: String,
    pub w: f64,
    /// The due-date chip (first in the row), and whether it is overdue.
    pub due: bool,
    pub overdue: bool,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Row {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub text_x: f64,
    pub text_w: f64,
    pub text_h: f64,
}

/// A node's measured inner geometry.
#[derive(Clone, Debug)]
pub struct NodeBox {
    pub w: f64,
    pub h: f64,
    pub style: Style,
    pub font: FontSpec,
    pub lines: Vec<String>,
    pub lh: f64,
    pub fs: f64,
    pub icon_size: f64,
    pub icon_gap: f64,
    pub lead: Vec<Icon>,
    pub trail: Vec<Icon>,
    pub tags: Vec<Tag>,
    pub img: Option<(f64, f64)>,
    pub img_pos: Option<(f64, f64)>,
    pub row: Row,
    pub tags_x: f64,
    pub tags_y: f64,
}

pub fn font_of(st: &Style) -> FontSpec {
    FontSpec { size: st.font_size, weight: if st.bold { 700 } else { 400 }, italic: st.italic, family: st.font }
}

/// `num` is the automatic outline number ("1.2"), or empty.
pub fn measure_node(node: &Node, depth: usize, branch: i64, ctx: &Ctx, floating: bool, num: &str) -> NodeBox {
    let st = themes::resolve_style(node, depth, branch, ctx.theme, ctx.settings, floating);
    let fs = st.font_size;
    let font = font_of(&st);
    let lh = round(fs * 1.32);
    let style_w = node.style.as_ref().and_then(|s| s.width).filter(|w| *w != 0.0);
    let max_w = style_w.unwrap_or(if depth == 0 { ctx.settings.max_width + 80.0 } else { ctx.settings.max_width });
    let raw = if node.text.is_empty() { " " } else { node.text.as_str() };
    // automatic numbering prefixes the first line only, like an outline (1, 1.1, 1.1.1…)
    let numbered;
    let text = if !num.is_empty() && !node.text.is_empty() {
        numbered = format!("{num}  {raw}");
        numbered.as_str()
    } else {
        raw
    };
    let lines = wrap_text(text, &font, max_w, ctx.measure);
    let mut text_w = 0f64;
    for l in &lines {
        text_w = text_w.max((ctx.measure)(l, &font));
    }
    text_w = text_w.max(if node.text.is_empty() { fs * 3.0 } else { 0.0 });
    let text_h = lines.len() as f64 * lh;

    let icon_size = round(fs * 1.15);
    let mut lead = vec![];
    if let Some(t) = js::some_str(&node.task) {
        lead.push(Icon::Task(t.into()));
    }
    for m in node.icons.iter().flatten() {
        lead.push(Icon::Marker(m.clone()));
    }
    let mut trail = vec![];
    if let Some(l) = js::some_str(&node.link) {
        trail.push(Icon::Link(l.into()));
    }
    if let Some(n) = js::some_str(&node.note) {
        trail.push(Icon::Note(n.into()));
    }
    let icon_gap = 5.0;
    let lead_w = lead.len() as f64 * (icon_size + icon_gap);
    let trail_w = trail.len() as f64 * (icon_size + icon_gap);
    let row_w = lead_w + text_w + trail_w;
    let row_h = text_h.max(if !lead.is_empty() || !trail.is_empty() { icon_size } else { 0.0 });

    let tag_font = FontSpec { size: 11.0, weight: 600, italic: false, family: st.font };
    let mut tags: Vec<Tag> =
        node.tags.iter().flatten().map(|t| Tag { text: t.clone(), w: (ctx.measure)(t, &tag_font).ceil() + 14.0, due: false, overdue: false }).collect();
    // a due date rides in the same chip row as the labels, first in line, red when overdue
    if let Some(d) = node.due.as_deref().filter(|d| model::is_date_str(d)) {
        let label = format!("📅 {}", model::format_due(d, ctx.today));
        let w = (ctx.measure)(&label, &tag_font).ceil() + 14.0;
        tags.insert(0, Tag { text: label, w, due: true, overdue: model::is_overdue(node, ctx.today) });
    }
    let tags_w = tags.iter().map(|t| t.w + 4.0).sum::<f64>() - if tags.is_empty() { 0.0 } else { 4.0 };
    let tags_h = if tags.is_empty() { 0.0 } else { 18.0 };

    let img = node.image.as_ref().filter(|i| !i.src.is_empty()).map(|i| {
        let iw0 = if i.w != 0.0 { i.w } else { 160.0 };
        let ih0 = if i.h != 0.0 { i.h } else { 120.0 };
        let iw = iw0.min(max_w);
        (iw, (ih0 * iw) / iw0)
    });
    let content_w = row_w.max(tags_w).max(img.map_or(0.0, |i| i.0));
    let content_h = img.map_or(0.0, |i| i.1 + 8.0) + row_h + if tags.is_empty() { 0.0 } else { tags_h + 6.0 };

    let (mut pad_x, mut pad_y) = (round(fs * 0.85), round(fs * 0.5));
    if depth == 0 && !floating {
        pad_x = round(fs);
        pad_y = round(fs * 0.62);
    }
    if st.shape == "underline" {
        pad_x = 6.0;
        pad_y = 5.0;
    }
    if st.shape == "none" {
        pad_x = 4.0;
        pad_y = 3.0;
    }
    let (mut w, mut h) = (content_w + pad_x * 2.0, content_h + pad_y * 2.0);
    match st.shape.as_str() {
        "ellipse" => {
            w = w * 1.22 + 6.0;
            h = h * 1.32 + 4.0;
        }
        "circle" => {
            let d = w.max(h) * 1.08 + 8.0;
            w = d;
            h = d;
        }
        "diamond" => {
            w = w * 1.5 + 8.0;
            h = h * 1.7 + 6.0;
        }
        "hexagon" => w += h * 0.7,
        "parallelogram" => w += h * 0.6,
        "cloud" => {
            w = w * 1.25 + 14.0;
            h = h * 1.45 + 10.0;
        }
        "pill" => w += h * 0.35,
        _ => {}
    }
    w = w.max(if depth == 0 { 80.0 } else { 36.0 }).ceil();
    h = h.ceil();
    let cx = (w - content_w) / 2.0;
    let cy = (h - content_h) / 2.0;
    let row_y = cy + img.map_or(0.0, |i| i.1 + 8.0);
    NodeBox {
        w,
        h,
        style: st,
        font,
        lines,
        lh,
        fs,
        icon_size,
        icon_gap,
        lead,
        trail,
        tags,
        img,
        img_pos: img.map(|i| (cx + (content_w - i.0) / 2.0, cy)),
        row: Row { x: cx + (content_w - row_w) / 2.0, y: row_y, w: row_w, h: row_h, text_x: lead_w, text_w, text_h },
        tags_x: cx + (content_w - tags_w) / 2.0,
        tags_y: row_y + row_h + 6.0,
    }
}

// ---------- the visual tree ----------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    None,
    R,
    L,
    D,
    U,
    Both,
    Tree,
    TreeUp,
    Radial,
    FishUp,
    FishDown,
    FishRow,
}
impl Dir {
    pub fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Dir::None => return None,
            Dir::R => "r",
            Dir::L => "l",
            Dir::D => "d",
            Dir::U => "u",
            Dir::Both => "both",
            Dir::Tree => "tree",
            Dir::TreeUp => "tree-up",
            Dir::Radial => "radial",
            Dir::FishUp => "fish-up",
            Dir::FishDown => "fish-down",
            Dir::FishRow => "fish-row",
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bone {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

/// A laid-out node.
#[derive(Clone, Debug)]
pub struct Item<'a> {
    pub id: &'a str,
    pub node: &'a Node,
    pub depth: usize,
    pub branch: i64,
    pub b: NodeBox,
    pub w: f64,
    pub h: f64,
    pub x: f64,
    pub y: f64,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub floating: bool,
    pub dir: Dir,
    /// Mind-map side of a main topic: `Some(true)` = left.
    pub left: Option<bool>,
    pub moved: bool,
    sh: f64,
    sw: f64,
    th: f64,
    a: f64,
    span: f64,
    pub bone: Option<Bone>,
    pub bone_x: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orient {
    H,
    V,
    Tree,
    Free,
    Bone,
    AxisDot,
}
impl Orient {
    pub fn as_str(self) -> &'static str {
        match self {
            Orient::H => "h",
            Orient::V => "v",
            Orient::Tree => "tree",
            Orient::Free => "free",
            Orient::Bone => "bone",
            Orient::AxisDot => "axisdot",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
    pub color: String,
    pub width: f64,
    pub depth: usize,
    pub orient: Orient,
    pub p1: (f64, f64),
    pub p2: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Boundary {
    pub item: usize,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub label: String,
    pub color: String,
    pub depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Extra {
    Spine { x1: f64, x2: f64, y: f64 },
    Axis { x1: f64, x2: f64, y: f64 },
}

pub struct Layout<'a> {
    /// Every laid-out node, in drawing order (root tree first, then floating trees, depth first).
    pub items: Vec<Item<'a>>,
    pub by_id: HashMap<&'a str, usize>,
    pub roots: Vec<usize>,
    pub edges: Vec<Edge>,
    pub boundaries: Vec<Boundary>,
    pub extras: Vec<Extra>,
    pub bounds: Bounds,
}

impl<'a> Layout<'a> {
    pub fn get(&self, id: &str) -> Option<&Item<'a>> {
        self.by_id.get(id).map(|&i| &self.items[i])
    }
}

struct Builder<'a, 'c> {
    items: Vec<Item<'a>>,
    ctx: &'c Ctx<'c>,
}

impl<'a> Builder<'a, '_> {
    fn build(&mut self, node: &'a Node, depth: usize, branch: i64, floating: bool, parent: Option<usize>, num: &str) -> usize {
        let b = measure_node(node, depth, branch, self.ctx, floating, num);
        let idx = self.items.len();
        self.items.push(Item {
            id: &node.id,
            node,
            depth,
            branch,
            w: b.w,
            h: b.h,
            b,
            x: 0.0,
            y: 0.0,
            parent,
            children: vec![],
            floating,
            dir: Dir::None,
            left: None,
            moved: false,
            sh: 0.0,
            sw: 0.0,
            th: 0.0,
            a: 0.0,
            span: 0.0,
            bone: None,
            bone_x: None,
        });
        if !node.is_collapsed() {
            for (i, c) in node.children.iter().enumerate() {
                let cb = if depth == 0 {
                    if floating {
                        branch
                    } else {
                        i as i64
                    }
                } else {
                    branch
                };
                let cnum = if self.ctx.settings.numbering == "on" {
                    if num.is_empty() {
                        (i + 1).to_string()
                    } else {
                        format!("{num}.{}", i + 1)
                    }
                } else {
                    String::new()
                };
                let ci = self.build(c, depth + 1, cb, floating, Some(idx), &cnum);
                self.items[idx].children.push(ci);
            }
        }
        idx
    }

    fn kids(&self, t: usize) -> Vec<usize> {
        self.items[t].children.clone()
    }

    // horizontal logic tree
    fn size_h(&mut self, t: usize, s: &Settings) -> f64 {
        let gap = if self.items[t].depth == 0 { s.v_gap * 1.8 } else { s.v_gap };
        let kids = self.kids(t);
        if kids.is_empty() {
            self.items[t].sh = self.items[t].h;
            return self.items[t].sh;
        }
        let mut sum = 0.0;
        for c in &kids {
            sum += self.size_h(*c, s);
        }
        sum += gap * (kids.len() as f64 - 1.0);
        self.items[t].sh = self.items[t].h.max(sum);
        self.items[t].sh
    }
    fn place_h(&mut self, t: usize, x: f64, cy: f64, dir: f64, s: &Settings) {
        let (w, h, depth) = (self.items[t].w, self.items[t].h, self.items[t].depth);
        let it = &mut self.items[t];
        it.x = if dir > 0.0 { x } else { x - w };
        it.y = cy - h / 2.0;
        it.dir = if dir > 0.0 { Dir::R } else { Dir::L };
        let tx = it.x;
        let kids = self.kids(t);
        if kids.is_empty() {
            return;
        }
        let gap = if depth == 0 { s.v_gap * 1.8 } else { s.v_gap };
        let hg = if depth == 0 { s.h_gap * 1.25 } else { s.h_gap };
        let child_x = if dir > 0.0 { tx + w + hg } else { tx - hg };
        let mut total = -gap;
        for c in &kids {
            total += self.items[*c].sh + gap;
        }
        let mut y = cy - total / 2.0;
        for c in kids {
            let sh = self.items[c].sh;
            self.place_h(c, child_x, y + sh / 2.0, dir, s);
            y += sh + gap;
        }
    }

    // vertical org chart
    fn leaf_group(&self, t: usize) -> bool {
        let it = &self.items[t];
        it.depth >= 1 && !it.children.is_empty() && it.children.iter().all(|c| self.items[*c].children.is_empty())
    }
    fn size_v(&mut self, t: usize, s: &Settings) -> f64 {
        let gap = 14f64.max(s.v_gap + 8.0);
        let kids = self.kids(t);
        if kids.is_empty() {
            self.items[t].sw = self.items[t].w;
            return self.items[t].sw;
        }
        if self.leaf_group(t) {
            let mut m = self.items[t].w;
            for c in &kids {
                m = m.max(stack_indent(s) + self.items[*c].w);
            }
            self.items[t].sw = m;
            return m;
        }
        let mut sum = 0.0;
        for c in &kids {
            sum += self.size_v(*c, s);
        }
        sum += gap * (kids.len() as f64 - 1.0);
        self.items[t].sw = self.items[t].w.max(sum);
        self.items[t].sw
    }
    fn place_v(&mut self, t: usize, cx: f64, y: f64, dir: f64, s: &Settings) {
        let kids = self.kids(t);
        if self.leaf_group(t) {
            let left = cx - self.items[t].sw / 2.0;
            let g = stack_gap(s);
            let th = self.items[t].h;
            let it = &mut self.items[t];
            it.x = left;
            it.y = if dir > 0.0 { y } else { y - th };
            it.dir = if dir > 0.0 { Dir::Tree } else { Dir::TreeUp };
            let tdir = it.dir;
            let mut cy = if dir > 0.0 { it.y + th + g } else { it.y - g };
            for c in kids {
                let ch = self.items[c].h;
                let ci = &mut self.items[c];
                ci.x = left + stack_indent(s);
                ci.y = if dir > 0.0 { cy } else { cy - ch };
                ci.dir = tdir;
                cy = if dir > 0.0 { ci.y + ch + g } else { ci.y - g };
            }
            return;
        }
        let (w, h) = (self.items[t].w, self.items[t].h);
        let it = &mut self.items[t];
        it.x = cx - w / 2.0;
        it.y = if dir > 0.0 { y } else { y - h };
        it.dir = if dir > 0.0 { Dir::D } else { Dir::U };
        let ty = it.y;
        if kids.is_empty() {
            return;
        }
        let gap = 14f64.max(s.v_gap + 8.0);
        let lg = 36f64.max(s.h_gap * 0.85);
        let child_y = if dir > 0.0 { ty + h + lg } else { ty - lg };
        let mut total = -gap;
        for c in &kids {
            total += self.items[*c].sw + gap;
        }
        let mut x = cx - total / 2.0;
        for c in kids {
            let sw = self.items[c].sw;
            self.place_v(c, x + sw / 2.0, child_y, dir, s);
            x += sw + gap;
        }
    }

    // indented tree
    fn size_t(&mut self, t: usize, s: &Settings) -> f64 {
        let gap = 8f64.max(s.v_gap * 0.7);
        let mut hsum = self.items[t].h;
        for c in self.kids(t) {
            hsum += gap + self.size_t(c, s);
        }
        self.items[t].th = hsum;
        hsum
    }
    fn place_t(&mut self, t: usize, x: f64, y: f64, s: &Settings) {
        let it = &mut self.items[t];
        it.x = x;
        it.y = y;
        it.dir = Dir::Tree;
        let gap = 8f64.max(s.v_gap * 0.7);
        let indent = 26f64.max(48f64.min(s.h_gap * 0.65));
        let mut cy = y + it.h + gap;
        for c in self.kids(t) {
            self.place_t(c, x + indent, cy, s);
            cy += self.items[c].th + gap;
        }
    }
    fn width_t(&self, t: usize) -> f64 {
        let it = &self.items[t];
        let mut m = it.x + it.w;
        for c in &it.children {
            m = m.max(self.width_t(*c));
        }
        m
    }

    // radial
    fn leaves(&self, t: usize) -> f64 {
        let k = &self.items[t].children;
        if k.is_empty() {
            1.0
        } else {
            k.iter().map(|c| self.leaves(*c)).sum()
        }
    }
    fn assign(&mut self, t: usize, a0: f64, a1: f64, by_depth: &mut Vec<Vec<usize>>) {
        self.items[t].a = (a0 + a1) / 2.0;
        self.items[t].span = a1 - a0;
        let d = self.items[t].depth;
        while by_depth.len() <= d {
            by_depth.push(vec![]);
        }
        by_depth[d].push(t);
        let total = self.leaves(t);
        let mut a = a0;
        for c in self.kids(t) {
            let span = ((a1 - a0) * self.leaves(c)) / total;
            self.assign(c, a, a + span, by_depth);
            a += span;
        }
    }
    fn place_radial(&mut self, root: usize, s: &Settings) {
        let r = &mut self.items[root];
        r.x = -r.w / 2.0;
        r.y = -r.h / 2.0;
        r.dir = Dir::Radial;
        let mut by_depth: Vec<Vec<usize>> = vec![];
        self.assign(root, -PI / 2.0, (PI * 3.0) / 2.0, &mut by_depth);
        let half = |it: &Item| it.w.hypot(it.h) / 2.0;
        let maxh = |v: &Vec<usize>, items: &Vec<Item>| v.iter().map(|i| half(&items[*i])).fold(f64::NEG_INFINITY, f64::max);
        let mut radius = vec![0.0];
        let mut min_gap = vec![0.0];
        for d in 1..by_depth.len() {
            let prev = maxh(&by_depth[d - 1], &self.items);
            let cur = maxh(&by_depth[d], &self.items);
            min_gap.push(prev + cur + s.h_gap * 0.6);
            let mut r = radius[d - 1] + min_gap[d];
            for &t in &by_depth[d] {
                let it = &self.items[t];
                let along = (it.w * it.a.sin()).abs() + (it.h * it.a.cos()).abs();
                let need = ((along + s.v_gap) * 1.12) / 0.05f64.max(it.span.min(PI));
                r = r.max(need);
            }
            radius.push(r);
        }
        if by_depth.len() >= 3 {
            for d in (1..=by_depth.len() - 2).rev() {
                let even = radius[d + 1] * (d as f64 / (d as f64 + 1.0)) * 0.8;
                radius[d] = radius[d].max(even.min(radius[d + 1] - min_gap[d + 1]));
            }
        }
        for (d, ring) in by_depth.iter().enumerate().skip(1) {
            for &t in ring {
                let it = &mut self.items[t];
                let (cx, cy) = (it.a.cos() * radius[d], it.a.sin() * radius[d]);
                it.x = cx - it.w / 2.0;
                it.y = cy - it.h / 2.0;
                it.dir = Dir::Radial;
            }
        }
    }

    // timeline
    fn place_timeline(&mut self, root: usize, s: &Settings, extras: &mut Vec<Extra>) {
        let r = &mut self.items[root];
        r.x = 0.0;
        r.y = -r.h / 2.0;
        r.dir = Dir::R;
        let (rx, rw) = (r.x, r.w);
        let mut x = rx + rw + s.h_gap;
        let gap = 8f64.max(s.v_gap * 0.7);
        for (i, c) in self.kids(root).into_iter().enumerate() {
            let ci = &mut self.items[c];
            ci.x = x;
            ci.y = -ci.h / 2.0;
            ci.dir = if i % 2 == 0 { Dir::Tree } else { Dir::TreeUp };
            let (cy, ch) = (ci.y, ci.h);
            let mut col_w = ci.w;
            let gk = self.kids(c);
            if !gk.is_empty() {
                let mut total = 0.0;
                for g in &gk {
                    total += self.size_t(*g, s) + gap;
                }
                let mut y = if i % 2 == 0 { cy + ch + gap + 6.0 } else { cy - 6.0 - total };
                for g in gk {
                    self.place_t(g, x + 22.0, y, s);
                    y += self.items[g].th + gap;
                    col_w = col_w.max(self.width_t(g) - x);
                }
            }
            x += col_w + s.h_gap * 0.8;
        }
        extras.push(Extra::Axis { x1: rx + rw, x2: x - s.h_gap * 0.4, y: 0.0 });
    }

    // fishbone
    fn flatten_rows(&self, t: usize, depth: usize, out: &mut Vec<(usize, usize)>) {
        for &c in &self.items[t].children {
            out.push((c, depth));
            self.flatten_rows(c, depth + 1, out);
        }
    }
    fn place_fishbone(&mut self, root: usize, s: &Settings, extras: &mut Vec<Extra>) {
        let r = &mut self.items[root];
        r.x = 0.0;
        r.y = -r.h / 2.0;
        r.dir = Dir::L;
        let rx = r.x;
        let mut sx = rx - 40.0;
        let slope = 0.55;
        let kids = self.kids(root);
        let n = kids.len();
        let mut pair_w = 0f64;
        for (i, c) in kids.into_iter().enumerate() {
            let above = i % 2 == 0;
            let mut rows = vec![];
            self.flatten_rows(c, 0, &mut rows);
            let row_gap = 6f64.max(s.v_gap * 0.6);
            let (mut rows_h, mut max_row_w) = (0.0, 0f64);
            for &(t, d) in &rows {
                rows_h += self.items[t].h + row_gap;
                max_row_w = max_row_w.max(self.items[t].w + d as f64 * 22.0);
            }
            let l = 70f64.max(rows_h + 24.0);
            let top_x = sx - slope * l;
            let ci = &mut self.items[c];
            ci.dir = if above { Dir::FishUp } else { Dir::FishDown };
            ci.x = top_x - ci.w / 2.0;
            ci.y = if above { -l - ci.h } else { l };
            ci.bone = Some(Bone { x1: sx, y1: 0.0, x2: top_x, y2: if above { -l } else { l } });
            let cw = ci.w;
            let mut y = if above { -l + 12.0 } else { 12.0 };
            for (t, d) in rows {
                let it = &mut self.items[t];
                let cy = y + it.h / 2.0;
                let bone_x = sx - slope * cy.abs();
                it.x = bone_x - 10.0 - d as f64 * 22.0 - it.w;
                it.y = y;
                it.dir = Dir::FishRow;
                it.bone_x = Some(bone_x);
                y += it.h + row_gap;
            }
            let slot_w = (cw / 2.0 + slope * l).max(slope * l + max_row_w + 20.0) + 30.0;
            pair_w = pair_w.max(slot_w);
            if !above || i == n - 1 {
                sx -= pair_w;
                pair_w = 0.0;
            }
        }
        extras.push(Extra::Spine { x1: rx, x2: (sx + 20.0).min(rx - 60.0), y: 0.0 });
    }

    fn apply_offsets(&mut self, t: usize, key: &str, mut ax: f64, mut ay: f64, ignore: Option<&str>, min_depth: usize) {
        let it = &mut self.items[t];
        if let Some(o) = it.node.offset.as_ref() {
            if o.l == key && it.depth >= min_depth && Some(it.id) != ignore {
                ax += o.x;
                ay += o.y;
                it.moved = true;
            }
        }
        if ax != 0.0 || ay != 0.0 {
            it.x += ax;
            it.y += ay;
        }
        for c in self.kids(t) {
            self.apply_offsets(c, key, ax, ay, ignore, min_depth);
        }
    }
}

fn stack_indent(s: &Settings) -> f64 {
    22f64.max(40f64.min(s.h_gap * 0.5))
}
fn stack_gap(s: &Settings) -> f64 {
    6f64.max(s.v_gap * 0.6)
}

/// Smallest depth that can be hand-placed in each structure (`None`: not at all).
pub fn free_min_depth(layout: &str) -> Option<usize> {
    match layout {
        "mindmap" | "right" | "left" | "down" | "up" | "tree" | "radial" => Some(1),
        "timeline" => Some(2),
        _ => None,
    }
}
pub fn can_free_position(layout: &str, depth: Option<usize>) -> bool {
    match (free_min_depth(layout), depth) {
        (Some(_), None) => true,
        (Some(m), Some(d)) => d >= m,
        _ => false,
    }
}

pub fn subtree_box(items: &[Item], t: usize) -> (f64, f64, f64, f64) {
    let it = &items[t];
    let (mut a, mut b, mut c, mut d) = (it.x, it.y, it.x + it.w, it.y + it.h);
    fn rec(items: &[Item], t: usize, a: &mut f64, b: &mut f64, c: &mut f64, d: &mut f64) {
        for &k in &items[t].children {
            let i = &items[k];
            *a = a.min(i.x);
            *b = b.min(i.y);
            *c = c.max(i.x + i.w);
            *d = d.max(i.y + i.h);
            rec(items, k, a, b, c, d);
        }
    }
    rec(items, t, &mut a, &mut b, &mut c, &mut d);
    (a, b, c, d)
}

/// Lays out the whole document (or the focused branch).
pub fn layout<'a>(doc: &'a Doc, ctx: &Ctx) -> Layout<'a> {
    let s = ctx.settings;
    let main_root = ctx.focus_id.and_then(|id| model::get(doc, id)).unwrap_or(&doc.root);
    let mut extras = vec![];
    let mut b = Builder { items: vec![], ctx };
    let root = b.build(main_root, 0, -1, false, None, "");
    let lay = s.layout.as_str();
    match lay {
        "mindmap" => {
            let r = &mut b.items[root];
            r.x = -r.w / 2.0;
            r.y = -r.h / 2.0;
            r.dir = Dir::Both;
            let kids = b.kids(root);
            for c in &kids {
                b.size_h(*c, s);
            }
            let side_of = |b: &Builder, c: usize| -> Option<&'static str> {
                b.items[c].node.side.as_deref().and_then(|v| ["left", "right"].into_iter().find(|s| *s == v))
            };
            let unsided: Vec<usize> = kids.iter().copied().filter(|c| side_of(&b, *c).is_none()).collect();
            let mut assigned: HashMap<usize, bool> = HashMap::new();
            if unsided.len() == kids.len() {
                let total: f64 = kids.iter().map(|c| b.items[*c].sh).sum();
                let mut acc = 0.0;
                for (i, c) in kids.iter().enumerate() {
                    let right = kids.len() == 1 || i == 0 || acc < total / 2.0;
                    assigned.insert(*c, !right);
                    if right {
                        acc += b.items[*c].sh;
                    }
                }
            } else {
                let (mut rh, mut lh) = (0.0, 0.0);
                for c in &kids {
                    match side_of(&b, *c) {
                        Some("left") => lh += b.items[*c].sh,
                        Some(_) => rh += b.items[*c].sh,
                        None => {}
                    }
                }
                for c in &unsided {
                    if rh <= lh {
                        assigned.insert(*c, false);
                        rh += b.items[*c].sh;
                    } else {
                        assigned.insert(*c, true);
                        lh += b.items[*c].sh;
                    }
                }
            }
            let (mut right, mut left) = (vec![], vec![]);
            for c in &kids {
                let is_left = match b.items[*c].node.side.as_deref().filter(|v| !v.is_empty()) {
                    Some(v) => v == "left",
                    None => assigned.get(c).copied().unwrap_or(false),
                };
                if is_left {
                    left.push(*c);
                } else {
                    right.push(*c);
                }
            }
            let gap = s.v_gap * 1.8;
            let hg = s.h_gap * 1.25;
            let (rx, rw) = (b.items[root].x, b.items[root].w);
            let stack = |b: &mut Builder, arr: Vec<usize>, dir: f64| {
                let mut tot = -gap;
                for c in &arr {
                    tot += b.items[*c].sh + gap;
                }
                let mut y = -tot / 2.0;
                for c in arr {
                    let sh = b.items[c].sh;
                    b.place_h(c, if dir > 0.0 { rx + rw + hg } else { rx - hg }, y + sh / 2.0, dir, s);
                    b.items[c].left = Some(dir < 0.0);
                    y += sh + gap;
                }
            };
            stack(&mut b, right, 1.0);
            left.reverse();
            stack(&mut b, left, -1.0);
        }
        "right" | "left" => {
            b.size_h(root, s);
            b.place_h(root, 0.0, 0.0, if lay == "right" { 1.0 } else { -1.0 }, s);
        }
        "down" | "up" => {
            b.size_v(root, s);
            b.place_v(root, 0.0, 0.0, if lay == "down" { 1.0 } else { -1.0 }, s);
        }
        "tree" => {
            b.size_t(root, s);
            b.place_t(root, 0.0, 0.0, s);
        }
        "radial" => b.place_radial(root, s),
        "timeline" => b.place_timeline(root, s, &mut extras),
        "fishbone" => b.place_fishbone(root, s, &mut extras),
        _ => {
            b.size_h(root, s);
            b.place_h(root, 0.0, 0.0, 1.0, s);
        }
    }
    if let Some(md) = free_min_depth(lay) {
        b.apply_offsets(root, lay, 0.0, 0.0, ctx.ignore_offset_id, md);
    }
    let mut roots = vec![root];
    if ctx.focus_id.is_none() {
        let offset = if doc.root.children.is_empty() { 1 } else { doc.root.children.len() } as i64;
        for (i, f) in doc.floating.iter().enumerate() {
            let ft = b.build(f, 0, offset + i as i64, true, None, "");
            b.size_h(ft, s);
            let h = b.items[ft].h;
            b.place_h(ft, f.x.unwrap_or(0.0), f.y.unwrap_or(0.0) + h / 2.0, 1.0, s);
            b.apply_offsets(ft, "float", 0.0, 0.0, ctx.ignore_offset_id, 1);
            roots.push(ft);
        }
    }
    let items = b.items;

    // drawing order = depth-first over each tree; edges parent → child
    let mut order = vec![];
    let mut edges = vec![];
    fn visit(items: &[Item], t: usize, s: &Settings, order: &mut Vec<usize>, edges: &mut Vec<Edge>) {
        order.push(t);
        for &c in &items[t].children {
            edges.push(make_edge(items, t, c, s));
            visit(items, c, s, order, edges);
        }
    }
    for &r in &roots {
        visit(&items, r, s, &mut order, &mut edges);
    }
    // re-index so `items` is in drawing order
    let mut remap = vec![0usize; items.len()];
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new;
    }
    let mut slots: Vec<Option<Item>> = items.into_iter().map(Some).collect();
    let mut items: Vec<Item> = order.iter().filter_map(|&old| slots[old].take()).collect();
    for it in &mut items {
        it.parent = it.parent.map(|p| remap[p]);
        for c in &mut it.children {
            *c = remap[*c];
        }
    }
    for e in &mut edges {
        e.from = remap[e.from];
        e.to = remap[e.to];
    }
    let roots: Vec<usize> = roots.iter().map(|r| remap[*r]).collect();
    let by_id: HashMap<&str, usize> = items.iter().enumerate().map(|(i, t)| (t.id, i)).collect();

    let mut boundaries = vec![];
    for (i, t) in items.iter().enumerate() {
        if !t.node.has_boundary() {
            continue;
        }
        let (a, bb, c, d) = subtree_box(&items, i);
        let pad = 12.0;
        let label = t.node.boundary_label.clone().unwrap_or_default();
        let lp = if label.is_empty() { 0.0 } else { 22.0 };
        boundaries.push(Boundary {
            item: i,
            x: a - pad,
            y: bb - pad - lp,
            w: c - a + pad * 2.0,
            h: d - bb + pad * 2.0 + lp,
            label,
            color: t.b.style.line.clone(),
            depth: t.depth,
        });
    }

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |x: f64, y: f64, w: f64, h: f64| {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x + w);
        max_y = max_y.max(y + h);
    };
    for t in &items {
        grow(t.x, t.y, t.w + if t.node.is_collapsed() && !t.node.children.is_empty() { 26.0 } else { 0.0 }, t.h + 4.0);
    }
    for bd in &boundaries {
        grow(bd.x, bd.y, bd.w, bd.h);
    }
    for e in &extras {
        let (Extra::Spine { x1, x2, y } | Extra::Axis { x1, x2, y }) = *e;
        grow(x1.min(x2), y - 2.0, (x2 - x1).abs(), 4.0);
    }
    if items.is_empty() {
        (min_x, min_y, max_x, max_y) = (0.0, 0.0, 1.0, 1.0);
    }
    Layout { items, by_id, roots, edges, boundaries, extras, bounds: Bounds { min_x, min_y, max_x, max_y, w: max_x - min_x, h: max_y - min_y } }
}

// ---------- connectors ----------

fn anchor_bottom(t: &Item) -> bool {
    t.b.style.shape == "underline"
}

pub fn clip_to_box(t: &Item, px: f64, py: f64) -> (f64, f64) {
    let (cx, cy) = (t.x + t.w / 2.0, t.y + t.h / 2.0);
    let (dx, dy) = (px - cx, py - cy);
    if dx == 0.0 && dy == 0.0 {
        return (cx, cy);
    }
    let nz = |v: f64| if v == 0.0 || v.is_nan() { 1e-9 } else { v };
    let sx = (t.w / 2.0) / nz(dx).abs();
    let sy = (t.h / 2.0) / nz(dy).abs();
    let k = sx.min(sy).min(1.0);
    (cx + dx * k, cy + dy * k)
}

fn make_edge(items: &[Item], pi: usize, ci: usize, s: &Settings) -> Edge {
    let (p, c) = (&items[pi], &items[ci]);
    let st = &c.b.style;
    let width = s.line_width * if c.depth == 1 { 1.4 } else { 1.0 };
    let mut e = Edge { from: pi, to: ci, color: st.line.clone(), width, depth: c.depth, orient: Orient::H, p1: (0.0, 0.0), p2: (0.0, 0.0) };
    let (pcy, ccy) = (p.y + p.h / 2.0, c.y + c.h / 2.0);
    let dir = if p.dir == Dir::Both {
        if c.left == Some(true) {
            Dir::L
        } else {
            Dir::R
        }
    } else {
        p.dir
    };
    if c.moved {
        let stem = p.x + 18f64.min(p.w / 3.0);
        let ok = match dir {
            Dir::R => c.x > p.x + p.w + 6.0,
            Dir::L => c.x + c.w < p.x - 6.0,
            Dir::D => c.y > p.y + p.h + 6.0,
            Dir::U => c.y + c.h < p.y - 6.0,
            Dir::Tree => c.x > stem + 4.0 && ccy > p.y + p.h + 2.0,
            Dir::TreeUp => c.x > stem + 4.0 && ccy < p.y - 2.0,
            _ => true,
        };
        if !ok {
            e.orient = Orient::Free;
            e.p1 = clip_to_box(p, c.x + c.w / 2.0, ccy);
            e.p2 = clip_to_box(c, p.x + p.w / 2.0, pcy);
            return e;
        }
    }
    match dir {
        Dir::R | Dir::L => {
            let right = dir == Dir::R;
            e.p1 = (if right { p.x + p.w } else { p.x }, if anchor_bottom(p) && p.depth > 0 { p.y + p.h } else { pcy });
            e.p2 = (if right { c.x } else { c.x + c.w }, if anchor_bottom(c) { c.y + c.h } else { ccy });
            if p.depth == 0 && p.dir == Dir::Both {
                e.p1.0 = if right { p.x + p.w - 10f64.min(p.w / 4.0) } else { p.x + 10f64.min(p.w / 4.0) };
            }
        }
        Dir::D | Dir::U => {
            e.orient = Orient::V;
            e.p1 = (p.x + p.w / 2.0, if dir == Dir::D { p.y + p.h } else { p.y });
            e.p2 = (c.x + c.w / 2.0, if dir == Dir::D { c.y } else { c.y + c.h });
        }
        Dir::Tree | Dir::TreeUp => {
            e.orient = Orient::Tree;
            e.p1 = (p.x + 18f64.min(p.w / 3.0), if dir == Dir::Tree { p.y + p.h } else { p.y });
            e.p2 = (c.x, if anchor_bottom(c) { c.y + c.h } else { ccy });
        }
        Dir::FishUp | Dir::FishDown => {
            e.orient = Orient::H;
            let y = if anchor_bottom(c) { c.y + c.h } else { ccy };
            e.p1 = (c.bone_x.unwrap_or(f64::NAN), y);
            e.p2 = (c.x + c.w, y);
        }
        Dir::FishRow => {
            e.orient = Orient::Tree;
            e.p1 = (p.x + p.w - 12.0, p.y + p.h);
            e.p2 = (c.x + c.w, if anchor_bottom(c) { c.y + c.h } else { ccy });
        }
        _ => {
            e.orient = Orient::Free;
            e.p1 = clip_to_box(p, c.x + c.w / 2.0, ccy);
            e.p2 = clip_to_box(c, p.x + p.w / 2.0, pcy);
        }
    }
    if let Some(bn) = c.bone {
        e.orient = Orient::Bone;
        e.p1 = (bn.x1, bn.y1);
        e.p2 = (bn.x2, bn.y2);
    }
    if p.depth == 0 && s.layout == "timeline" && !p.floating {
        e.orient = Orient::AxisDot;
        e.p1 = (c.x - 6.0, 0.0);
        e.p2 = (c.x, 0.0);
    }
    e
}

fn f(v: f64) -> String {
    num(r1(v))
}

/// SVG path of a connector; `filled` for the tapered style.
pub struct PathD {
    pub d: String,
    pub filled: bool,
}

pub fn edge_path(e: &Edge, style: &str) -> PathD {
    let ((x1, y1), (x2, y2)) = (e.p1, e.p2);
    let (dx, dy) = (x2 - x1, y2 - y1);
    let line = || PathD { d: format!("M {} {} L {} {}", f(x1), f(y1), f(x2), f(y2)), filled: false };
    if matches!(e.orient, Orient::Bone | Orient::AxisDot) {
        return line();
    }
    let mut orient = e.orient;
    if orient == Orient::Free {
        if matches!(style, "straight" | "elbow" | "rounded") {
            return line();
        }
        orient = if dx.abs() >= dy.abs() { Orient::H } else { Orient::V };
    }
    if orient == Orient::Tree {
        return match style {
            "straight" => line(),
            "curve" | "tapered" => PathD { d: format!("M {} {} Q {} {} {} {}", f(x1), f(y1), f(x1), f(y2), f(x2), f(y2)), filled: false },
            "elbow" => PathD { d: format!("M {} {} V {} H {}", f(x1), f(y1), f(y2), f(x2)), filled: false },
            _ => {
                let r = 10f64.min(dy.abs()).min(dx.abs());
                let (sy, sx) = (sign1(dy), sign1(dx));
                PathD { d: format!("M {} {} V {} Q {} {} {} {} H {}", f(x1), f(y1), f(y2 - sy * r), f(x1), f(y2), f(x1 + sx * r), f(y2), f(x2)), filled: false }
            }
        };
    }
    let hz = orient == Orient::H;
    let p = |a: f64, b: f64| if hz { format!("{} {}", f(a), f(b)) } else { format!("{} {}", f(b), f(a)) };
    let (a1, b1, a2, b2) = if hz { (x1, y1, x2, y2) } else { (y1, x1, y2, x2) };
    let da = a2 - a1;
    match style {
        "straight" => PathD { d: format!("M {} L {}", p(a1, b1), p(a2, b2)), filled: false },
        "elbow" => {
            let am = a1 + da / 2.0;
            PathD { d: format!("M {} L {} L {} L {}", p(a1, b1), p(am, b1), p(am, b2), p(a2, b2)), filled: false }
        }
        "rounded" => {
            let am = a1 + da / 2.0;
            let r = 10f64.min((b2 - b1).abs() / 2.0).min(da.abs() / 2.0);
            if r < 0.5 {
                return PathD { d: format!("M {} L {}", p(a1, b1), p(a2, b2)), filled: false };
            }
            let (sa, sb) = (sign1(da), sign1(b2 - b1));
            PathD {
                d: format!(
                    "M {} L {} Q {} {} L {} Q {} {} L {}",
                    p(a1, b1),
                    p(am - sa * r, b1),
                    p(am, b1),
                    p(am, b1 + sb * r),
                    p(am, b2 - sb * r),
                    p(am, b2),
                    p(am + sa * r, b2),
                    p(a2, b2)
                ),
                filled: false,
            }
        }
        "tapered" => {
            let w1 = 1.5f64.max(e.width * if e.depth == 1 { 2.6 } else { 1.7 });
            let w2 = 0.6f64.max(e.width * 0.35);
            let (c1, c2) = (a1 + da * 0.5, a2 - da * 0.5);
            PathD {
                d: format!(
                    "M {} C {} {} {} L {} C {} {} {} Z",
                    p(a1, b1 - w1 / 2.0),
                    p(c1, b1 - w1 / 2.0),
                    p(c2, b2 - w2 / 2.0),
                    p(a2, b2 - w2 / 2.0),
                    p(a2, b2 + w2 / 2.0),
                    p(c2, b2 + w2 / 2.0),
                    p(c1, b1 + w1 / 2.0),
                    p(a1, b1 + w1 / 2.0)
                ),
                filled: true,
            }
        }
        _ => {
            let (c1, c2) = (a1 + da * 0.5, a2 - da * 0.5);
            PathD { d: format!("M {} C {} {} {}", p(a1, b1), p(c1, b1), p(c2, b2), p(a2, b2)), filled: false }
        }
    }
}

/// A relationship curve between two laid-out nodes.
pub struct RelPath {
    pub d: String,
    pub p1: (f64, f64),
    pub c1: (f64, f64),
    pub c2: (f64, f64),
    pub p2: (f64, f64),
    pub mid: (f64, f64),
    pub ang: f64,
}
pub fn relation_path(a: &Item, b: &Item, bend: Option<f64>) -> RelPath {
    let ac = (a.x + a.w / 2.0, a.y + a.h / 2.0);
    let bc = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    let (dx, dy) = (bc.0 - ac.0, bc.1 - ac.1);
    let len = {
        let l = dx.hypot(dy);
        if l == 0.0 {
            1.0
        } else {
            l
        }
    };
    let (nx, ny) = (-dy / len, dx / len);
    let bend = bend.unwrap_or(120f64.min(len * 0.35));
    let c1 = (ac.0 + dx * 0.25 + nx * bend, ac.1 + dy * 0.25 + ny * bend);
    let c2 = (ac.0 + dx * 0.75 + nx * bend, ac.1 + dy * 0.75 + ny * bend);
    let p1 = clip_to_box(a, c1.0, c1.1);
    let p2 = clip_to_box(b, c2.0, c2.1);
    let mid = (0.125 * p1.0 + 0.375 * c1.0 + 0.375 * c2.0 + 0.125 * p2.0, 0.125 * p1.1 + 0.375 * c1.1 + 0.375 * c2.1 + 0.125 * p2.1);
    let ang = (p2.1 - c2.1).atan2(p2.0 - c2.0);
    RelPath { d: format!("M {} {} C {} {} {} {} {} {}", f(p1.0), f(p1.1), f(c1.0), f(c1.1), f(c2.0), f(c2.1), f(p2.0), f(p2.1)), p1, c1, c2, p2, mid, ang }
}
