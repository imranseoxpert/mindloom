//! The `.mindloom` document and its tree operations (port of `model.js`).
//! The JSON shape is the same as Mindloom 1.x (ADR-004); unknown fields are kept.

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};

/// Write whole numbers without a fraction, like JavaScript's `JSON.stringify`.
fn ser_f64<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.fract() == 0.0 && v.abs() < 9e15 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}
fn ser_opt<S: Serializer>(v: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Some(x) => ser_f64(x, s),
        None => s.serialize_none(),
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NodeStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shape: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub align: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_dash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub font_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub border_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Image {
    pub src: String,
    #[serde(serialize_with = "ser_f64")]
    pub w: f64,
    #[serde(serialize_with = "ser_f64")]
    pub h: f64,
}

/// A hand-placed shift, kept only for the structure (`l`) it was made in.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Offset {
    #[serde(serialize_with = "ser_f64")]
    pub x: f64,
    #[serde(serialize_with = "ser_f64")]
    pub y: f64,
    pub l: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub children: Vec<Node>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapsed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<NodeStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<Image>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<Offset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub boundary_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub y: Option<f64>,
    /// Floating topic imported without coordinates: placed under the map on first open.
    #[serde(rename = "_place", skip_serializing_if = "Option::is_none")]
    pub place: Option<bool>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Node {
    pub fn new(text: impl Into<String>) -> Node {
        Node { id: uid(), text: text.into(), ..Default::default() }
    }
    pub fn is_collapsed(&self) -> bool {
        self.collapsed == Some(true)
    }
    pub fn has_boundary(&self) -> bool {
        self.boundary == Some(true)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Relation {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", serialize_with = "ser_opt")]
    pub bend: Option<f64>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Shapes {
    pub root: String,
    pub main: String,
    pub sub: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub layout: String,
    pub line_style: String,
    pub pattern: String,
    pub font: String,
    pub shapes: Shapes,
    pub rainbow: Option<bool>,
    #[serde(serialize_with = "ser_f64")]
    pub h_gap: f64,
    #[serde(serialize_with = "ser_f64")]
    pub v_gap: f64,
    #[serde(serialize_with = "ser_f64")]
    pub line_width: f64,
    #[serde(serialize_with = "ser_f64")]
    pub max_width: f64,
    #[serde(serialize_with = "ser_f64")]
    pub font_scale: f64,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "fresh".into(),
            layout: "mindmap".into(),
            line_style: "curve".into(),
            pattern: "dots".into(),
            font: "system".into(),
            shapes: Shapes { root: "rounded".into(), main: "rounded".into(), sub: "rounded".into(), extra: Map::new() },
            rainbow: None,
            h_gap: 56.0,
            v_gap: 14.0,
            line_width: 2.0,
            max_width: 260.0,
            font_scale: 1.0,
            extra: Map::new(),
        }
    }
}

fn default_settings_value() -> Value {
    serde_json::to_value(Settings::default()).unwrap_or(Value::Null)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Doc {
    pub app: String,
    pub version: u32,
    pub root: Node,
    #[serde(default)]
    pub floating: Vec<Node>,
    #[serde(default)]
    pub relations: Vec<Relation>,
    pub settings: Settings,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Doc {
    pub fn new(title: &str) -> Doc {
        Doc {
            app: "mindloom".into(),
            version: 1,
            root: Node::new(if title.is_empty() { "Central Topic" } else { title }),
            floating: vec![],
            relations: vec![],
            settings: Settings::default(),
            extra: Map::new(),
        }
    }
    pub fn trees(&self) -> impl Iterator<Item = &Node> {
        std::iter::once(&self.root).chain(self.floating.iter())
    }
}

// ---------- ids ----------

static COUNTER: AtomicU64 = AtomicU64::new(0);
static RNG: AtomicU64 = AtomicU64::new(0);

fn base36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut out = vec![];
    while n > 0 {
        out.push(D[(n % 36) as usize]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// A new unique id, in the same style as 1.x (`n` + time + random + counter).
pub fn uid() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let c = (COUNTER.fetch_add(1, Ordering::Relaxed) + 1) % 1_000_000;
    // xorshift seeded from the clock; ids only need to be unique, not unpredictable
    let mut x = RNG.load(Ordering::Relaxed) ^ now.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (c << 17);
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    RNG.store(x, Ordering::Relaxed);
    format!("n{}{:0>5}{}", base36(now), base36(x % 60_466_176), base36(c))
}

// ---------- normalizing loaded files (works on JSON like the original) ----------

fn js_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Null => 0.0,
        Value::String(s) => {
            let t = crate::js::trim(s);
            if t.is_empty() {
                0.0
            } else if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                u64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(f64::NAN)
            } else if t == "Infinity" || t == "+Infinity" {
                f64::INFINITY
            } else if t == "-Infinity" {
                f64::NEG_INFINITY
            } else {
                t.parse::<f64>().ok().filter(|_| !t.contains(|c: char| c.is_ascii_alphabetic() && c != 'e' && c != 'E')).unwrap_or(f64::NAN)
            }
        }
        Value::Array(a) => match a.len() {
            0 => 0.0,
            1 => js_number(&Value::String(js_string(&a[0]))),
            _ => f64::NAN,
        },
        Value::Object(_) => f64::NAN,
    }
}
fn js_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => crate::js::num(n.as_f64().unwrap_or(f64::NAN)),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_string(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn num_in(v: &Value, lo: f64, hi: f64) -> Option<f64> {
    let x = js_number(v);
    if x.is_finite() {
        Some(x.clamp(lo, hi))
    } else {
        None
    }
}
fn num_val(x: f64) -> Value {
    if x.fract() == 0.0 && x.abs() < 9e15 {
        Value::from(x as i64)
    } else {
        Value::from(x)
    }
}
fn is_color(s: &str) -> bool {
    let l = s.to_ascii_lowercase();
    if l == "transparent" {
        return true;
    }
    if let Some(h) = l.strip_prefix('#') {
        return (3..=8).contains(&h.len()) && h.bytes().all(|c| c.is_ascii_hexdigit());
    }
    let inner = l.strip_prefix("rgba(").or_else(|| l.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'));
    inner.is_some_and(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | '%') || crate::js::is_space(c)))
}
fn one_of(v: &Value, list: &[&str]) -> Option<String> {
    v.as_str().filter(|s| list.contains(s)).map(str::to_string)
}
fn ids<T>(items: &[T], f: impl Fn(&T) -> &'static str) -> Vec<&'static str> {
    items.iter().map(f).collect()
}

fn clean_node(n: &mut Map<String, Value>) {
    use crate::themes::{FONTS, SHAPES};
    match n.get_mut("style") {
        Some(Value::Object(st)) => {
            for k in ["fill", "color", "border", "lineColor"] {
                if let Some(v) = st.get(k) {
                    if !v.is_null() && !is_color(&js_string(v)) {
                        st.remove(k);
                    }
                }
            }
            for (k, lo, hi) in [("fontSize", 6.0, 200.0), ("width", 40.0, 2000.0), ("borderWidth", 0.0, 20.0)] {
                if let Some(v) = st.get(k).filter(|v| !v.is_null()) {
                    match num_in(v, lo, hi) {
                        Some(x) => {
                            st.insert(k.into(), num_val(x));
                        }
                        None => {
                            st.remove(k);
                        }
                    }
                }
            }
            let shapes = ids(SHAPES, |x| x.id);
            let fonts = ids(FONTS, |x| x.id);
            for (k, list) in
                [("shape", &shapes[..]), ("font", &fonts[..]), ("align", &["left", "center", "right"][..]), ("borderDash", &["dashed", "dotted"][..])]
            {
                if let Some(v) = st.get(k).filter(|v| !v.is_null()) {
                    match one_of(v, list) {
                        Some(x) => {
                            st.insert(k.into(), Value::String(x));
                        }
                        None => {
                            st.remove(k);
                        }
                    }
                }
            }
            for k in ["bold", "italic", "underline", "strike"] {
                if let Some(v) = st.get(k).filter(|v| !v.is_null() && !v.is_boolean()) {
                    let b = truthy(v);
                    st.insert(k.into(), Value::Bool(b));
                }
            }
            if st.is_empty() {
                n.remove("style");
            }
        }
        _ => {
            n.remove("style");
        }
    }
    if let Some(o) = n.get("offset").cloned() {
        if truthy(&o) {
            let x = num_in(o.get("x").unwrap_or(&Value::Null), -1e5, 1e5);
            let y = num_in(o.get("y").unwrap_or(&Value::Null), -1e5, 1e5);
            let l = o.get("l").and_then(Value::as_str);
            match (x, y, l) {
                (Some(x), Some(y), Some(l)) if o.get("x").is_some() && o.get("y").is_some() => {
                    n.insert("offset".into(), serde_json::json!({ "x": num_val(x), "y": num_val(y), "l": l }));
                }
                _ => {
                    n.remove("offset");
                }
            }
        }
    }
    if let Some(v) = n.get("icons").filter(|v| !v.is_null()).cloned() {
        let list: Vec<Value> = match v {
            Value::Array(a) => a.into_iter().filter(|m| m.as_str().is_some_and(|s| crate::js::len16(s) <= 24)).take(12).collect(),
            _ => vec![],
        };
        if list.is_empty() {
            n.remove("icons");
        } else {
            n.insert("icons".into(), Value::Array(list));
        }
    }
    if let Some(v) = n.get("tags").filter(|v| !v.is_null()).cloned() {
        let list: Vec<Value> = match v {
            Value::Array(a) => a.iter().map(js_string).filter(|s| !s.is_empty()).take(20).map(Value::String).collect(),
            _ => vec![],
        };
        if list.is_empty() {
            n.remove("tags");
        } else {
            n.insert("tags".into(), Value::Array(list));
        }
    }
    for k in ["note", "link", "boundaryLabel"] {
        if let Some(v) = n.get(k).filter(|v| !v.is_null()) {
            let s = js_string(v);
            if s.is_empty() {
                n.remove(k);
            } else {
                n.insert(k.into(), Value::String(s));
            }
        }
    }
    if let Some(im) = n.get("image").filter(|v| !v.is_null()).cloned() {
        let w = num_in(im.get("w").unwrap_or(&Value::Null), 8.0, 4000.0).filter(|_| im.get("w").is_some());
        let h = num_in(im.get("h").unwrap_or(&Value::Null), 8.0, 4000.0).filter(|_| im.get("h").is_some());
        match (im.get("src").and_then(Value::as_str), w, h) {
            (Some(src), Some(w), Some(h)) if src.starts_with("data:image/") => {
                n.insert("image".into(), serde_json::json!({ "src": src, "w": num_val(w), "h": num_val(h) }));
            }
            _ => {
                n.remove("image");
            }
        }
    }
    for (k, list) in [("side", &["left", "right"][..]), ("task", &["todo", "done"][..])] {
        if let Some(v) = n.get(k).filter(|v| !v.is_null()) {
            match one_of(v, list) {
                Some(x) => {
                    n.insert(k.into(), Value::String(x));
                }
                None => {
                    n.remove(k);
                }
            }
        }
    }
    for k in ["collapsed", "boundary"] {
        if let Some(v) = n.get(k).filter(|v| !v.is_null()) {
            if truthy(v) {
                n.insert(k.into(), Value::Bool(true));
            } else {
                n.remove(k);
            }
        }
    }
    for k in ["x", "y"] {
        if let Some(v) = n.get(k).filter(|v| !v.is_number() && !v.is_null()).cloned() {
            let x = js_number(&v);
            if x.is_finite() {
                n.insert(k.into(), num_val(x));
            } else {
                n.remove(k);
            }
        }
    }
}

fn clean_settings(s: &mut Map<String, Value>) {
    use crate::themes::{FONTS, LAYOUTS, LINE_STYLES, PATTERNS, SHAPES, THEMES};
    let d = Settings::default();
    let pick = |s: &mut Map<String, Value>, k: &str, list: Vec<&str>, dv: &str| {
        let v = s.get(k).and_then(|v| one_of(v, &list)).unwrap_or_else(|| dv.to_string());
        s.insert(k.into(), Value::String(v));
    };
    pick(s, "theme", ids(THEMES, |x| x.id), &d.theme);
    pick(s, "layout", ids(LAYOUTS, |x| x.id), &d.layout);
    pick(s, "lineStyle", ids(LINE_STYLES, |x| x.id), &d.line_style);
    pick(s, "pattern", ids(PATTERNS, |x| x.id), &d.pattern);
    pick(s, "font", ids(FONTS, |x| x.id), &d.font);
    let shapes = ids(SHAPES, |x| x.id);
    if let Some(Value::Object(sh)) = s.get_mut("shapes") {
        for (k, dv) in [("root", &d.shapes.root), ("main", &d.shapes.main), ("sub", &d.shapes.sub)] {
            let v = sh.get(k).and_then(|v| one_of(v, &shapes)).unwrap_or_else(|| dv.clone());
            sh.insert(k.into(), Value::String(v));
        }
    }
    let rb = match s.get("rainbow") {
        None | Some(Value::Null) => Value::Null,
        Some(v) => Value::Bool(truthy(v)),
    };
    s.insert("rainbow".into(), rb);
    for (k, lo, hi, dv) in [
        ("hGap", 10.0, 300.0, d.h_gap),
        ("vGap", 0.0, 120.0, d.v_gap),
        ("lineWidth", 0.5, 12.0, d.line_width),
        ("maxWidth", 60.0, 1200.0, d.max_width),
        ("fontScale", 0.4, 3.0, d.font_scale),
    ] {
        let v = num_in(s.get(k).unwrap_or(&Value::Null), lo, hi).filter(|_| s.get(k).is_some()).unwrap_or(dv);
        s.insert(k.into(), num_val(v));
    }
}

/// Port of `normalizeDoc`: fills in missing fields, repairs ids and drops malformed values,
/// so any file is safe to use. Fails only when there is no root.
pub fn normalize_value(mut doc: Value) -> Result<Doc, String> {
    let obj = doc.as_object_mut().filter(|o| o.get("root").is_some_and(truthy)).ok_or_else(|| "Not a Mindloom document".to_string())?;
    obj.insert("app".into(), "mindloom".into());
    obj.insert("version".into(), 1.into());
    for k in ["floating", "relations"] {
        if !obj.get(k).is_some_and(Value::is_array) {
            obj.insert(k.into(), Value::Array(vec![]));
        }
    }
    let mut settings = match default_settings_value() {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if let Some(Value::Object(given)) = obj.get("settings") {
        for (k, v) in given {
            settings.insert(k.clone(), v.clone());
        }
    }
    let mut shapes = match serde_json::to_value(Settings::default().shapes) {
        Ok(Value::Object(m)) => m,
        _ => Map::new(),
    };
    if let Some(Value::Object(sh)) = obj.get("settings").and_then(|s| s.get("shapes")) {
        for (k, v) in sh {
            shapes.insert(k.clone(), v.clone());
        }
    }
    settings.insert("shapes".into(), Value::Object(shapes));
    clean_settings(&mut settings);
    obj.insert("settings".into(), Value::Object(settings));

    let mut seen: HashSet<String> = HashSet::new();
    fn fix(n: &mut Value, seen: &mut HashSet<String>) -> Result<(), String> {
        let m = n.as_object_mut().ok_or_else(|| "Not a Mindloom document".to_string())?;
        let id = m.get("id").filter(|v| truthy(v)).map(js_string);
        let id = match id {
            Some(i) if !seen.contains(&i) => i,
            _ => uid(),
        };
        seen.insert(id.clone());
        m.insert("id".into(), Value::String(id));
        let text = match m.get("text") {
            None | Some(Value::Null) => String::new(),
            Some(v) => js_string(v),
        };
        m.insert("text".into(), Value::String(text));
        let kids: Vec<Value> = match m.remove("children") {
            Some(Value::Array(a)) => a.into_iter().filter(|c| c.is_object() || c.is_array()).filter(Value::is_object).collect(),
            _ => vec![],
        };
        m.insert("children".into(), Value::Array(kids));
        clean_node(m);
        if let Some(Value::Array(kids)) = m.get_mut("children") {
            for c in kids {
                fix(c, seen)?;
            }
        }
        Ok(())
    }
    fix(obj.get_mut("root").ok_or("Not a Mindloom document")?, &mut seen)?;
    if let Some(Value::Array(fl)) = obj.get_mut("floating") {
        fl.retain(Value::is_object);
        for f in fl.iter_mut() {
            fix(f, &mut seen)?;
            let m = f.as_object_mut().ok_or("Not a Mindloom document")?;
            let (x, y) = (m.get("x").cloned().unwrap_or(Value::Null), m.get("y").cloned().unwrap_or(Value::Null));
            if x.is_null() && y.is_null() {
                m.insert("_place".into(), Value::Bool(true));
            }
            for (k, v) in [("x", x), ("y", y)] {
                let n = js_number(&v);
                m.insert(k.into(), num_val(if n.is_nan() || n == 0.0 { 0.0 } else { n }));
            }
        }
    }
    if let Some(Value::Array(rel)) = obj.get_mut("relations") {
        rel.retain(|r| {
            let f = r.get("from").map(js_string);
            let t = r.get("to").map(js_string);
            r.is_object() && f.is_some_and(|f| seen.contains(&f)) && t.is_some_and(|t| seen.contains(&t))
        });
        for r in rel.iter_mut() {
            let m = r.as_object_mut().ok_or("Not a Mindloom document")?;
            for k in ["from", "to"] {
                let v = m.get(k).map(js_string).unwrap_or_default();
                m.insert(k.into(), Value::String(v));
            }
            if !m.get("id").is_some_and(truthy) {
                m.insert("id".into(), Value::String(uid()));
            } else if let Some(v) = m.get("id").map(js_string) {
                m.insert("id".into(), Value::String(v));
            }
            let label = match m.get("label") {
                None | Some(Value::Null) => String::new(),
                Some(v) => js_string(v),
            };
            m.insert("label".into(), Value::String(label));
            if let Some(c) = m.get("color").filter(|v| !v.is_null()) {
                if !is_color(&js_string(c)) {
                    m.remove("color");
                } else {
                    let s = js_string(c);
                    m.insert("color".into(), Value::String(s));
                }
            } else {
                m.remove("color");
            }
            if let Some(b) = m.get("bend").filter(|v| !v.is_null()) {
                match num_in(b, -2000.0, 2000.0) {
                    Some(x) => {
                        m.insert("bend".into(), num_val(x));
                    }
                    None => {
                        m.remove("bend");
                    }
                }
            } else {
                m.remove("bend");
            }
        }
    }
    serde_json::from_value(doc).map_err(|e| format!("Not a Mindloom document ({e})"))
}

/// Normalize an already typed document (repairs ids and values in place).
pub fn normalize(doc: Doc) -> Doc {
    let v = serde_json::to_value(&doc).unwrap_or(Value::Null);
    normalize_value(v).unwrap_or(doc)
}

// ---------- finding nodes ----------

/// Where a node lives: `tree` is `None` for the root tree, `Some(i)` for floating tree `i`;
/// `path` are child indices from that tree's root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loc {
    pub tree: Option<usize>,
    pub path: Vec<usize>,
}
impl Loc {
    pub fn depth(&self) -> usize {
        self.path.len()
    }
    pub fn floating(&self) -> bool {
        self.tree.is_some()
    }
    pub fn has_parent(&self) -> bool {
        !self.path.is_empty()
    }
    pub fn parent(&self) -> Option<Loc> {
        if self.path.is_empty() {
            return None;
        }
        Some(Loc { tree: self.tree, path: self.path[..self.path.len() - 1].to_vec() })
    }
    pub fn index(&self) -> usize {
        self.path.last().copied().or(self.tree).unwrap_or(0)
    }
}

fn find_in(n: &Node, id: &str, path: &mut Vec<usize>) -> bool {
    if n.id == id {
        return true;
    }
    for (i, c) in n.children.iter().enumerate() {
        path.push(i);
        if find_in(c, id, path) {
            return true;
        }
        path.pop();
    }
    false
}

pub fn find(doc: &Doc, id: &str) -> Option<Loc> {
    let mut path = vec![];
    if find_in(&doc.root, id, &mut path) {
        return Some(Loc { tree: None, path });
    }
    for (i, f) in doc.floating.iter().enumerate() {
        path.clear();
        if find_in(f, id, &mut path) {
            return Some(Loc { tree: Some(i), path });
        }
    }
    None
}
pub fn tree_root<'a>(doc: &'a Doc, loc: &Loc) -> &'a Node {
    match loc.tree {
        None => &doc.root,
        Some(i) => &doc.floating[i],
    }
}
pub fn at<'a>(doc: &'a Doc, loc: &Loc) -> &'a Node {
    let mut n = tree_root(doc, loc);
    for &i in &loc.path {
        n = &n.children[i];
    }
    n
}
pub fn at_mut<'a>(doc: &'a mut Doc, loc: &Loc) -> &'a mut Node {
    let mut n = match loc.tree {
        None => &mut doc.root,
        Some(i) => &mut doc.floating[i],
    };
    for &i in &loc.path {
        n = &mut n.children[i];
    }
    n
}
pub fn get<'a>(doc: &'a Doc, id: &str) -> Option<&'a Node> {
    find(doc, id).map(|l| at(doc, &l))
}
pub fn get_mut<'a>(doc: &'a mut Doc, id: &str) -> Option<&'a mut Node> {
    let l = find(doc, id)?;
    Some(at_mut(doc, &l))
}
/// Nodes from the tree root down to `id`.
pub fn path_to<'a>(doc: &'a Doc, id: &str) -> Vec<&'a Node> {
    let Some(loc) = find(doc, id) else { return vec![] };
    let mut n = tree_root(doc, &loc);
    let mut out = vec![n];
    for &i in &loc.path {
        n = &n.children[i];
        out.push(n);
    }
    out
}
pub fn is_ancestor(doc: &Doc, ancestor: &str, id: &str) -> bool {
    let p = path_to(doc, id);
    p.len() > 1 && p[..p.len() - 1].iter().any(|n| n.id == ancestor)
}
pub fn walk<'a>(n: &'a Node, f: &mut dyn FnMut(&'a Node, usize)) {
    fn rec<'a>(n: &'a Node, d: usize, f: &mut dyn FnMut(&'a Node, usize)) {
        f(n, d);
        for c in &n.children {
            rec(c, d + 1, f);
        }
    }
    rec(n, 0, f);
}
pub fn walk_mut(n: &mut Node, f: &mut dyn FnMut(&mut Node, usize)) {
    fn rec(n: &mut Node, d: usize, f: &mut dyn FnMut(&mut Node, usize)) {
        f(n, d);
        for c in &mut n.children {
            rec(c, d + 1, f);
        }
    }
    rec(n, 0, f);
}
pub fn walk_doc<'a>(doc: &'a Doc, f: &mut dyn FnMut(&'a Node, usize)) {
    for t in doc.trees() {
        walk(t, f);
    }
}
pub fn walk_doc_mut(doc: &mut Doc, f: &mut dyn FnMut(&mut Node, usize)) {
    walk_mut(&mut doc.root, f);
    for t in &mut doc.floating {
        walk_mut(t, f);
    }
}

