//! Replays what Mindloom 1.x's JavaScript engine did (recorded by `rust/record.js` into
//! `rust/cases/`) and holds the Rust engine to it: numbers within 1e-6, strings exactly.

use flate2::read::GzDecoder;
use mindloom_core::layout::{self, Ctx, FontSpec, Layout};
use mindloom_core::model::{self, Doc, Node};
use mindloom_core::render::{self, SceneOpts};
use mindloom_core::themes::{self, Style};
use mindloom_core::{io, js};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::io::Read;

fn cases(name: &str) -> Vec<Value> {
    let path = format!("{}/../cases/{name}.json.gz", env!("CARGO_MANIFEST_DIR"));
    let mut s = String::new();
    GzDecoder::new(std::fs::File::open(&path).unwrap_or_else(|e| panic!("{path}: {e}"))).read_to_string(&mut s).unwrap();
    match serde_json::from_str(&s).unwrap() {
        Value::Array(a) => a,
        _ => panic!("{name}: not a list"),
    }
}

/// Differences between the Rust value and the recorded one (a missing key equals null).
fn diff(path: &str, r: &Value, j: &Value, out: &mut Vec<String>) {
    if out.len() > 6 {
        return;
    }
    match (r, j) {
        (Value::Number(a), Value::Number(b)) => {
            let (a, b) = (a.as_f64().unwrap(), b.as_f64().unwrap());
            if (a - b).abs() > 1e-6 {
                out.push(format!("{path}: rust {a} js {b}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            let mut seen = std::collections::HashSet::new();
            for k in a.keys().chain(b.keys()) {
                if seen.insert(k) {
                    diff(&format!("{path}.{k}"), a.get(k).unwrap_or(&Value::Null), b.get(k).unwrap_or(&Value::Null), out);
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            if a.len() != b.len() {
                out.push(format!("{path}: rust len {} js len {}\n   rust {}\n   js   {}", a.len(), b.len(), short(r), short(j)));
                return;
            }
            for (i, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}[{i}]"), x, y, out);
            }
        }
        _ if r == j => {}
        _ => out.push(format!("{path}: rust {} js {}", short(r), short(j))),
    }
}
fn short(v: &Value) -> String {
    let s = v.to_string();
    match s.char_indices().nth(300) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s,
    }
}
fn str_diff(path: &str, r: &str, j: &str, out: &mut Vec<String>) {
    if r != j {
        let at = r.bytes().zip(j.bytes()).take_while(|(a, b)| a == b).count();
        let cut = |s: &str| {
            let a = (0..=at.saturating_sub(80).min(s.len())).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0);
            let b = ((at + 120).min(s.len())..=s.len()).find(|i| s.is_char_boundary(*i)).unwrap_or(s.len());
            s[a..b].to_string()
        };
        out.push(format!("{path}: differs at byte {at}\n   rust …{}\n   js   …{}", cut(r), cut(j)));
    }
}

struct Report {
    name: &'static str,
    total: usize,
    failed: usize,
    shown: Vec<String>,
}
impl Report {
    fn new(name: &'static str) -> Self {
        Report { name, total: 0, failed: 0, shown: vec![] }
    }
    fn case(&mut self, label: String, errs: Vec<String>) {
        self.total += 1;
        if !errs.is_empty() {
            self.failed += 1;
            if self.shown.len() < 6 {
                self.shown.push(format!("--- {label}\n{}", errs.join("\n")));
            }
        }
    }
    fn finish(self) {
        eprintln!("{}: {}/{} match", self.name, self.total - self.failed, self.total);
        if self.failed > 0 {
            panic!("{} of {} {} cases differ:\n{}", self.failed, self.total, self.name, self.shown.join("\n"));
        }
    }
}

fn style_json(s: &Style) -> Value {
    json!({ "level": s.level, "fill": s.fill, "color": s.color, "border": s.border, "borderWidth": s.border_width, "borderDash": s.border_dash,
        "line": s.line, "shape": s.shape, "fontSize": s.font_size, "bold": s.bold, "italic": s.italic, "underline": s.underline,
        "strike": s.strike, "align": s.align, "font": s.font })
}
fn pt(p: (f64, f64)) -> Value {
    json!([p.0, p.1])
}

fn summary(l: &Layout) -> Value {
    let list: Vec<Value> = l
        .items
        .iter()
        .map(|t| {
            let b = &t.b;
            json!({ "id": t.id, "x": t.x, "y": t.y, "w": t.w, "h": t.h, "depth": t.depth, "branch": t.branch, "dir": t.dir.as_str(),
                "side": t.left.map(|l| if l { "left" } else { "right" }), "moved": t.moved, "lines": b.lines, "lh": b.lh,
                "row": { "x": b.row.x, "y": b.row.y, "w": b.row.w, "h": b.row.h, "textX": b.row.text_x, "textW": b.row.text_w, "textH": b.row.text_h },
                "tagsX": b.tags_x, "tagsY": b.tags_y, "tags": b.tags.iter().map(|g| json!({ "text": g.text, "w": g.w })).collect::<Vec<_>>(),
                "img": b.img.map(|(w, h)| json!({ "w": w, "h": h })), "imgPos": b.img_pos.map(|(x, y)| json!({ "x": x, "y": y })),
                "style": style_json(&b.style) })
        })
        .collect();
    let edges: Vec<Value> = l
        .edges
        .iter()
        .map(|e| {
            let paths: Map<String, Value> = themes::LINE_STYLES
                .iter()
                .map(|s| {
                    let p = layout::edge_path(e, s.id);
                    (s.id.to_string(), Value::String(if p.filled { format!("F{}", p.d) } else { p.d }))
                })
                .collect();
            json!({ "from": l.items[e.from].id, "to": l.items[e.to].id, "orient": e.orient.as_str(), "p1": pt(e.p1), "p2": pt(e.p2),
                "color": e.color, "width": e.width, "depth": e.depth, "paths": paths })
        })
        .collect();
    let boundaries: Vec<Value> = l
        .boundaries
        .iter()
        .map(|b| json!({ "id": l.items[b.item].id, "x": b.x, "y": b.y, "w": b.w, "h": b.h, "label": b.label, "color": b.color, "depth": b.depth }))
        .collect();
    let extras: Vec<Value> = l
        .extras
        .iter()
        .map(|e| match *e {
            layout::Extra::Spine { x1, x2, y } => json!({ "type": "spine", "x1": x1, "x2": x2, "y": y }),
            layout::Extra::Axis { x1, x2, y } => json!({ "type": "axis", "x1": x1, "x2": x2, "y": y }),
        })
        .collect();
    let b = l.bounds;
    json!({ "list": list, "edges": edges, "boundaries": boundaries, "extras": extras,
        "bounds": { "minX": b.min_x, "minY": b.min_y, "maxX": b.max_x, "maxY": b.max_y, "w": b.w, "h": b.h } })
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

#[test]
fn layout_and_render() {
    let mut rep = Report::new("layout");
    for (i, c) in cases("layout").iter().enumerate() {
        let doc: Doc = model::normalize_value(c["doc"].clone()).expect("doc");
        let theme = themes::get_theme(&doc.settings.theme);
        let measure = layout::test_measure;
        let ctx = Ctx { theme, settings: &doc.settings, measure: &measure, focus_id: None, ignore_offset_id: None };
        let l = layout::layout(&doc, &ctx);
        let mut errs = vec![];
        diff("layout", &summary(&l), &c["layout"], &mut errs);
        let (sel, mat) = (strs(&c["selected"]), strs(&c["matches"]));
        let o = SceneOpts { theme, settings: &doc.settings, selected: &sel, matches: &mat, selected_rel: c["selectedRel"].as_str(), export: false };
        str_diff("scene", &render::scene(&l, &doc, &o), c["scene"].as_str().unwrap(), &mut errs);
        str_diff("svg", &render::export_svg(&l, &doc, theme, &doc.settings, false, None).svg, c["svg"].as_str().unwrap(), &mut errs);
        str_diff("svgTransparent", &render::export_svg(&l, &doc, theme, &doc.settings, true, Some(16.0)).svg, c["svgTransparent"].as_str().unwrap(), &mut errs);
        for key in ["focus", "ignore"] {
            if let Some(f) = c.get(key) {
                let id = f["id"].as_str().unwrap();
                let ctx = Ctx {
                    theme,
                    settings: &doc.settings,
                    measure: &measure,
                    focus_id: (key == "focus").then_some(id),
                    ignore_offset_id: (key == "ignore").then_some(id),
                };
                diff(key, &summary(&layout::layout(&doc, &ctx)), &f["layout"], &mut errs);
            }
        }
        rep.case(format!("case {i} (layout {}, theme {})", doc.settings.layout, doc.settings.theme), errs);
    }
    rep.finish();
}

#[test]
fn wrapping() {
    let mut rep = Report::new("wrap");
    for c in cases("wrap") {
        let f = FontSpec { size: c["size"].as_f64().unwrap(), weight: 400, italic: false, family: "x" };
        let out = layout::wrap_text(c["text"].as_str().unwrap(), &f, c["maxW"].as_f64().unwrap(), &layout::test_measure);
        let mut errs = vec![];
        diff("out", &json!(out), &c["out"], &mut errs);
        rep.case(format!("{:?} max {} size {}", c["text"], c["maxW"], c["size"]), errs);
    }
    rep.finish();
}

#[test]
fn styles() {
    let mut rep = Report::new("style");
    for c in cases("style") {
        let theme = themes::get_theme(c["theme"].as_str().unwrap());
        let mut s =
            model::Settings { theme: theme.id.into(), rainbow: c["rainbow"].as_bool(), font_scale: c["fontScale"].as_f64().unwrap(), ..Default::default() };
        s.shapes.root = "ellipse".into();
        s.shapes.main = "pill".into();
        s.shapes.sub = "underline".into();
        let mut node = Node::new("");
        if c["style"].as_object().is_some_and(|o| !o.is_empty()) {
            node.style = Some(serde_json::from_value(c["style"].clone()).unwrap());
        }
        let st =
            themes::resolve_style(&node, c["depth"].as_u64().unwrap() as usize, c["branch"].as_i64().unwrap(), theme, &s, c["floating"].as_bool().unwrap());
        let mut errs = vec![];
        diff("out", &style_json(&st), &c["out"], &mut errs);
        rep.case(format!("{c}"), errs);
    }
    rep.finish();
}

/// The document with ids replaced by their walk order (as `canon` in record.js).
fn canon(doc: &Doc) -> Value {
    let mut map = HashMap::new();
    for t in doc.trees() {
        model::walk(t, &mut |n, _| {
            let k = map.len();
            map.insert(n.id.clone(), format!("i{k}"));
        });
    }
    let v = serde_json::to_value(doc).unwrap();
    fn fix(n: &mut Value, map: &HashMap<String, String>) {
        let id = n["id"].as_str().unwrap().to_string();
        n["id"] = json!(map[&id]);
        if let Some(Value::Array(k)) = n.get_mut("children") {
            k.iter_mut().for_each(|c| fix(c, map));
        }
    }
    let mut root = v["root"].clone();
    fix(&mut root, &map);
    let floating: Vec<Value> = v["floating"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|mut f| {
            fix(&mut f, &map);
            f
        })
        .collect();
    let relations: Vec<Value> = v["relations"]
        .as_array()
        .unwrap()
        .iter()
        .cloned()
        .map(|mut r| {
            r["id"] = json!("r");
            for k in ["from", "to"] {
                let id = r[k].as_str().unwrap().to_string();
                r[k] = json!(map.get(&id));
            }
            r
        })
        .collect();
    json!({ "root": root, "floating": floating, "relations": relations, "settings": v["settings"] })
}

fn result_doc(r: Result<Doc, String>) -> Value {
    match r {
        Ok(d) => canon(&d),
        Err(e) => json!({ "error": e }),
    }
}

#[test]
fn text_formats() {
    let mut rep = Report::new("io");
    for (i, c) in cases("io").iter().enumerate() {
        let doc = model::normalize_value(c["doc"].clone()).expect("doc");
        let mut errs = vec![];
        str_diff("md", &io::to_markdown(&doc), c["md"].as_str().unwrap(), &mut errs);
        str_diff("opml", &io::to_opml(&doc), c["opml"].as_str().unwrap(), &mut errs);
        str_diff("txt", &io::to_text(&doc), c["txt"].as_str().unwrap(), &mut errs);
        str_diff("mm", &io::to_freemind(&doc), c["mm"].as_str().unwrap(), &mut errs);
        type Parse = fn(&str) -> Result<Doc, String>;
        let parsers: [(&str, Parse); 4] = [("md", io::from_markdown), ("opml", io::from_opml), ("txt", io::from_text), ("mm", io::from_freemind)];
        for (k, f) in parsers {
            diff(&format!("back.{k}"), &result_doc(f(c[k].as_str().unwrap())), &c["back"][k], &mut errs);
        }
        rep.case(format!("case {i}"), errs);
    }
    rep.finish();

    let mut rep = Report::new("import");
    for c in cases("import") {
        let r = io::import_text(c["name"].as_str().unwrap(), c["text"].as_str().unwrap());
        let mut errs = vec![];
        match (&r, c.get("error")) {
            (Err(_), Some(_)) => {} // fails on purpose in both; JSON parser messages differ by design
            _ => diff("out", &result_doc(r), &c["out"], &mut errs),
        }
        rep.case(format!("{} {:?}", c["name"], c["text"]), errs);
    }
    rep.finish();
}

#[test]
fn normalize() {
    let mut rep = Report::new("normalize");
    for c in cases("normalize") {
        let mut errs = vec![];
        match (model::normalize_value(c["doc"].clone()), c.get("error")) {
            (Err(e), Some(j)) => diff("error", &json!(e), j, &mut errs),
            (r, _) => diff("out", &result_doc(r), &c["out"], &mut errs),
        }
        rep.case(format!("{}", c["doc"]), errs);
    }
    rep.finish();
}

/// Path like `["r", 0, 1]` or `["f0", 2]` to a node id.
fn id_at(doc: &Doc, p: &Value) -> Option<String> {
    let p = p.as_array()?;
    let head = p[0].as_str()?;
    let mut n = if head == "r" { &doc.root } else { &doc.floating[head[1..].parse::<usize>().ok()?] };
    for i in &p[1..] {
        n = &n.children[i.as_u64()? as usize];
    }
    Some(n.id.clone())
}
fn path_of(doc: &Doc, id: &str) -> Value {
    match model::find(doc, id) {
        None => Value::Null,
        Some(loc) => {
            let mut v = vec![json!(match loc.tree {
                None => "r".to_string(),
                Some(k) => format!("f{k}"),
            })];
            v.extend(loc.path.iter().map(|i| json!(i)));
            Value::Array(v)
        }
    }
}

#[test]
fn operations() {
    let mut rep = Report::new("ops");
    for (i, c) in cases("ops").iter().enumerate() {
        let mut doc = model::normalize_value(c["doc"].clone()).expect("doc");
        let args = &c["args"];
        let a = id_at(&doc, &args["a"]).unwrap_or_default();
        let b = id_at(&doc, &args["b"]).unwrap_or_default();
        let text = args["text"].as_str().unwrap_or("");
        let index = args["index"].as_i64();
        let op = c["op"].as_str().unwrap();
        let result = match op {
            "addChild" => json!(model::add_child(&mut doc, &a, text, index).is_some()),
            "addSibling" => json!(model::add_sibling(&mut doc, &a, text, false).is_some()),
            "addSiblingBefore" => json!(model::add_sibling(&mut doc, &a, text, true).is_some()),
            "addParent" => json!(model::add_parent(&mut doc, &a, text).is_some()),
            "remove" => match model::remove(&mut doc, &[a.clone(), b.clone()]) {
                Some(nx) => path_of(&doc, &nx),
                None => Value::Null,
            },
            "move" => json!(model::move_node(&mut doc, &a, &b, index)),
            "detach" => json!(model::detach(&mut doc, &a, args["x"].as_f64().unwrap(), args["y"].as_f64().unwrap())),
            "reorderUp" => json!(model::reorder(&mut doc, &a, -1)),
            "reorderDown" => json!(model::reorder(&mut doc, &a, 1)),
            "indent" => json!(model::indent(&mut doc, &a)),
            "outdent" => json!(model::outdent(&mut doc, &a)),
            "duplicate" => json!(model::duplicate(&mut doc, &a).is_some()),
            "insertSubtree" => {
                let sub = model::get(&doc, &a).cloned().unwrap();
                json!(model::insert_subtree(&mut doc, &b, &sub, index).is_some())
            }
            "collapse" => json!(model::set_collapsed(&mut doc, &a, args["flag"].as_bool())),
            "collapseToLevel" => {
                model::collapse_to_level(&mut doc, args["n"].as_u64().unwrap() as usize);
                Value::Null
            }
            "marker" => {
                model::toggle_marker(model::get_mut(&mut doc, &a).unwrap(), args["marker"].as_str().unwrap());
                Value::Null
            }
            "relation" => json!(model::add_relation(&mut doc, &a, &b, text).is_some()),
            "search" => Value::Array(model::search(&doc, args["q"].as_str().unwrap()).iter().map(|id| path_of(&doc, id)).collect()),
            "stats" => {
                let s = model::stats(&doc);
                json!({ "nodes": s.nodes, "words": s.words, "tasks": s.tasks, "done": s.done })
            }
            _ => panic!("{op}"),
        };
        let mut errs = vec![];
        diff("result", &result, &c["result"], &mut errs);
        diff("after", &canon(&doc), &c["after"], &mut errs);
        rep.case(format!("case {i} {op} {args}"), errs);
    }
    rep.finish();
}

#[test]
fn numbers_print_like_js() {
    assert_eq!(js::n(12.25), "12.3");
}
