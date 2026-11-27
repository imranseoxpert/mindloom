//! The application state and every command (port of `app.js`): selection, inline editing,
//! structure commands, connected dragging, search, clipboard, undo, presentation and autosave.

use crate::canvas::{self, View};
use crate::files::{self, Prefs};
use crate::fonts::{MapFonts, MeasureCache};
use egui::{Pos2, Rect};
use mindloom_core::layout::{self, Ctx, Dir, FontSpec, Layout, Row};
use mindloom_core::model::{self, Doc, History, Node, NodeStyle, Offset};
use mindloom_core::themes::{self, Style};
use mindloom_core::{io, templates};
use resvg::usvg::fontdb::Database;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

// ---------- geometry kept from the last frame (for hit testing) ----------

#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Toggle,
    Task,
    Link(String),
    Note,
    Marker(String),
}

#[derive(Clone, Debug)]
pub struct GItem {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub depth: usize,
    pub dir: Dir,
    pub left: Option<bool>,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    pub row: Row,
    pub style: Style,
    pub font: FontSpec,
    /// Clickable icons: (x, y, w, h, action), world coordinates.
    pub hits: Vec<(f64, f64, f64, f64, Act)>,
}

#[derive(Clone, Debug, Default)]
pub struct Geo {
    pub items: Vec<GItem>,
    pub by_id: HashMap<String, usize>,
    pub bounds: (f64, f64, f64, f64),
    /// Relationship curves: id, flattened points, label middle.
    pub rels: Vec<(String, Vec<(f64, f64)>)>,
}

impl Geo {
    pub fn from_layout(l: &Layout, doc: &Doc) -> Geo {
        let items = l
            .items
            .iter()
            .map(|t| {
                let mut hits = vec![];
                for (ic, x, y, s) in canvas::icon_slots(t) {
                    let act = match ic {
                        layout::Icon::Task(_) => Act::Task,
                        layout::Icon::Link(u) => Act::Link(u.clone()),
                        layout::Icon::Note(_) => Act::Note,
                        layout::Icon::Marker(m) => Act::Marker(m.clone()),
                    };
                    hits.push((x, y, s, s, act));
                }
                if !t.node.children.is_empty() {
                    let (px, py) = mindloom_core::render::toggle_pos(t);
                    hits.push((t.x + px - 10.0, t.y + py - 10.0, 20.0, 20.0, Act::Toggle));
                }
                GItem {
                    id: t.id.to_string(),
                    x: t.x,
                    y: t.y,
                    w: t.w,
                    h: t.h,
                    depth: t.depth,
                    dir: t.dir,
                    left: t.left,
                    parent: t.parent,
                    children: t.children.clone(),
                    row: t.b.row,
                    style: t.b.style.clone(),
                    font: t.b.font.clone(),
                    hits,
                }
            })
            .collect::<Vec<_>>();
        let by_id = items.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
        let rels = doc
            .relations
            .iter()
            .filter_map(|r| {
                let (a, b) = (l.get(&r.from)?, l.get(&r.to)?);
                let rp = layout::relation_path(a, b, r.bend);
                let pts = canvas::flatten(&rp.d, |_| 24).into_iter().flat_map(|(p, _)| p).collect();
                Some((r.id.clone(), pts))
            })
            .collect();
        let b = l.bounds;
        Geo { items, by_id, bounds: (b.min_x, b.min_y, b.w, b.h), rels }
    }
    pub fn get(&self, id: &str) -> Option<&GItem> {
        self.by_id.get(id).map(|i| &self.items[*i])
    }
    /// The box around a topic and everything below it.
    pub fn subtree_box(&self, i: usize) -> (f64, f64, f64, f64) {
        let t = &self.items[i];
        let mut b = (t.x, t.y, t.x + t.w, t.y + t.h);
        for &c in &t.children {
            let s = self.subtree_box(c);
            b = (b.0.min(s.0), b.1.min(s.1), b.2.max(s.2), b.3.max(s.3));
        }
        b
    }
    /// The topic under a world point (the smallest one, as topics can overlap when hand-placed).
    pub fn node_at(&self, x: f64, y: f64, pad: f64) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (i, t) in self.items.iter().enumerate() {
            if x < t.x - pad || x > t.x + t.w + pad || y < t.y - pad || y > t.y + t.h + pad {
                continue;
            }
            if best.is_some_and(|b| self.items[b].w * self.items[b].h < t.w * t.h) {
                continue;
            }
            best = Some(i);
        }
        best
    }
    pub fn act_at(&self, x: f64, y: f64) -> Option<(usize, Act)> {
        for (i, t) in self.items.iter().enumerate().rev() {
            for (hx, hy, hw, hh, a) in &t.hits {
                if x >= *hx && x <= hx + hw && y >= *hy && y <= hy + hh {
                    return Some((i, a.clone()));
                }
            }
        }
        None
    }
    pub fn rel_at(&self, x: f64, y: f64, tol: f64) -> Option<String> {
        for (id, pts) in &self.rels {
            for w in pts.windows(2) {
                if seg_dist((x, y), w[0], w[1]) <= tol {
                    return Some(id.clone());
                }
            }
        }
        None
    }
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    (p.0 - a.0 - t * dx).hypot(p.1 - a.1 - t * dy)
}

// ---------- interaction state ----------

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cam {
    pub zoom: f32,
    pub x: f32,
    pub y: f32,
}

pub struct Anim {
    pub from: Cam,
    pub to: Cam,
    pub t0: f64,
    pub dur: f64,
}

pub struct Editing {
    pub id: String,
    pub original: String,
    pub text: String,
    pub select_all: bool,
    pub focused: bool,
}

#[derive(Clone, Debug)]
pub struct Orig {
    pub floating_root: bool,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub offset: Option<Offset>,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DropKind {
    Child,
    Before,
    After,
}
#[derive(Clone, Debug)]
pub struct DropAt {
    pub kind: DropKind,
    pub target: usize,
    pub vertical: bool,
    pub side: Option<&'static str>,
}

pub struct Drag {
    pub id: String,
    pub start: Pos2,
    pub cur: Pos2,
    pub started: bool,
    pub narrow_on_click: bool,
    pub ids: Vec<String>,
    pub exclude: HashSet<String>,
    pub key: String,
    pub orig: HashMap<String, Orig>,
    pub t0: (f64, f64, f64, f64),
    pub home: Option<(f64, f64, f64, f64)>,
    pub orig_json: String,
    pub world: (f64, f64),
    pub drop: Option<DropAt>,
    pub flip: Option<&'static str>,
    pub snapped: bool,
}

pub struct Pan {
    pub start: Pos2,
    pub cam: Cam,
    pub moved: bool,
}
pub struct Marquee {
    pub start: Pos2,
    pub base: Vec<String>,
    pub cur: Pos2,
}
pub struct RelDrag {
    pub id: String,
    pub start: Pos2,
    pub moved: bool,
}

pub struct Present {
    pub slides: Vec<Option<String>>,
    pub i: usize,
    pub sel: Vec<String>,
    pub collapsed: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Topic,
    Map,
    Markers,
    Notes,
}

/// What to do after the "Save changes?" question.
#[derive(Clone, Debug)]
pub enum After {
    Open,
    OpenPath(PathBuf),
    Template(String),
    Quit,
    SetDoc(Box<Doc>, String),
}

pub enum Dialog {
    Templates,
    Shortcuts,
    About,
    SaveChanges(After),
    Link(String),
    /// Due date being typed, `YYYY-MM-DD`.
    Due(String),
    RelLabel(String),
    Ai(Box<crate::ui::AiForm>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Node,
    Rel,
    Canvas,
}

pub struct App {
    pub doc: Doc,
    pub path: Option<PathBuf>,
    pub name: String,
    pub dirty: bool,
    pub sel: Vec<String>,
    pub sel_rel: Option<String>,
    pub cam: Cam,
    pub anim: Option<Anim>,
    pub stage: Rect,
    pub pending_fit: bool,
    pub focus_id: Option<String>,
    pub history: History,
    pub editing: Option<Editing>,
    pub relation_from: Option<String>,
    pub clipboard: Vec<Node>,
    pub style_clip: Option<Option<NodeStyle>>,
    pub matches: Vec<String>,
    pub match_idx: usize,
    pub search_open: bool,
    pub search_q: String,
    pub search_focus: bool,
    pub present: Option<Present>,
    pub geo: Geo,
    pub drag: Option<Drag>,
    pub pan: Option<Pan>,
    pub marquee: Option<Marquee>,
    pub rel_drag: Option<RelDrag>,
    pub ctx_target: Target,
    pub ctx_world: (f64, f64),
    pub hover: Option<String>,
    pub prefs: Prefs,
    pub tab: Tab,
    pub dialog: Option<Dialog>,
    pub toasts: Vec<(String, f64)>,
    pub save_status: String,
    pub autosave_key: String,
    pub autosave_due: Option<f64>,
    pub now: f64,
    pub fonts: MapFonts,
    pub measure: MeasureCache,
    pub images: canvas::ImageCache,
    pub db: Arc<Database>,
    pub ctx: egui::Context,
    pub quit_ok: bool,
    pub note_focus: bool,
    pub previews: HashMap<String, Doc>,
    /// Today, `YYYY-MM-DD` in local time (due dates, "overdue").
    pub today: String,
    pub replace_q: String,
}

/// Today's date in local time, `YYYY-MM-DD`.
pub fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

pub fn measure_fn<'a>(ctx: &'a egui::Context, fonts: &'a MapFonts, cache: &'a MeasureCache) -> impl Fn(&str, &FontSpec) -> f64 + 'a {
    move |t: &str, f: &FontSpec| cache.width(ctx, fonts, t, f)
}