fn siblings_mut<'a>(doc: &'a mut Doc, loc: &Loc) -> &'a mut Vec<Node> {
    match loc.parent() {
        Some(p) => &mut at_mut(doc, &p).children,
        None => &mut doc.floating,
    }
}

// ---------- operations ----------

/// Adds a child; returns its id.
pub fn add_child(doc: &mut Doc, parent: &str, text: &str, index: Option<i64>) -> Option<String> {
    let p = get_mut(doc, parent)?;
    let n = Node::new(text);
    let id = n.id.clone();
    match index {
        Some(i) if i >= 0 && (i as usize) <= p.children.len() => p.children.insert(i as usize, n),
        _ => p.children.push(n),
    }
    p.collapsed = Some(false);
    Some(id)
}

pub fn add_sibling(doc: &mut Doc, id: &str, text: &str, before: bool) -> Option<String> {
    let loc = find(doc, id)?;
    if !loc.has_parent() {
        if loc.floating() {
            let src = at(doc, &loc);
            let mut n = Node::new(text);
            n.x = Some(src.x.unwrap_or(0.0));
            n.y = Some(src.y.unwrap_or(0.0) + 80.0);
            let nid = n.id.clone();
            doc.floating.push(n);
            return Some(nid);
        }
        return add_child(doc, id, text, None);
    }
    let i = loc.index();
    let n = Node::new(text);
    let nid = n.id.clone();
    siblings_mut(doc, &loc).insert(if before { i } else { i + 1 }, n);
    Some(nid)
}

pub fn add_parent(doc: &mut Doc, id: &str, text: &str) -> Option<String> {
    let loc = find(doc, id)?;
    if !loc.has_parent() {
        return None;
    }
    let i = loc.index();
    let list = siblings_mut(doc, &loc);
    let mut node = list.remove(i);
    let mut n = Node::new(text);
    n.side = node.side.take();
    n.offset = node.offset.take();
    n.children.push(node);
    let nid = n.id.clone();
    list.insert(i, n);
    Some(nid)
}

fn collect_ids(n: &Node, out: &mut HashSet<String>) {
    walk(n, &mut |x, _| {
        out.insert(x.id.clone());
    });
}

/// Removes nodes (never the main root); returns a sensible node to select next.
pub fn remove(doc: &mut Doc, ids: &[String]) -> Option<String> {
    let mut next: Option<String> = None;
    let mut removed = HashSet::new();
    for id in ids {
        let Some(loc) = find(doc, id) else { continue };
        if !loc.has_parent() && !loc.floating() {
            continue;
        }
        let node = if loc.has_parent() {
            let i = loc.index();
            let parent_id = at(doc, &loc.parent().unwrap_or(Loc { tree: None, path: vec![] })).id.clone();
            let list = siblings_mut(doc, &loc);
            next = Some(list.get(i + 1).or_else(|| if i > 0 { list.get(i - 1) } else { None }).map(|n| n.id.clone()).unwrap_or(parent_id));
            list.remove(i)
        } else {
            next = Some(doc.root.id.clone());
            doc.floating.remove(loc.index())
        };
        collect_ids(&node, &mut removed);
    }
    if !removed.is_empty() {
        doc.relations.retain(|r| !removed.contains(&r.from) && !removed.contains(&r.to));
    }
    if next.as_ref().is_some_and(|n| removed.contains(n)) {
        next = Some(doc.root.id.clone());
    }
    next
}