/// The first map people see.
pub fn welcome_doc() -> Doc {
    let text = "Welcome to Mindloom\n  Create\n    Tab adds a subtopic\n    Enter adds a topic\n    Just start typing to rename\n    Double-click the canvas for a floating topic\n  Organise\n    Drag topics anywhere, they stay connected\n    Drop onto a topic to re-parent\n    Collapse branches with the ⊖ handle\n    Focus on one branch\n  Style\n    16 colour themes\n    9 structures\n    Shapes, lines and backgrounds\n    Rainbow branches\n  Enrich\n    Markers and stickers\n    Notes and links\n    Images\n    Relationships and boundaries\n  Share\n    PNG, SVG and PDF\n    Markdown, OPML, FreeMind\n    Opens XMind files\n    Presentation mode";
    let mut d = io::from_text(text).unwrap_or_else(|_| Doc::new("Welcome to Mindloom"));
    if d.root.children.len() == 5 {
        let c = &mut d.root.children;
        for (i, m) in ["p1", "🧠", "🎨", "💡", "🚀"].iter().enumerate() {
            c[i].icons = Some(vec![m.to_string()]);
        }
        c[3].children[1].note = Some("Notes appear in the Notes tab on the right. Click the note icon to open them.".into());
        c[3].children[1].link = Some("https://github.com/".into());
        c[1].children[2].task = Some("done".into());
        c[1].children[3].task = Some("todo".into());
        c[4].children[0].tags = Some(vec!["export".into()]);
        c[2].boundary = Some(true);
        c[2].boundary_label = Some("Make it yours".into());
        let (a, b) = (c[0].id.clone(), c[4].id.clone());
        model::add_relation(&mut d, &a, &b, "then");
    }
    d.settings.theme = "fresh".into();
    d.settings.line_style = "tapered".into();
    d
}

impl App {
    pub fn new(ctx: egui::Context, fonts: MapFonts, db: Arc<Database>) -> App {
        let doc = welcome_doc();
        let snap = serde_json::to_string(&doc).unwrap_or_default();
        let mut history = History::new(200, 60_000_000);
        history.reset(snap);
        App {
            sel: vec![doc.root.id.clone()],
            doc,
            path: None,
            name: "Welcome to Mindloom".into(),
            dirty: false,
            sel_rel: None,
            cam: Cam { zoom: 1.0, x: 0.0, y: 0.0 },
            anim: None,
            stage: Rect::from_min_size(Pos2::ZERO, egui::vec2(1200.0, 800.0)),
            pending_fit: true,
            focus_id: None,
            history,
            editing: None,
            relation_from: None,
            clipboard: vec![],
            style_clip: None,
            matches: vec![],
            match_idx: 0,
            search_open: false,
            search_q: String::new(),
            search_focus: false,
            present: None,
            geo: Geo::default(),
            drag: None,
            pan: None,
            marquee: None,
            rel_drag: None,
            ctx_target: Target::Canvas,
            ctx_world: (0.0, 0.0),
            hover: None,
            prefs: files::load_prefs(),
            tab: Tab::Topic,
            dialog: None,
            toasts: vec![],
            save_status: String::new(),
            autosave_key: files::autosave_key(),
            autosave_due: None,
            now: 0.0,
            fonts,
            measure: MeasureCache::default(),
            images: canvas::ImageCache::with_fonts(db.clone()),
            db,
            ctx,
            quit_ok: false,
            note_focus: false,
            previews: HashMap::new(),
            today: today(),
            replace_q: String::new(),
        }
    }