/// Moves `id` under `new_parent` at `index` (end when out of range). Refuses cycles.
pub fn move_node(doc: &mut Doc, id: &str, new_parent: &str, index: Option<i64>) -> bool {
    if id == new_parent || id == doc.root.id || is_ancestor(doc, id, new_parent) {
        return false;
    }
    let (Some(loc), true) = (find(doc, id), get(doc, new_parent).is_some()) else { return false };
    let mut node = if loc.has_parent() {
        siblings_mut(doc, &loc).remove(loc.index())
    } else if loc.floating() {
        let mut n = doc.floating.remove(loc.index());
        n.x = None;
        n.y = None;
        n
    } else {
        return false;
    };
    node.offset = None;
    let Some(np) = get_mut(doc, new_parent) else { return false };
    match index {
        Some(i) if i >= 0 && (i as usize) <= np.children.len() => np.children.insert(i as usize, node),
        _ => np.children.push(node),
    }
    np.collapsed = Some(false);
    true
}

pub fn detach(doc: &mut Doc, id: &str, x: f64, y: f64) -> bool {
    let Some(loc) = find(doc, id) else { return false };
    if !loc.has_parent() {
        return false;
    }
    let mut node = siblings_mut(doc, &loc).remove(loc.index());
    node.x = Some(x);
    node.y = Some(y);
    node.side = None;
    node.offset = None;
    doc.floating.push(node);
    true
}

pub fn reorder(doc: &mut Doc, id: &str, delta: i64) -> bool {
    let Some(loc) = find(doc, id) else { return false };
    if !loc.has_parent() {
        return false;
    }
    let i = loc.index() as i64;
    let list = siblings_mut(doc, &loc);
    let j = i + delta;
    if j < 0 || j >= list.len() as i64 {
        return false;
    }
    let n = list.remove(i as usize);
    list.insert(j as usize, n);
    true
}

pub fn indent(doc: &mut Doc, id: &str) -> bool {
    let Some(loc) = find(doc, id) else { return false };
    if !loc.has_parent() || loc.index() == 0 {
        return false;
    }
    let i = loc.index();
    let list = siblings_mut(doc, &loc);
    let mut n = list.remove(i);
    n.offset = None;
    let prev = &mut list[i - 1];
    prev.children.push(n);
    prev.collapsed = Some(false);
    true
}

pub fn outdent(doc: &mut Doc, id: &str) -> bool {
    let Some(loc) = find(doc, id) else { return false };
    if loc.path.len() < 2 {
        return false;
    }
    let i = loc.index();
    let parent = loc.parent().unwrap_or(Loc { tree: None, path: vec![] });
    let pi = parent.index();
    let mut n = at_mut(doc, &parent).children.remove(i);
    n.offset = None;
    let grand = parent.parent().unwrap_or(Loc { tree: None, path: vec![] });
    at_mut(doc, &grand).children.insert(pi + 1, n);
    true
}