    // ---------- small helpers ----------

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toasts.push((msg.into(), self.now + 2.6));
    }
    pub fn primary(&self) -> Option<String> {
        self.sel.last().cloned()
    }
    pub fn is_root(&self, id: &str) -> bool {
        id == self.doc.root.id
    }
    pub fn theme(&self) -> &'static themes::Theme {
        themes::get_theme(&self.doc.settings.theme)
    }
    pub fn view(&self) -> View {
        View { zoom: self.cam.zoom, off: self.stage.min.to_vec2() + egui::vec2(self.cam.x, self.cam.y) }
    }
    pub fn to_world(&self, p: Pos2) -> (f64, f64) {
        self.view().to_world(p)
    }
    pub fn snapshot(&self) -> String {
        serde_json::to_string(&self.doc).unwrap_or_default()
    }

    /// Lay out the current document (optionally ignoring one topic's hand placement).
    pub fn layout_with<R>(&self, doc: &Doc, ignore: Option<&str>, f: impl FnOnce(&Layout) -> R) -> R {
        let m = measure_fn(&self.ctx, &self.fonts, &self.measure);
        let theme = themes::get_theme(&doc.settings.theme);
        let ctx = Ctx { theme, settings: &doc.settings, measure: &m, focus_id: self.focus_id.as_deref(), ignore_offset_id: ignore, today: &self.today };
        let l = layout::layout(doc, &ctx);
        f(&l)
    }

    /// Recompute the hit-testing geometry and remember the side of each main topic (so later edits never reshuffle the map).
    pub fn refresh_geo(&mut self) {
        let (geo, sides) = self.layout_with(&self.doc, None, |l| {
            let sides: Vec<(String, bool)> = if self.doc.settings.layout == "mindmap" && self.focus_id.is_none() {
                l.items.iter().filter(|t| t.depth == 1 && !t.floating && t.node.side.is_none()).filter_map(|t| t.left.map(|s| (t.id.to_string(), s))).collect()
            } else {
                vec![]
            };
            (Geo::from_layout(l, &self.doc), sides)
        });
        self.geo = geo;
        for (id, left) in sides {
            if let Some(n) = model::get_mut(&mut self.doc, &id) {
                n.side = Some(if left { "left" } else { "right" }.into());
            }
        }
    }

    // ---------- document lifecycle ----------

    pub fn set_doc(&mut self, doc: Doc, path: Option<PathBuf>, name: Option<String>, dirty: bool) {
        self.doc = model::normalize(doc);
        self.path = path;
        self.name = name.unwrap_or_else(|| self.doc.root.text.lines().next().unwrap_or("").chars().take(60).collect());
        if self.name.trim().is_empty() {
            self.name = "Untitled".into();
        }
        self.focus_id = None;
        self.sel_rel = None;
        self.matches.clear();
        self.editing = None;
        self.present = None;
        self.place_imported_floating();
        let snap = self.snapshot();
        self.history.reset(snap);
        self.sel = vec![self.doc.root.id.clone()];
        self.dirty = dirty;
        self.pending_fit = true;
        self.previews.clear();
        self.refresh_geo();
        if let Some(p) = &self.path {
            files::add_recent(p);
            self.prefs.last_path = Some(p.to_string_lossy().to_string());
            files::save_prefs(&self.prefs);
        }
        self.schedule_autosave();
    }

    /// Imported floating topics have no coordinates yet: line them up under the main map.
    fn place_imported_floating(&mut self) {
        if !self.doc.floating.iter().any(|f| f.place == Some(true)) {
            return;
        }
        let mut main = self.doc.clone();
        main.floating.clear();
        let mb = self.layout_with(&main, None, |l| l.bounds);
        let mut x = mb.min_x;
        let y = mb.max_y + 70.0;
        for i in 0..self.doc.floating.len() {
            if self.doc.floating[i].place != Some(true) {
                continue;
            }
            let mut one = self.doc.clone();
            one.root = self.doc.floating[i].clone();
            one.floating.clear();
            one.relations.clear();
            one.settings.layout = "right".into();
            let (bw, bh, rh) = self.layout_with(&one, None, |l| (l.bounds.w, l.bounds.h, l.items.first().map_or(0.0, |r| r.h)));
            let f = &mut self.doc.floating[i];
            f.x = Some(x.round());
            f.y = Some((y + (bh - rh) / 2.0).round());
            f.place = None;
            x += bw + 60.0;
        }
    }

    /// After any change: record undo, mark dirty, schedule autosave, keep the selection in view.
    pub fn commit(&mut self, scroll: bool) {
        let snap = self.snapshot();
        if self.history.push(snap) {
            self.dirty = true;
        }
        self.refresh_geo();
        if scroll {
            if let Some(p) = self.primary() {
                self.ensure_visible(&p);
            }
        }
        self.schedule_autosave();
    }
    pub fn schedule_autosave(&mut self) {
        self.autosave_due = Some(self.now + 0.7);
    }
    pub fn autosave_tick(&mut self) {
        if self.autosave_due.is_some_and(|t| self.now >= t) {
            self.autosave_due = None;
            let a = files::Autosave {
                doc: serde_json::to_value(&self.doc).unwrap_or_default(),
                path: self.path.as_ref().map(|p| p.to_string_lossy().to_string()),
                name: self.name.clone(),
                dirty: self.dirty,
                t: files::now_ms(),
            };
            if files::autosave_write(&self.autosave_key, &a) && self.save_status.is_empty() {
                self.save_status = "Auto-saved".into();
            }
        }
    }

    fn restore(&mut self, snap: &str) {
        let Ok(doc) = serde_json::from_str::<Doc>(snap) else { return };
        self.doc = doc;
        self.sel.retain(|id| model::get(&self.doc, id).is_some());
        if self.sel.is_empty() {
            self.sel = vec![self.doc.root.id.clone()];
        }
        if self.focus_id.as_ref().is_some_and(|f| model::get(&self.doc, f).is_none()) {
            self.focus_id = None;
        }
        self.sel_rel = None;
        self.dirty = true;
        self.refresh_geo();
        self.schedule_autosave();
    }

    // ---------- view ----------

    pub fn animate_to(&mut self, to: Cam, dur: f64) {
        if dur <= 0.0 {
            self.cam = to;
            self.anim = None;
        } else {
            self.anim = Some(Anim { from: self.cam, to, t0: self.now, dur });
        }
    }
    pub fn settle_view(&mut self) {
        if let Some(a) = self.anim.take() {
            self.cam = a.to;
        }
    }
    pub fn step_anim(&mut self) -> bool {
        let Some(a) = &self.anim else { return false };
        let k = ((self.now - a.t0) / a.dur).clamp(0.0, 1.0) as f32;
        let e = 1.0 - (1.0 - k).powi(3);
        self.cam = Cam { zoom: a.from.zoom + (a.to.zoom - a.from.zoom) * e, x: a.from.x + (a.to.x - a.from.x) * e, y: a.from.y + (a.to.y - a.from.y) * e };
        if k >= 1.0 {
            self.anim = None;
        }
        true
    }
    pub fn fit_box(&mut self, b: (f64, f64, f64, f64), animate: bool, max_zoom: f32, pad: f32) {
        let (w, h) = (self.stage.width(), self.stage.height());
        let zoom = ((w - pad * 2.0) / (b.2 as f32).max(1.0)).min((h - pad * 2.0) / (b.3 as f32).max(1.0)).min(max_zoom).max(0.08);
        let (cx, cy) = ((b.0 + b.2 / 2.0) as f32, (b.1 + b.3 / 2.0) as f32);
        self.animate_to(Cam { zoom, x: w / 2.0 - cx * zoom, y: h / 2.0 - cy * zoom }, if animate { 0.3 } else { 0.0 });
    }
    pub fn fit(&mut self, animate: bool) {
        let b = self.geo.bounds;
        self.fit_box(b, animate, 1.1, 60.0);
    }
    pub fn zoom_at(&mut self, factor: f32, at: Option<Pos2>) {
        self.settle_view();
        let c = at.map(|p| p - self.stage.min).unwrap_or(self.stage.size() / 2.0);
        let z = (self.cam.zoom * factor).clamp(0.08, 4.0);
        let (wx, wy) = ((c.x - self.cam.x) / self.cam.zoom, (c.y - self.cam.y) / self.cam.zoom);
        self.cam = Cam { zoom: z, x: c.x - wx * z, y: c.y - wy * z };
    }
    pub fn center_on(&mut self, id: &str, animate: bool) {
        let Some(t) = self.geo.get(id) else { return };
        let (w, h, z) = (self.stage.width(), self.stage.height(), self.cam.zoom);
        let to = Cam { zoom: z, x: w / 2.0 - (t.x + t.w / 2.0) as f32 * z, y: h / 2.0 - (t.y + t.h / 2.0) as f32 * z };
        self.animate_to(to, if animate { 0.25 } else { 0.0 });
    }
    pub fn ensure_visible(&mut self, id: &str) {
        let Some(t) = self.geo.get(id) else { return };
        let (w, h, z, m) = (self.stage.width(), self.stage.height(), self.cam.zoom, 50.0);
        let target = self.anim.as_ref().map(|a| a.to).unwrap_or(self.cam);
        let x1 = t.x as f32 * z + target.x;
        let y1 = t.y as f32 * z + target.y;
        let (x2, y2) = (x1 + t.w as f32 * z, y1 + t.h as f32 * z);
        let dx = if x1 < m {
            m - x1
        } else if x2 > w - m {
            (m - x1).max(w - m - x2)
        } else {
            0.0
        };
        let dy = if y1 < m {
            m - y1
        } else if y2 > h - m {
            (m - y1).max(h - m - y2)
        } else {
            0.0
        };
        if dx != 0.0 || dy != 0.0 {
            self.animate_to(Cam { zoom: target.zoom, x: target.x + dx, y: target.y + dy }, 0.18);
        }
    }

    // ---------- selection & editing ----------

    pub fn select(&mut self, ids: Vec<String>, toggle: bool, scroll: bool) {
        if toggle {
            let mut s = self.sel.clone();
            for id in ids {
                if let Some(i) = s.iter().position(|x| *x == id) {
                    s.remove(i);
                } else {
                    s.push(id);
                }
            }
            self.sel = s;
        } else {
            self.sel = ids;
        }
        self.sel_rel = None;
        if scroll {
            if let Some(p) = self.primary() {
                self.ensure_visible(&p);
            }
        }
    }

    pub fn start_edit(&mut self, id: &str, initial: Option<String>) {
        if self.editing.is_some() {
            self.commit_edit();
        }
        let Some(n) = model::get(&self.doc, id) else { return };
        if self.geo.get(id).is_none() {
            return;
        }
        let original = n.text.clone();
        let select_all = initial.is_none();
        let text = initial.clone().unwrap_or_else(|| original.clone());
        if initial.is_some() {
            if let Some(n) = model::get_mut(&mut self.doc, id) {
                n.text = text.clone();
            }
            self.refresh_geo();
        }
        self.sel = vec![id.to_string()];
        self.sel_rel = None;
        self.editing = Some(Editing { id: id.to_string(), original, text, select_all, focused: false });
        self.ensure_visible(id);
    }
    /// Live update while typing.
    pub fn edit_text(&mut self, text: String) {
        let Some(ed) = &mut self.editing else { return };
        ed.text = text.clone();
        let id = ed.id.clone();
        if let Some(n) = model::get_mut(&mut self.doc, &id) {
            n.text = text;
        }
        self.refresh_geo();
    }
    pub fn commit_edit(&mut self) {
        let Some(ed) = self.editing.take() else { return };
        let text = mindloom_core::js::trim_end(&ed.text).to_string();
        let mut changed = false;
        if let Some(n) = model::get_mut(&mut self.doc, &ed.id) {
            n.text = text.clone();
            changed = text != ed.original;
        }
        if changed {
            if self.path.is_none() && ed.id == self.doc.root.id {
                let first: String = text.lines().next().unwrap_or("").chars().take(60).collect();
                self.name = if first.trim().is_empty() { "Untitled".into() } else { first };
            }
            self.commit(true);
        } else {
            self.refresh_geo();
        }
    }
    pub fn cancel_edit(&mut self) {
        let Some(ed) = self.editing.take() else { return };
        if let Some(n) = model::get_mut(&mut self.doc, &ed.id) {
            n.text = ed.original;
        }
        self.refresh_geo();
    }

    fn default_text(&self, parent: &str) -> &'static str {
        match model::find(&self.doc, parent) {
            Some(l) if l.depth() + 1 + usize::from(l.floating()) <= 1 => "Main Topic",
            Some(_) => "Subtopic",
            None => "Topic",
        }
    }
    fn after_create(&mut self, id: Option<String>) {
        let Some(id) = id else { return };
        self.sel = vec![id.clone()];
        self.sel_rel = None;
        self.commit(true);
        self.start_edit(&id, None);
    }
    pub fn top_level_of(&self, ids: &[String]) -> Vec<String> {
        ids.iter().filter(|id| !ids.iter().any(|o| o != *id && model::is_ancestor(&self.doc, o, id))).cloned().collect()
    }

    // ---------- structure commands ----------

    pub fn add_child(&mut self) {
        let id = self.primary().unwrap_or_else(|| self.doc.root.id.clone());
        let text = self.default_text(&id);
        let n = model::add_child(&mut self.doc, &id, text, None);
        self.after_create(n);
    }
    pub fn add_sibling(&mut self, before: bool) {
        let id = self.primary().unwrap_or_else(|| self.doc.root.id.clone());
        let Some(loc) = model::find(&self.doc, &id) else { return };
        if self.focus_id.as_deref() == Some(id.as_str()) {
            return self.add_child();
        }
        let text = match loc.parent() {
            Some(p) => self.default_text(&model::at(&self.doc, &p).id.clone()),
            None if loc.floating() => "Floating Topic",
            None => "Main Topic",
        };
        let n = model::add_sibling(&mut self.doc, &id, text, before);
        if loc.floating() && !loc.has_parent() {
            if let (Some(nid), Some(&gi)) = (&n, self.geo.by_id.get(&id)) {
                let t = &self.geo.items[gi];
                let b = self.geo.subtree_box(gi);
                let (x, y) = (t.x.round(), (b.3 + 30.0).round());
                if let Some(node) = model::get_mut(&mut self.doc, nid) {
                    node.x = Some(x);
                    node.y = Some(y);
                }
            }
        }
        self.after_create(n);
    }
    pub fn add_parent(&mut self) {
        let Some(id) = self.primary() else { return };
        if self.is_root(&id) {
            return;
        }
        let n = model::add_parent(&mut self.doc, &id, "Topic");
        self.after_create(n);
    }
    pub fn delete_sel(&mut self) {
        if let Some(r) = self.sel_rel.take() {
            self.doc.relations.retain(|x| x.id != r);
            self.commit(false);
            return;
        }
        let ids: Vec<String> = self.sel.iter().filter(|id| !self.is_root(id) && self.focus_id.as_ref() != Some(*id)).cloned().collect();
        if ids.is_empty() {
            return;
        }
        let next = model::remove(&mut self.doc, &ids);
        self.sel = vec![next.unwrap_or_else(|| self.doc.root.id.clone())];
        self.commit(true);
    }
    pub fn edit_primary(&mut self) {
        if let Some(p) = self.primary() {
            self.start_edit(&p, None);
        }
    }
    pub fn duplicate(&mut self) {
        let Some(id) = self.primary() else { return };
        if let Some(c) = model::duplicate(&mut self.doc, &id) {
            self.sel = vec![c];
            self.commit(true);
        }
    }
    pub fn reorder(&mut self, d: i64) {
        if let Some(p) = self.primary() {
            if model::reorder(&mut self.doc, &p, d) {
                self.commit(true);
            }
        }
    }
    pub fn indent(&mut self, out: bool) {
        if let Some(p) = self.primary() {
            let ok = if out { model::outdent(&mut self.doc, &p) } else { model::indent(&mut self.doc, &p) };
            if ok {
                self.commit(true);
            }
        }
    }
    pub fn toggle_collapse(&mut self) {
        let mut changed = false;
        for id in self.sel.clone() {
            changed = model::set_collapsed(&mut self.doc, &id, None) || changed;
        }
        if changed {
            self.commit(true);
        }
    }
    pub fn toggle_collapse_of(&mut self, id: &str) {
        model::set_collapsed(&mut self.doc, id, None);
        self.sel = vec![id.to_string()];
        self.commit(false);
    }
    pub fn collapse_all(&mut self) {
        for c in &mut self.doc.root.children {
            model::walk_mut(c, &mut |n, _| {
                if !n.children.is_empty() {
                    n.collapsed = Some(true);
                }
            });
        }
        self.sel = vec![self.doc.root.id.clone()];
        self.commit(true);
        self.fit(true);
    }
    pub fn expand_all(&mut self) {
        model::walk_doc_mut(&mut self.doc, &mut |n, _| n.collapsed = None);
        self.commit(true);
        self.fit(true);
    }
    pub fn collapse_level(&mut self, level: usize) {
        model::collapse_to_level(&mut self.doc, level);
        self.commit(true);
        self.fit(true);
    }
    pub fn sort_children(&mut self) {
        let Some(id) = self.primary() else { return };
        let Some(n) = model::get_mut(&mut self.doc, &id) else { return };
        if n.children.len() < 2 {
            return;
        }
        n.children.sort_by(|a, b| model::collate(&a.text, &b.text));
        self.commit(true);
    }
    pub fn sort_by_due(&mut self) {
        let Some(id) = self.primary() else { return };
        if model::get(&self.doc, &id).is_none_or(|n| n.children.len() < 2) {
            return;
        }
        if model::sort_by_due(&mut self.doc, &id) {
            self.commit(true);
        } else {
            self.toast("Subtopics are already in due-date order.");
        }
    }
    /// Set (or with `None`, remove) the due date of the selected topics.
    pub fn set_due(&mut self, due: Option<String>) {
        if let Some(d) = &due {
            if !model::is_date_str(d) {
                return self.toast("That is not a valid date — use YYYY-MM-DD, e.g. 2026-12-01.");
            }
        }
        if self.for_selected(|n| n.due = due.clone()) {
            self.commit(false);
        }
    }
    /// Find & replace across all topic text and notes (the search box holds what to find).
    pub fn replace_all(&mut self) {
        let find = self.search_q.clone();
        if find.is_empty() {
            return self.toast("Type what to find first.");
        }
        let n = model::replace_text(&mut self.doc, &find, &self.replace_q.clone(), false);
        if n == 0 {
            return self.toast(format!("No matches for “{find}”."));
        }
        self.commit(false);
        self.run_search();
        self.toast(format!("Replaced {n} occurrence{} — undo with {}", if n == 1 { "" } else { "s" }, crate::ui::kb("Mod+Z")));
    }
    pub fn reset_position(&mut self) {
        let mut k = 0;
        for id in self.sel.clone() {
            if let Some(n) = model::get_mut(&mut self.doc, &id) {
                if n.offset.take().is_some() {
                    k += 1;
                }
            }
        }
        if k > 0 {
            self.commit(false);
        } else {
            self.toast("That topic is already in its automatic position.");
        }
    }
    pub fn reset_all_positions(&mut self) {
        let mut k = 0;
        model::walk_doc_mut(&mut self.doc, &mut |n, _| {
            if n.offset.take().is_some() {
                k += 1;
            }
        });
        if k > 0 {
            self.commit(false);
            self.toast(format!("Reset {k} hand-placed topic{}", if k > 1 { "s" } else { "" }));
            self.fit(true);
        } else {
            self.toast("No topics have been moved by hand.");
        }
    }
    pub fn nudge(&mut self, dx: f64, dy: f64) {
        let key = self.doc.settings.layout.clone();
        let mut moved = false;
        for id in self.top_level_of(&self.sel.clone()) {
            let Some(loc) = model::find(&self.doc, &id) else { continue };
            if (!loc.has_parent() && !loc.floating()) || self.focus_id.as_ref() == Some(&id) {
                continue;
            }
            let n = model::at_mut(&mut self.doc, &loc);
            if loc.floating() && !loc.has_parent() {
                n.x = Some(n.x.unwrap_or(0.0) + dx);
                n.y = Some(n.y.unwrap_or(0.0) + dy);
                moved = true;
                continue;
            }
            let k = if loc.floating() { "float".to_string() } else { key.clone() };
            if !loc.floating() && !layout::can_free_position(&key, Some(loc.depth())) {
                continue;
            }
            let (ox, oy) = n.offset.as_ref().filter(|o| o.l == k).map_or((0.0, 0.0), |o| (o.x, o.y));
            let (nx, ny) = (ox + dx, oy + dy);
            n.offset = if nx == 0.0 && ny == 0.0 { None } else { Some(Offset { x: nx, y: ny, l: k }) };
            moved = true;
        }
        if moved {
            self.commit(true);
        }
    }
    pub fn detach(&mut self) {
        let Some(id) = self.primary() else { return };
        let Some(t) = self.geo.get(&id) else { return };
        let (x, y) = ((t.x + 40.0).round(), (t.y + 40.0).round());
        if model::find(&self.doc, &id).is_some_and(|l| l.has_parent()) && model::detach(&mut self.doc, &id, x, y) {
            self.commit(true);
        }
    }
    pub fn floating_at(&mut self, x: f64, y: f64) {
        let mut n = Node::new("Floating Topic");
        n.x = Some((x - 60.0).round());
        n.y = Some((y - 18.0).round());
        let id = n.id.clone();
        self.doc.floating.push(n);
        self.after_create(Some(id));
    }
    pub fn undo(&mut self) {
        if let Some(s) = self.history.undo().map(String::from) {
            self.restore(&s);
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.history.redo().map(String::from) {
            self.restore(&s);
        }
    }
    fn selected_ids(&self) -> Vec<String> {
        self.sel.iter().filter(|id| model::get(&self.doc, id).is_some()).cloned().collect()
    }
    pub fn for_selected(&mut self, mut f: impl FnMut(&mut Node)) -> bool {
        let ids = self.selected_ids();
        for id in &ids {
            if let Some(n) = model::get_mut(&mut self.doc, id) {
                f(n);
            }
        }
        !ids.is_empty()
    }
    pub fn boundary(&mut self) {
        let on = match self.selected_ids().first().and_then(|id| model::get(&self.doc, id)) {
            Some(n) => !n.has_boundary(),
            None => return,
        };
        self.for_selected(|n| {
            if on {
                n.boundary = Some(true);
            } else {
                n.boundary = None;
                n.boundary_label = None;
            }
        });
        self.commit(true);
    }
    pub fn relation(&mut self) {
        match self.primary() {
            Some(id) => self.relation_from = Some(id),
            None => self.toast("Select a topic first, then choose Relationship."),
        }
    }
    pub fn finish_relation(&mut self, to: Option<String>) {
        let Some(from) = self.relation_from.take() else { return };
        if let Some(to) = to.filter(|t| *t != from) {
            if let Some(r) = model::add_relation(&mut self.doc, &from, &to, "") {
                self.sel_rel = Some(r);
                self.sel.clear();
                self.commit(false);
            }
        }
    }
    pub fn task(&mut self) {
        let next = match self.selected_ids().first().and_then(|id| model::get(&self.doc, id)) {
            Some(n) => match n.task.as_deref() {
                None | Some("") => Some("todo"),
                Some("todo") => Some("done"),
                _ => None,
            },
            None => return,
        };
        self.for_selected(|n| n.task = next.map(String::from));
        self.commit(true);
    }
    pub fn marker(&mut self, m: &str) {
        if self.for_selected(|n| model::toggle_marker(n, m)) {
            self.commit(true);
        }
    }
    /// Change the style of every selected topic; empty styles are removed.
    pub fn style(&mut self, f: impl Fn(&mut NodeStyle)) {
        let any = self.for_selected(|n| {
            let mut st = n.style.clone().unwrap_or_default();
            f(&mut st);
            for v in [&mut st.fill, &mut st.color, &mut st.border, &mut st.line_color, &mut st.shape, &mut st.font, &mut st.align, &mut st.border_dash] {
                if v.as_deref() == Some("") {
                    *v = None;
                }
            }
            n.style = if st == NodeStyle::default() { None } else { Some(st) };
        });
        if any {
            self.commit(false);
        }
    }
    pub fn toggle_style(&mut self, key: &str) {
        let Some(t) = self.primary().and_then(|p| self.geo.get(&p).cloned()) else { return };
        let st = &t.style;
        let cur = match key {
            "bold" => st.bold,
            "italic" => st.italic,
            "underline" => st.underline,
            _ => st.strike,
        };
        let key = key.to_string();
        self.style(move |s| {
            let v = Some(!cur);
            match key.as_str() {
                "bold" => s.bold = v,
                "italic" => s.italic = v,
                "underline" => s.underline = v,
                _ => s.strike = v,
            }
        });
    }
    pub fn focus(&mut self) {
        if self.focus_id.is_some() {
            self.focus_id = None;
        } else {
            let Some(id) = self.primary() else { return self.toast("Select a branch to focus on.") };
            if self.is_root(&id) || model::find(&self.doc, &id).is_none_or(|l| l.floating()) {
                return self.toast("Select a branch to focus on.");
            }
            self.focus_id = Some(id);
        }
        self.refresh_geo();
        self.fit(true);
    }
    pub fn actual_size(&mut self) {
        let (w, h) = (self.stage.width(), self.stage.height());
        let (cx, cy) = ((w / 2.0 - self.cam.x) / self.cam.zoom, (h / 2.0 - self.cam.y) / self.cam.zoom);
        self.animate_to(Cam { zoom: 1.0, x: w / 2.0 - cx, y: h / 2.0 - cy }, 0.26);
    }
    pub fn open_link(&mut self) {
        let link = self.primary().and_then(|p| model::get(&self.doc, &p).and_then(|n| n.link.clone()));
        if let Some(l) = link {
            self.ctx.open_url(egui::OpenUrl::new_tab(l));
        }
    }
    pub fn relation_reverse(&mut self) {
        let Some(id) = self.sel_rel.clone() else { return };
        if let Some(r) = self.doc.relations.iter_mut().find(|r| r.id == id) {
            std::mem::swap(&mut r.from, &mut r.to);
            self.commit(false);
        }
    }

    // ---------- search ----------

    pub fn run_search(&mut self) {
        self.matches = model::search(&self.doc, &self.search_q, &self.today);
        self.match_idx = 0;
        for id in self.matches.clone() {
            model::expand_to(&mut self.doc, &id);
        }
        if let Some(f) = self.focus_id.clone() {
            if self.matches.iter().any(|id| *id != f && !model::is_ancestor(&self.doc, &f, id)) {
                self.focus_id = None;
            }
        }
        self.refresh_geo();
        if let Some(first) = self.matches.first().cloned() {
            self.sel = vec![first.clone()];
            self.center_on(&first, true);
        }
    }
    pub fn step_search(&mut self, d: i64) {
        if self.matches.is_empty() {
            return;
        }
        let n = self.matches.len() as i64;
        self.match_idx = ((self.match_idx as i64 + d + n) % n) as usize;
        let id = self.matches[self.match_idx].clone();
        self.sel = vec![id.clone()];
        self.center_on(&id, true);
    }
    pub fn close_search(&mut self) {
        self.search_open = false;
        self.matches.clear();
    }

    // ---------- keyboard navigation ----------

    pub fn navigate(&mut self, dir: &str) {
        let Some(ci) = self.primary().and_then(|p| self.geo.by_id.get(&p).copied()) else {
            let id = self.focus_id.clone().unwrap_or_else(|| self.doc.root.id.clone());
            return self.select(vec![id], false, true);
        };
        let cur = &self.geo.items[ci];
        let (cx, cy) = (cur.x + cur.w / 2.0, cur.y + cur.h / 2.0);
        let mut best: Option<(usize, f64)> = None;
        for (i, t) in self.geo.items.iter().enumerate() {
            if i == ci {
                continue;
            }
            let (dx, dy) = (t.x + t.w / 2.0 - cx, t.y + t.h / 2.0 - cy);
            let (main, cross) = match dir {
                "right" => (if t.x - (cur.x + cur.w) >= -cur.w / 2.0 { dx } else { -1.0 }, dy),
                "left" => (if cur.x - (t.x + t.w) >= -cur.w / 2.0 { -dx } else { -1.0 }, dy),
                "down" => (dy, dx),
                _ => (-dy, dx),
            };
            if main <= 2.0 {
                continue;
            }
            let related = t.parent == Some(ci) || cur.parent == Some(i) || (t.parent.is_some() && t.parent == cur.parent);
            let score = main + cross.abs() * 2.2 - if related { 30.0 } else { 0.0 };
            if best.is_none_or(|b| score < b.1) {
                best = Some((i, score));
            }
        }
        if let Some((i, _)) = best {
            let id = self.geo.items[i].id.clone();
            self.select(vec![id], false, true);
        }
    }

    // ---------- clipboard ----------

    fn top_selected(&self) -> Vec<Node> {
        self.top_level_of(&self.sel).iter().filter_map(|id| model::get(&self.doc, id).cloned()).collect()
    }
    fn subtree_text(nodes: &[Node]) -> String {
        let mut out = vec![];
        for n in nodes {
            model::walk(n, &mut |x, d| out.push("\t".repeat(d) + &x.text.replace('\n', " ")));
        }
        out.join("\n")
    }
    /// Copies the selected branches; returns their text for the system clipboard.
    pub fn copy(&mut self) -> Option<String> {
        let nodes = self.top_selected();
        if nodes.is_empty() {
            return None;
        }
        let text = Self::subtree_text(&nodes);
        self.toast(format!("Copied {} topic{}", nodes.len(), if nodes.len() > 1 { "s" } else { "" }));
        self.clipboard = nodes;
        Some(text)
    }
    pub fn cut(&mut self) -> Option<String> {
        let nodes: Vec<Node> = self.top_selected().into_iter().filter(|n| n.id != self.doc.root.id).collect();
        if nodes.is_empty() {
            return None;
        }
        let text = Self::subtree_text(&nodes);
        self.sel = nodes.iter().map(|n| n.id.clone()).collect();
        self.clipboard = nodes;
        self.delete_sel();
        Some(text)
    }
    pub fn paste(&mut self, text: Option<&str>) {
        let target = self.primary().unwrap_or_else(|| self.doc.root.id.clone());
        let mut added = vec![];
        let own = !self.clipboard.is_empty() && text.is_some_and(|t| t == Self::subtree_text(&self.clipboard));
        match text.filter(|t| !t.trim().is_empty() && !own) {
            Some(t) => {
                let (roots, _) = io::parse_indented(t);
                for r in roots {
                    added.extend(model::insert_subtree(&mut self.doc, &target, &r, None));
                }
            }
            None => {
                for n in self.clipboard.clone() {
                    added.extend(model::insert_subtree(&mut self.doc, &target, &n, None));
                }
            }
        }
        if added.is_empty() {
            if text.is_none() && self.clipboard.is_empty() {
                self.toast("The clipboard is empty.");
            }
            return;
        }
        self.sel = added;
        self.commit(true);
    }
    pub fn copy_style(&mut self) {
        let Some(n) = self.primary().and_then(|p| model::get(&self.doc, &p).cloned()) else { return };
        self.style_clip = Some(n.style);
        self.toast("Style copied — select topics and choose Paste style");
    }
    pub fn paste_style(&mut self) {
        let Some(c) = self.style_clip.clone() else { return self.toast("Copy a topic's style first.") };
        if self.for_selected(|n| n.style = c.clone()) {
            self.commit(false);
        }
    }

    // ---------- images ----------

    pub fn set_image(&mut self, id: Option<String>, img: image::DynamicImage) {
        let Some(id) = id.or_else(|| self.primary()) else { return self.toast("Select a topic first.") };
        match files::image_data_url(img) {
            Ok((src, w, h)) => {
                if let Some(n) = model::get_mut(&mut self.doc, &id) {
                    n.image = Some(model::Image { src, w, h });
                }
                self.sel = vec![id];
                self.commit(true);
            }
            Err(_) => self.toast("Could not load that image."),
        }
    }
    pub fn pick_image(&mut self) {
        if self.primary().is_none() {
            return self.toast("Select a topic first.");
        }
        let Some(path) = rfd::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp", "bmp"]).pick_file() else { return };
        match image::open(&path) {
            Ok(img) => self.set_image(None, img),
            Err(_) => self.toast("Could not load that image."),
        }
    }

    // ---------- files ----------

    /// Run `after` now, or ask about unsaved changes first.
    pub fn guard(&mut self, after: After) {
        if self.dirty {
            self.dialog = Some(Dialog::SaveChanges(after));
        } else {
            self.run_after(after);
        }
    }
    pub fn run_after(&mut self, after: After) {
        match after {
            After::Open => self.open_dialog(),
            After::OpenPath(p) => {
                self.load_path(&p);
            }
            After::Template(id) => {
                let d = templates::build(&id);
                let name = if id == "blank" { "Untitled".to_string() } else { d.root.text.clone() };
                self.set_doc(d, None, Some(name), false);
                if id == "blank" {
                    let r = self.doc.root.id.clone();
                    self.start_edit(&r, None);
                }
            }
            After::Quit => {
                self.quit_ok = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            After::SetDoc(d, name) => {
                self.set_doc(*d, None, Some(name), true);
            }
        }
    }
    pub fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("Mind maps", files::OPEN_EXTS).add_filter("All files", &["*"]).pick_file() {
            self.load_path(&p);
        }
    }
    pub fn load_path(&mut self, p: &std::path::Path) -> bool {
        match files::read_map(p) {
            Ok(doc) => {
                let native = files::is_native(p);
                let base = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
                self.set_doc(doc, native.then(|| p.to_path_buf()), Some(base), !native);
                if !native {
                    self.toast(format!("Imported {}", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()));
                }
                true
            }
            Err(e) => {
                self.toast(format!("Could not open {}: {e}", p.display()));
                false
            }
        }
    }
    pub fn save(&mut self, save_as: bool) -> bool {
        if self.editing.is_some() {
            self.commit_edit();
        }
        let path = match (&self.path, save_as) {
            (Some(p), false) => p.clone(),
            _ => {
                let Some(p) = rfd::FileDialog::new()
                    .add_filter("Mindloom map", &["mindloom"])
                    .set_file_name(format!("{}.mindloom", files::file_base(&self.name)))
                    .save_file()
                else {
                    return false;
                };
                if p.extension().is_none() {
                    p.with_extension("mindloom")
                } else {
                    p
                }
            }
        };
        match files::write_atomic(&path, io::serialize(&self.doc).as_bytes()) {
            Ok(()) => {
                self.name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| self.name.clone());
                files::add_recent(&path);
                self.prefs.last_path = Some(path.to_string_lossy().to_string());
                files::save_prefs(&self.prefs);
                self.path = Some(path);
                self.dirty = false;
                self.save_status = "Saved".into();
                self.schedule_autosave();
                true
            }
            Err(e) => {
                self.toast(format!("Save failed: {e}"));
                false
            }
        }
    }

    // ---------- export ----------

    fn export_svg(&self, transparent: bool) -> mindloom_core::render::Exported {
        self.layout_with(&self.doc, None, |l| files::export_svg(l, &self.doc, self.theme(), &self.doc.settings, transparent, None))
    }
    fn save_bytes(&mut self, ext: &str, label: &str, bytes: &[u8], done: &str) {
        let Some(p) = rfd::FileDialog::new().add_filter(label, &[ext]).set_file_name(format!("{}.{ext}", files::file_base(&self.name))).save_file() else {
            return;
        };
        match files::write_atomic(&p, bytes) {
            Ok(()) => self.toast(done.to_string()),
            Err(e) => self.toast(format!("Export failed: {e}")),
        }
    }
    pub fn export(&mut self, kind: &str) {
        if self.editing.is_some() {
            self.commit_edit();
        }
        match kind {
            "png" | "png-t" => {
                let e = self.export_svg(kind == "png-t");
                match files::png(&e.svg, e.width, e.height, 2.0, &self.db) {
                    Ok(b) => self.save_bytes("png", "PNG image", &b, "Exported PNG"),
                    Err(err) => self.toast(format!("Export failed: {err}")),
                }
            }
            "svg" => {
                let e = self.export_svg(false);
                let s = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{}", e.svg);
                self.save_bytes("svg", "SVG image", s.as_bytes(), "Exported SVG");
            }
            "pdf" => {
                let e = self.export_svg(false);
                match files::pdf(&e.svg, &self.db) {
                    Ok(b) => self.save_bytes("pdf", "PDF document", &b, "Exported PDF"),
                    Err(err) => self.toast(format!("Export failed: {err}")),
                }
            }
            "md" => self.save_bytes("md", "Markdown", io::to_markdown(&self.doc).as_bytes(), "Exported Markdown"),
            "opml" => self.save_bytes("opml", "OPML", io::to_opml(&self.doc).as_bytes(), "Exported OPML"),
            "txt" => self.save_bytes("txt", "Text", io::to_text(&self.doc).as_bytes(), "Exported text outline"),
            "mm" => self.save_bytes("mm", "FreeMind", io::to_freemind(&self.doc).as_bytes(), "Exported FreeMind map"),
            "csv" => self.save_bytes("csv", "CSV spreadsheet", io::to_csv(&self.doc).as_bytes(), "Exported CSV — opens in Excel, Sheets or Numbers"),
            "html" => {
                let svg = self.export_svg(false).svg;
                let page = io::to_html(&self.doc, &svg, &self.today);
                self.save_bytes("html", "Web page", page.as_bytes(), "Exported web page — open it in any browser");
            }
            "copy-image" => {
                let e = self.export_svg(false);
                let r = files::raster(&e.svg, e.width, e.height, 2.0, &self.db).map_err(|e| e.to_string()).and_then(|pm| {
                    let img = arboard::ImageData { width: pm.width() as usize, height: pm.height() as usize, bytes: files::rgba(&pm).into() };
                    arboard::Clipboard::new().and_then(|mut c| c.set_image(img)).map_err(|e| e.to_string())
                });
                match r {
                    Ok(()) => self.toast("Map image copied to clipboard"),
                    Err(err) => self.toast(format!("Could not copy the image: {err}")),
                }
            }
            _ => {}
        }
    }

    // ---------- presentation ----------

    pub fn present(&mut self) {
        if self.editing.is_some() {
            self.commit_edit();
        }
        self.focus_id = None;
        let mut slides = vec![None];
        slides.extend(self.doc.root.children.iter().map(|c| Some(c.id.clone())));
        let mut collapsed = vec![];
        model::walk_doc(&self.doc, &mut |n, _| {
            if n.is_collapsed() {
                collapsed.push(n.id.clone());
            }
        });
        self.present = Some(Present { slides, i: 0, sel: std::mem::take(&mut self.sel), collapsed });
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        self.pending_fit = false;
        self.show_slide();
    }
    pub fn present_step(&mut self, d: i64) {
        let Some(p) = &mut self.present else { return };
        p.i = (p.i as i64 + d).clamp(0, p.slides.len() as i64 - 1) as usize;
        self.show_slide();
    }
    pub fn show_slide(&mut self) {
        let Some(p) = &self.present else { return };
        let slide = p.slides[p.i].clone();
        self.sel.clear();
        match slide {
            None => {
                self.refresh_geo();
                let b = self.geo.bounds;
                self.fit_box(b, true, 1.6, 50.0);
            }
            Some(id) => {
                if let Some(n) = model::get_mut(&mut self.doc, &id) {
                    n.collapsed = None;
                }
                self.refresh_geo();
                if let Some(&i) = self.geo.by_id.get(&id) {
                    let b = self.geo.subtree_box(i);
                    self.fit_box((b.0, b.1, b.2 - b.0, b.3 - b.1), true, 2.0, 90.0);
                }
            }
        }
    }
    pub fn present_exit(&mut self) {
        let Some(p) = self.present.take() else { return };
        self.sel = p.sel;
        let was: HashSet<String> = p.collapsed.into_iter().collect();
        model::walk_doc_mut(&mut self.doc, &mut |n, _| n.collapsed = if was.contains(&n.id) { Some(true) } else { None });
        self.ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        self.refresh_geo();
        self.pending_fit = true;
    }

    // ---------- dragging topics ----------
    // Dropping on a topic re-parents (centre) or reorders (top/bottom edge). Dropping on empty canvas keeps
    // the topic attached to its parent and just moves it (and its branch) there. A main topic dragged across
    // the central topic switches sides. Close to its home slot it snaps back.

    pub fn press_node(&mut self, id: String, at: Pos2) {
        let in_multi = self.sel.contains(&id) && self.sel.len() > 1;
        if !in_multi && (!self.sel.contains(&id) || self.sel_rel.is_some()) {
            self.select(vec![id.clone()], false, false);
        }
        self.drag = Some(Drag {
            id,
            start: at,
            cur: at,
            started: false,
            narrow_on_click: in_multi,
            ids: vec![],
            exclude: HashSet::new(),
            key: String::new(),
            orig: HashMap::new(),
            t0: (0.0, 0.0, 0.0, 0.0),
            home: None,
            orig_json: String::new(),
            world: (0.0, 0.0),
            drop: None,
            flip: None,
            snapped: false,
        });
    }

    fn begin_drag(&mut self) -> bool {
        let Some(d) = &self.drag else { return false };
        let primary = d.id.clone();
        let ids = if self.sel.contains(&primary) && self.sel.len() > 1 { self.top_level_of(&self.sel) } else { vec![primary.clone()] };
        let ids: Vec<String> =
            ids.into_iter().filter(|id| !self.is_root(id) && self.focus_id.as_ref() != Some(id) && model::find(&self.doc, id).is_some()).collect();
        if ids.is_empty() {
            return false;
        }
        let mut exclude = HashSet::new();
        let mut orig = HashMap::new();
        for id in &ids {
            if let Some(n) = model::get(&self.doc, id) {
                model::walk(n, &mut |x, _| {
                    exclude.insert(x.id.clone());
                });
            }
            if let Some(loc) = model::find(&self.doc, id) {
                let n = model::at(&self.doc, &loc);
                orig.insert(
                    id.clone(),
                    Orig {
                        floating_root: loc.floating() && !loc.has_parent(),
                        x: n.x,
                        y: n.y,
                        offset: n.offset.clone(),
                        depth: loc.depth() + usize::from(loc.floating()),
                    },
                );
            }
        }
        let Some(t) = self.geo.get(&primary) else { return false };
        let t0 = (t.x, t.y, t.w, t.h);
        let home = self.layout_with(&self.doc, Some(&primary), |l| l.get(&primary).map(|h| (h.x, h.y, h.w, h.h)));
        let orig_json = self.snapshot();
        let key = self.doc.settings.layout.clone();
        if let Some(d) = &mut self.drag {
            d.started = true;
            d.ids = ids;
            d.exclude = exclude;
            d.orig = orig;
            d.t0 = t0;
            d.home = home;
            d.orig_json = orig_json;
            d.key = key;
        }
        true
    }

    pub fn can_free(&self, id: &str) -> bool {
        let Some(d) = &self.drag else { return false };
        let Some(o) = d.orig.get(id) else { return false };
        if o.floating_root || model::find(&self.doc, id).is_some_and(|l| l.floating()) {
            return true;
        }
        layout::can_free_position(&d.key, Some(o.depth))
    }

    /// Pointer moved while pressing a topic.
    pub fn drag_move(&mut self, at: Pos2) {
        let Some(d) = &mut self.drag else { return };
        d.cur = at;
        if !d.started {
            if (at - d.start).length() < 5.0 {
                return;
            }
            let id = d.id.clone();
            if self.is_root(&id) || self.focus_id.as_ref() == Some(&id) || !self.begin_drag() {
                self.drag = None;
                return;
            }
        }
        self.drag_step();
    }

    fn drag_step(&mut self) {
        let Some(d) = &self.drag else { return };
        let w = self.to_world(d.cur);
        let s = self.to_world(d.start);
        let (dx, dy) = (w.0 - s.0, w.1 - s.1);
        let drop = self.find_drop(w);
        let flip = if drop.is_none() { self.side_flip(w) } else { None };
        if let Some(d) = &mut self.drag {
            d.world = w;
            d.drop = drop;
            d.flip = flip;
        }
        self.apply_drag_delta(dx, dy);
        self.refresh_geo();
    }

    fn apply_drag_delta(&mut self, dx: f64, dy: f64) {
        const SNAP: f64 = 10.0;
        let Some(d) = &self.drag else { return };
        let (ids, key, orig, primary, t0, home) = (d.ids.clone(), d.key.clone(), d.orig.clone(), d.id.clone(), d.t0, d.home);
        let mut snapped = false;
        for id in &ids {
            let Some(o) = orig.get(id) else { continue };
            if o.floating_root {
                if let Some(n) = model::get_mut(&mut self.doc, id) {
                    n.x = Some((o.x.unwrap_or(0.0) + dx).round());
                    n.y = Some((o.y.unwrap_or(0.0) + dy).round());
                }
                continue;
            }
            if !self.can_free(id) {
                continue;
            }
            let floating = model::find(&self.doc, id).is_some_and(|l| l.floating());
            let k = if floating { "float".to_string() } else { key.clone() };
            let (bx, by) = o.offset.as_ref().filter(|x| x.l == k).map_or((0.0, 0.0), |x| (x.x, x.y));
            let (mut ox, mut oy) = (bx + dx, by + dy);
            if *id == primary && ids.len() == 1 {
                if let Some(h) = home {
                    // distance between where the topic would land and its home slot
                    let (lx, ly) = (t0.0 + dx - h.0, t0.1 + dy - h.1);
                    if lx.abs() < SNAP && ly.abs() < SNAP {
                        ox -= lx;
                        oy -= ly;
                        snapped = true;
                    }
                }
            }
            if let Some(n) = model::get_mut(&mut self.doc, id) {
                n.offset = if ox.abs() < 0.5 && oy.abs() < 0.5 { None } else { Some(Offset { x: ox.round(), y: oy.round(), l: k }) };
            }
        }
        if let Some(d) = &mut self.drag {
            d.snapped = snapped;
        }
    }

    /// A main topic of the mind map dragged to the other side of the central topic.
    fn side_flip(&self, w: (f64, f64)) -> Option<&'static str> {
        let d = self.drag.as_ref()?;
        if d.key != "mindmap" || self.focus_id.is_some() || d.ids.len() != 1 {
            return None;
        }
        let loc = model::find(&self.doc, &d.id)?;
        if loc.floating() || loc.depth() != 1 {
            return None;
        }
        let r = self.geo.get(&self.doc.root.id)?;
        let cur = self.geo.get(&d.id).and_then(|t| t.left).map(|l| if l { "left" } else { "right" }).or(model::at(&self.doc, &loc).side.as_deref().map(|s| {
            if s == "left" {
                "left"
            } else {
                "right"
            }
        }));
        let side = if w.0 < r.x + r.w / 2.0 { "left" } else { "right" };
        (Some(side) != cur).then_some(side)
    }

    fn find_drop(&self, w: (f64, f64)) -> Option<DropAt> {
        let d = self.drag.as_ref()?;
        let lay = self.doc.settings.layout.as_str();
        let mut best: Option<usize> = None;
        for (i, t) in self.geo.items.iter().enumerate() {
            if d.exclude.contains(&t.id) || w.0 < t.x - 6.0 || w.0 > t.x + t.w + 6.0 || w.1 < t.y - 6.0 || w.1 > t.y + t.h + 6.0 {
                continue;
            }
            if best.is_some_and(|b| self.geo.items[b].w * self.geo.items[b].h < t.w * t.h) {
                continue;
            }
            best = Some(i);
        }
        let i = best?;
        let t = &self.geo.items[i];
        let loc = model::find(&self.doc, &t.id)?;
        if !loc.has_parent() || self.focus_id.as_ref() == Some(&t.id) {
            let side = (lay == "mindmap" && t.id == self.doc.root.id && self.focus_id.is_none()).then(|| if w.0 < t.x + t.w / 2.0 { "left" } else { "right" });
            return Some(DropAt { kind: DropKind::Child, target: i, vertical: false, side });
        }
        let vertical = matches!(t.dir, Dir::D | Dir::U) || (t.depth == 1 && matches!(lay, "down" | "up" | "timeline"));
        let rel = if vertical { (w.0 - t.x) / t.w } else { (w.1 - t.y) / t.h };
        let kind = if rel < 0.25 {
            DropKind::Before
        } else if rel > 0.75 {
            DropKind::After
        } else {
            DropKind::Child
        };
        Some(DropAt { kind, target: i, vertical, side: None })
    }

    pub fn cancel_drag(&mut self) {
        if let Some(d) = self.drag.take() {
            if d.started {
                if let Ok(doc) = serde_json::from_str(&d.orig_json) {
                    self.doc = doc;
                }
                self.refresh_geo();
            }
        }
    }

    /// Pointer released after pressing a topic.
    pub fn drag_end(&mut self) {
        if self.drag.as_ref().is_some_and(|d| d.started) {
            self.drag_step();
        }
        let Some(d) = self.drag.take() else { return };
        if !d.started {
            if d.narrow_on_click {
                self.select(vec![d.id], false, false);
            }
            return;
        }
        let restore_offsets = |doc: &mut Doc| {
            for id in &d.ids {
                let (Some(o), Some(n)) = (d.orig.get(id), model::get_mut(doc, id)) else { continue };
                if o.floating_root {
                    n.x = o.x;
                    n.y = o.y;
                }
                n.offset = o.offset.clone();
            }
        };
        let mut changed = false;
        if let Some(drop) = &d.drop {
            restore_offsets(&mut self.doc);
            let target = self.geo.items[drop.target].id.clone();
            if drop.kind == DropKind::Child {
                for id in &d.ids {
                    if model::move_node(&mut self.doc, id, &target, None) {
                        changed = true;
                        if let Some(n) = model::get_mut(&mut self.doc, id) {
                            n.side = drop.side.map(String::from);
                        }
                    }
                }
            } else {
                let order: Vec<String> = if drop.kind == DropKind::After { d.ids.iter().rev().cloned().collect() } else { d.ids.clone() };
                for id in order {
                    let (Some(f), Some(tf)) = (model::find(&self.doc, &id), model::find(&self.doc, &target)) else { break };
                    let Some(tp) = tf.parent() else { break };
                    let tparent = model::at(&self.doc, &tp).id.clone();
                    let tside = model::at(&self.doc, &tf).side.clone();
                    let mut idx = tf.index() + usize::from(drop.kind == DropKind::After);
                    if let Some(fp) = f.parent() {
                        if model::at(&self.doc, &fp).id == tparent && f.index() < idx {
                            idx -= 1;
                        }
                    }
                    if model::move_node(&mut self.doc, &id, &tparent, Some(idx as i64)) {
                        changed = true;
                        if let Some(n) = model::get_mut(&mut self.doc, &id) {
                            n.side = tside;
                        }
                    }
                }
            }
        } else if let Some(flip) = d.flip {
            restore_offsets(&mut self.doc);
            let kids = std::mem::take(&mut self.doc.root.children);
            let (mut moving, rest): (Vec<Node>, Vec<Node>) = kids.into_iter().partition(|c| c.id == d.id);
            if let Some(mut n) = moving.pop() {
                let side_of = |c: &Node| if c.side.as_deref() == Some("left") { "left" } else { "right" };
                let mut right: Vec<Node> = rest.iter().filter(|c| side_of(c) == "right").cloned().collect();
                let mut left: Vec<Node> = rest.iter().filter(|c| side_of(c) == "left").cloned().collect();
                left.reverse(); // the left side is stacked bottom-up
                let target = if flip == "left" { &mut left } else { &mut right };
                let at = target.iter().position(|c| self.geo.get(&c.id).is_some_and(|t| d.world.1 < t.y + t.h / 2.0)).unwrap_or(target.len());
                n.side = Some(flip.into());
                n.offset = None;
                target.insert(at, n);
                left.reverse();
                right.extend(left);
                self.doc.root.children = right;
                changed = true;
            } else {
                self.doc.root.children = rest;
            }
        } else {
            let free = d.ids.iter().any(|id| self.drag_can_free(&d, id));
            if !free {
                restore_offsets(&mut self.doc);
                let name = themes::LAYOUTS.iter().find(|l| l.id == d.key).map_or(d.key.as_str(), |l| l.name);
                let msg = if d.key == "timeline" {
                    "Timeline topics stay on the axis. Drop onto a topic to move it, or drag a subtopic freely.".to_string()
                } else {
                    format!("Free positioning isn't available in {name}. Drop onto a topic to move it there.")
                };
                self.toast(msg);
            }
            changed = self.snapshot() != d.orig_json;
        }
        if changed {
            self.commit(false);
        } else {
            self.refresh_geo();
        }
    }
    fn drag_can_free(&self, d: &Drag, id: &str) -> bool {
        let Some(o) = d.orig.get(id) else { return false };
        o.floating_root || model::find(&self.doc, id).is_some_and(|l| l.floating()) || layout::can_free_position(&d.key, Some(o.depth))
    }

    /// Reshape a relationship curve: its middle follows the pointer.
    pub fn rel_drag_move(&mut self, at: Pos2) {
        let Some(rd) = &mut self.rel_drag else { return };
        if !rd.moved && (at - rd.start).length() < 4.0 {
            return;
        }
        rd.moved = true;
        let id = rd.id.clone();
        let w = self.to_world(at);
        let Some(r) = self.doc.relations.iter().find(|r| r.id == id).cloned() else { return };
        let (Some(a), Some(b)) = (self.geo.get(&r.from), self.geo.get(&r.to)) else { return };
        let (ax, ay, bx, by) = (a.x + a.w / 2.0, a.y + a.h / 2.0, b.x + b.w / 2.0, b.y + b.h / 2.0);
        let (dx, dy) = (bx - ax, by - ay);
        let len = dx.hypot(dy).max(1.0);
        let (nx, ny) = (-dy / len, dx / len);
        // the curve's middle sits 0.75 × bend away from the chord
        let bend = (((w.0 - (ax + bx) / 2.0) * nx + (w.1 - (ay + by) / 2.0) * ny) / 0.75).round().clamp(-2000.0, 2000.0);
        if let Some(r) = self.doc.relations.iter_mut().find(|r| r.id == id) {
            r.bend = Some(bend);
        }
        self.refresh_geo();
    }

    pub fn marquee_end(&mut self) {
        let Some(m) = self.marquee.take() else { return };
        if (m.cur - m.start).length() < 3.0 {
            return;
        }
        let (a, b) = (self.to_world(m.start), self.to_world(m.cur));
        let (x1, y1, x2, y2) = (a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
        let mut ids = m.base;
        for t in &self.geo.items {
            if t.x < x2 && t.x + t.w > x1 && t.y < y2 && t.y + t.h > y1 && !ids.contains(&t.id) {
                ids.push(t.id.clone());
            }
        }
        self.select(ids, false, false);
    }

    /// Imported or AI-generated outline as a new map or a branch.
    pub fn stats_line(&self) -> String {
        let s = model::stats(&self.doc, &self.today);
        let tasks =
            if s.tasks > 0 { format!(" · {}/{} tasks done ({}%)", s.done, s.tasks, (s.done as f64 / s.tasks as f64 * 100.0).round()) } else { String::new() };
        let overdue = if s.overdue > 0 { format!(" · ⚠ {} overdue", s.overdue) } else { String::new() };
        format!("{} topics · {} words{tasks}{overdue}", s.nodes, s.words)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn welcome_has_extras() {
        let d = welcome_doc();
        assert_eq!(d.root.children.len(), 5);
        assert_eq!(d.relations.len(), 1);
        assert_eq!(d.root.children[2].boundary_label.as_deref(), Some("Make it yours"));
    }
}