pub fn clone_with_new_ids(n: &Node) -> Node {
    let mut c = n.clone();
    walk_mut(&mut c, &mut |x, _| x.id = uid());
    c
}

pub fn duplicate(doc: &mut Doc, id: &str) -> Option<String> {
    let loc = find(doc, id)?;
    if !loc.has_parent() {
        return None;
    }
    let c = clone_with_new_ids(at(doc, &loc));
    let nid = c.id.clone();
    siblings_mut(doc, &loc).insert(loc.index() + 1, c);
    Some(nid)
}

pub fn insert_subtree(doc: &mut Doc, parent: &str, subtree: &Node, index: Option<i64>) -> Option<String> {
    let p = get_mut(doc, parent)?;
    let mut c = clone_with_new_ids(subtree);
    c.x = None;
    c.y = None;
    c.offset = None;
    c.side = None;
    let nid = c.id.clone();
    match index {
        None => p.children.push(c),
        Some(i) => {
            // Array.prototype.splice clamps the start
            let len = p.children.len() as i64;
            let at = if i < 0 { (len + i).max(0) } else { i.min(len) };
            p.children.insert(at as usize, c);
        }
    }
    p.collapsed = Some(false);
    Some(nid)
}

pub fn expand_to(doc: &mut Doc, id: &str) {
    let Some(loc) = find(doc, id) else { return };
    let mut l = Loc { tree: loc.tree, path: vec![] };
    for &i in &loc.path {
        at_mut(doc, &l).collapsed = Some(false);
        l.path.push(i);
    }
}

pub fn set_collapsed(doc: &mut Doc, id: &str, value: Option<bool>) -> bool {
    let Some(n) = get_mut(doc, id) else { return false };
    if n.children.is_empty() {
        return false;
    }
    n.collapsed = Some(value.unwrap_or(!n.is_collapsed()));
    true
}

pub fn collapse_to_level(doc: &mut Doc, level: usize) {
    walk_mut(&mut doc.root, &mut |n, d| {
        if !n.children.is_empty() {
            n.collapsed = Some(d >= level);
        }
    });
}

fn plain_text(n: &Node) -> String {
    [n.text.as_str(), n.note.as_deref().unwrap_or(""), &n.tags.as_ref().map(|t| t.join(" ")).unwrap_or_default(), n.link.as_deref().unwrap_or("")]
        .join(" ")
        .to_lowercase()
}

pub fn search(doc: &Doc, query: &str) -> Vec<String> {
    let q = crate::js::trim(query).to_lowercase();
    if q.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    walk_doc(doc, &mut |n, _| {
        if plain_text(n).contains(&q) {
            out.push(n.id.clone());
        }
    });
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub nodes: usize,
    pub words: usize,
    pub tasks: usize,
    pub done: usize,
}
pub fn stats(doc: &Doc) -> Stats {
    let mut s = Stats::default();
    walk_doc(doc, &mut |n, _| {
        s.nodes += 1;
        s.words += n.text.split(crate::js::is_space).filter(|w| !w.is_empty()).count();
        if let Some(t) = crate::js::some_str(&n.task) {
            s.tasks += 1;
            if t == "done" {
                s.done += 1;
            }
        }
    });
    s
}

pub fn toggle_marker(node: &mut Node, marker: &str) {
    let group = crate::themes::marker_group(marker);
    let icons = node.icons.take().unwrap_or_default();
    let had = icons.iter().any(|m| m == marker);
    let mut icons: Vec<String> = icons.into_iter().filter(|m| crate::themes::marker_group(m) != group).collect();
    if !had {
        icons.push(marker.to_string());
    }
    node.icons = if icons.is_empty() { None } else { Some(icons) };
}

pub fn add_relation(doc: &mut Doc, from: &str, to: &str, label: &str) -> Option<String> {
    if from == to || get(doc, from).is_none() || get(doc, to).is_none() {
        return None;
    }
    let id = uid();
    doc.relations.push(Relation { id: id.clone(), from: from.into(), to: to.into(), label: label.into(), ..Default::default() });
    Some(id)
}

/// Snapshot undo history, capped by steps and by total size (always keeps the last two steps).
pub struct History {
    limit: usize,
    max_bytes: usize,
    stack: Vec<String>,
    idx: usize,
}
impl History {
    pub fn new(limit: usize, max_bytes: usize) -> History {
        History { limit, max_bytes, stack: vec![], idx: 0 }
    }
    pub fn reset(&mut self, snapshot: String) {
        self.stack = vec![snapshot];
        self.idx = 0;
    }
    pub fn push(&mut self, snapshot: String) -> bool {
        if self.stack.get(self.idx) == Some(&snapshot) {
            return false;
        }
        self.stack.truncate(self.idx + 1);
        self.stack.push(snapshot);
        let mut total: usize = self.stack.iter().map(String::len).sum();
        while self.stack.len() > 2 && (self.stack.len() > self.limit || total > self.max_bytes) {
            total -= self.stack.remove(0).len();
        }
        self.idx = self.stack.len() - 1;
        true
    }
    pub fn can_undo(&self) -> bool {
        self.idx > 0
    }
    pub fn can_redo(&self) -> bool {
        self.idx + 1 < self.stack.len()
    }
    pub fn undo(&mut self) -> Option<&str> {
        if !self.can_undo() {
            return None;
        }
        self.idx -= 1;
        self.stack.get(self.idx).map(String::as_str)
    }
    pub fn redo(&mut self) -> Option<&str> {
        if !self.can_redo() {
            return None;
        }
        self.idx += 1;
        self.stack.get(self.idx).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_caps_but_keeps_last_two() {
        let mut h = History::new(300, 100);
        h.reset("a".repeat(60));
        h.push("b".repeat(60));
        h.push("c".repeat(60));
        assert!(h.can_undo());
        assert!(h.undo().is_some_and(|s| s.starts_with('b')));
        assert!(h.undo().is_none());
    }
    #[test]
    fn ids_are_unique() {
        let a: HashSet<String> = (0..10_000).map(|_| uid()).collect();
        assert_eq!(a.len(), 10_000);
    }
}
