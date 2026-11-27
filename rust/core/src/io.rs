//! Import and export of text formats (port of `io.js`): Markdown, OPML, indented text,
//! FreeMind, XMind, and the `.mindloom` JSON itself.

use crate::js::{self, is_space, slice16, trim, WS};
use crate::model::{self, Doc, Node};
use regex::{Captures, Regex};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::LazyLock;

/// `.` in a JavaScript regex (no `s` flag): anything but a line terminator.
const DOT: &str = r"[^\n\r\x{2028}\x{2029}]";

fn re(src: &str) -> Regex {
    // INFALLIBLE: every pattern is a constant checked by the unit tests.
    Regex::new(&src.replace("WS", WS).replace("DOT", DOT)).unwrap_or_else(|e| panic!("bad pattern {src}: {e}"))
}

// ---------- tiny XML parser ----------

fn decode_entities(s: &str) -> Result<String, String> {
    static ENT: LazyLock<Regex> = LazyLock::new(|| re(r"&(#[xX][0-9a-fA-F]+|#[0-9]+|[A-Za-z0-9_]+);"));
    let mut err = None;
    let out = ENT.replace_all(s, |c: &Captures| {
        let (m, e) = (&c[0], &c[1]);
        if let Some(num) = e.strip_prefix('#') {
            let (digits, radix) = match num.strip_prefix(['x', 'X']) {
                Some(h) => (h, 16.0),
                None => (num, 10.0),
            };
            let v = digits.chars().fold(0f64, |a, ch| a * radix + ch.to_digit(16).unwrap_or(0) as f64);
            if !v.is_finite() {
                return m.to_string();
            }
            if v > 1_114_111.0 {
                err.get_or_insert_with(|| format!("Invalid code point {}", js::num(v)));
                return m.to_string();
            }
            // a lone surrogate cannot be held in a Rust string
            return char::from_u32(v as u32).unwrap_or('\u{FFFD}').to_string();
        }
        match e.to_ascii_lowercase().as_str() {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            "nbsp" => "\u{A0}",
            _ => m,
        }
        .to_string()
    });
    match err {
        Some(e) => Err(e),
        None => Ok(out.into_owned()),
    }
}

pub fn xml_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\n', "&#10;")
}

#[derive(Debug, Default)]
pub struct XEl {
    pub name: String,
    pub attrs: HashMap<String, String>,
    pub children: Vec<usize>,
    pub text: String,
}

/// A parsed XML document: element 0 is the synthetic `#root`.
#[derive(Debug)]
pub struct Xml {
    pub els: Vec<XEl>,
}

impl Xml {
    pub fn kids<'a>(&'a self, el: usize, name: &'a str) -> impl Iterator<Item = usize> + 'a {
        self.els[el].children.iter().copied().filter(move |&c| self.els[c].name.eq_ignore_ascii_case(name))
    }
    pub fn first(&self, el: usize, name: &str) -> Option<usize> {
        self.kids(el, name).next()
    }
    pub fn deep_first(&self, el: usize, name: &str) -> Option<usize> {
        for &c in &self.els[el].children {
            if self.els[c].name.eq_ignore_ascii_case(name) {
                return Some(c);
            }
            if let Some(d) = self.deep_first(c, name) {
                return Some(d);
            }
        }
        None
    }
    pub fn attr(&self, el: usize, k: &str) -> Option<&str> {
        self.els[el].attrs.get(k).map(String::as_str)
    }
    fn flat_text(&self, el: usize) -> String {
        let e = &self.els[el];
        let mut s = e.text.clone();
        for &c in &e.children {
            s += &self.flat_text(c);
            if ["p", "br", "div", "li"].iter().any(|t| self.els[c].name.eq_ignore_ascii_case(t)) {
                s.push('\n');
            }
        }
        s
    }
}

pub fn parse_xml(src: &str) -> Result<Xml, String> {
    static TOK: LazyLock<Regex> = LazyLock::new(|| {
        re(
            r#"<!\[CDATA\[((?s:.)*?)\]\]>|<!--(?s:.)*?-->|<\?(?s:.)*?\?>|<!DOCTYPE[^>]*>|</WS*([A-Za-z0-9_:.-]+)WS*>|<([A-Za-z0-9_:.-]+)((?:WS+[A-Za-z0-9_:.-]+WS*=WS*(?:"[^"]*"|'[^']*'))*)WS*(/?)>|([^<]+)"#,
        )
    });
    static ATTR: LazyLock<Regex> = LazyLock::new(|| re(r#"([A-Za-z0-9_:.-]+)WS*=WS*("([^"]*)"|'([^']*)')"#));
    let mut x = Xml { els: vec![XEl { name: "#root".into(), ..Default::default() }] };
    let mut stack = vec![0usize];
    for m in TOK.captures_iter(src) {
        let top = *stack.last().unwrap_or(&0);
        if let Some(c) = m.get(1) {
            x.els[top].text += c.as_str();
        } else if m.get(2).is_some() {
            if stack.len() > 1 {
                stack.pop();
            }
        } else if let Some(name) = m.get(3) {
            let mut el = XEl { name: name.as_str().into(), ..Default::default() };
            for a in ATTR.captures_iter(m.get(4).map_or("", |v| v.as_str())) {
                let v = a.get(3).or(a.get(4)).map_or("", |v| v.as_str());
                el.attrs.insert(a[1].to_string(), decode_entities(v)?);
            }
            let id = x.els.len();
            x.els.push(el);
            x.els[top].children.push(id);
            if m.get(5).is_none_or(|v| v.as_str().is_empty()) {
                stack.push(id);
            }
        } else if let Some(t) = m.get(6) {
            let t = decode_entities(t.as_str())?;
            x.els[top].text += &t;
        }
    }
    Ok(x)
}

// ---------- building documents ----------

/// Several top-level trees: flagged ones become floating topics; more than one unflagged gets a wrapper root.
fn doc_from_roots(roots: Vec<Node>, floating: &[bool], title: Option<&str>) -> Doc {
    let mut main = vec![];
    let mut floats = vec![];
    for (r, f) in roots.into_iter().zip(floating.iter().copied().chain(std::iter::repeat(false))) {
        if f {
            floats.push(r);
        } else {
            main.push(r);
        }
    }
    if main.is_empty() && !floats.is_empty() {
        main.push(floats.remove(0));
    }
    let root = if main.len() > 1 {
        let mut w = Node::new(title.filter(|t| !t.is_empty()).unwrap_or("Imported"));
        w.children = main;
        w
    } else {
        main.pop().unwrap_or_else(|| Node::new(""))
    };
    for f in &mut floats {
        f.place = Some(true);
    }
    doc_from_tree(root, floats)
}

fn doc_from_tree(root: Node, floating: Vec<Node>) -> Doc {
    let mut doc = Doc::new("");
    doc.root = root;
    doc.floating = floating;
    model::normalize(doc)
}

/// Nodes whose children are filled in later by index (the parsers keep a stack of open nodes).
#[derive(Default)]
struct Arena {
    nodes: Vec<Node>,
    kids: Vec<Vec<usize>>,
}
impl Arena {
    fn add(&mut self, n: Node) -> usize {
        self.nodes.push(n);
        self.kids.push(vec![]);
        self.nodes.len() - 1
    }
    fn build(&mut self, i: usize) -> Node {
        let mut n = std::mem::take(&mut self.nodes[i]);
        n.children = std::mem::take(&mut self.kids[i]).into_iter().map(|k| self.build(k)).collect();
        n
    }
}

fn lines_of(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n").replace('\r', "\n").split('\n').map(str::to_string).collect()
}

fn indent_of(line: &str) -> usize {
    line.chars().take_while(|c| *c == '\t' || *c == ' ').map(|c| if c == '\t' { 4 } else { 1 }).sum()
}

fn take_task(t: &str) -> (Option<&'static str>, &str) {
    static TASK: LazyLock<Regex> = LazyLock::new(|| re(r"^\[([ xX])\]WS+"));
    match TASK.captures(t) {
        Some(c) => (Some(if &c[1] == " " { "todo" } else { "done" }), &t[c[0].len()..]),
        None => (None, t),
    }
}

// ---------- indented plain text ----------

/// Parse indented lines into root-level nodes and, for each root, whether a blank line came before it.
/// Bullets (-, *, +, •, 1.) and task boxes are recognised.
pub fn parse_indented(text: &str) -> (Vec<Node>, Vec<bool>) {
    static BULLET: LazyLock<Regex> = LazyLock::new(|| re(r"^(?:[-*+•]|[0-9]+[.)])WS+"));
    let mut a = Arena::default();
    let (mut roots, mut gaps) = (vec![], vec![]);
    let mut stack: Vec<(usize, usize)> = vec![];
    let mut gap = false;
    for raw in lines_of(text) {
        if trim(&raw).is_empty() {
            gap = true;
            continue;
        }
        let ind = indent_of(&raw);
        let t = BULLET.replace(trim(&raw), "").into_owned();
        let (task, t) = take_task(&t);
        let mut node = Node::new(t);
        node.task = task.map(Into::into);
        let id = a.add(node);
        while stack.last().is_some_and(|s| s.0 >= ind) {
            stack.pop();
        }
        match stack.last() {
            Some(&(_, p)) => a.kids[p].push(id),
            None => {
                roots.push(id);
                gaps.push(gap);
            }
        }
        stack.push((ind, id));
        gap = false;
    }
    (roots.into_iter().map(|r| a.build(r)).collect(), gaps)
}

pub fn from_text(text: &str) -> Result<Doc, String> {
    let (roots, gaps) = parse_indented(text);
    if roots.is_empty() {
        return Err("Nothing to import".into());
    }
    // a first tree followed by blank-line separated trees = a map plus its floating topics
    let floats = roots.len() > 1 && !roots[0].children.is_empty() && gaps[1..].iter().all(|g| *g);
    let flags: Vec<bool> = (0..roots.len()).map(|i| floats && i > 0).collect();
    Ok(doc_from_roots(roots, &flags, None))
}

pub fn to_text(doc: &Doc) -> String {
    let trees: Vec<String> = doc
        .trees()
        .map(|t| {
            let mut out = vec![];
            model::walk(t, &mut |n, d| out.push("\t".repeat(d) + &n.text.replace('\n', " ")));
            out.join("\n")
        })
        .collect();
    trees.join("\n\n") + "\n"
}

// ---------- Markdown ----------

pub fn to_markdown(doc: &Doc) -> String {
    let mut out: Vec<String> = vec![];
    for (i, tree) in doc.trees().enumerate() {
        if i > 0 {
            out.push("<!-- floating -->".into());
        }
        model::walk(tree, &mut |n, d| {
            let text = n.text.replace('\n', " ");
            let task = match js::some_str(&n.task) {
                Some("done") => "[x] ",
                Some(_) => "[ ] ",
                None => "",
            };
            let link = js::some_str(&n.link).map(|l| format!(" ([link]({l}))")).unwrap_or_default();
            match d {
                0 => out.extend([format!("# {text}{link}"), String::new()]),
                1 => out.extend([format!("## {task}{text}{link}"), String::new()]),
                _ => out.push(format!("{}- {task}{text}{link}", "  ".repeat(d - 2))),
            }
            if let Some(note) = js::some_str(&n.note) {
                let pad = if d >= 2 { "  ".repeat(d - 1) } else { String::new() };
                out.extend(note.split('\n').map(|l| format!("{pad}> {l}")));
                if d < 2 {
                    out.push(String::new());
                }
            }
        });
        out.push(String::new());
    }
    static GAPS: LazyLock<Regex> = LazyLock::new(|| re(r"\n{3,}"));
    let joined = out.join("\n");
    trim(&GAPS.replace_all(&joined, "\n\n")).to_string() + "\n"
}

struct Md {
    a: Arena,
    roots: Vec<usize>,
    stack: Vec<(usize, usize)>, // (level, node)
    last: Option<usize>,
    float_next: bool,
    floating: Vec<usize>,
}
impl Md {
    fn attach(&mut self, level: usize, node: usize) {
        while self.stack.last().is_some_and(|s| s.0 >= level) {
            self.stack.pop();
        }
        if self.float_next {
            self.stack.clear();
            self.floating.push(node);
            self.float_next = false;
        }
        match self.stack.last() {
            Some(&(_, p)) => self.a.kids[p].push(node),
            None => self.roots.push(node),
        }
        self.stack.push((level, node));
        self.last = Some(node);
    }
    fn note(&mut self, line: &str) {
        let Some(n) = self.last else { return };
        let node = &mut self.a.nodes[n];
        node.note = Some(match js::some_str(&node.note) {
            Some(prev) => format!("{prev}\n{line}"),
            None => line.to_string(),
        });
    }
}

pub fn from_markdown(text: &str) -> Result<Doc, String> {
    static FENCE: LazyLock<Regex> = LazyLock::new(|| re(r"^WS*```"));
    static FLOAT: LazyLock<Regex> = LazyLock::new(|| re(r"^WS*<!--WS*floatingWS*-->WS*$"));
    static COMMENT: LazyLock<Regex> = LazyLock::new(|| re(r"^WS*<!--DOT*-->WS*$"));
    static HEAD: LazyLock<Regex> = LazyLock::new(|| re(r"^(#{1,6})WS+(DOT*)$"));
    static ITEM: LazyLock<Regex> = LazyLock::new(|| re(r"^(WS*)(?:[-*+]|[0-9]+[.)])WS+(DOT*)$"));
    static QUOTE: LazyLock<Regex> = LazyLock::new(|| re(r"^WS*>WS?(DOT*)$"));
    static RULE: LazyLock<Regex> = LazyLock::new(|| re(r"^WS*(---|\*\*\*|___)WS*$"));

    let mut st = Md { a: Arena::default(), roots: vec![], stack: vec![], last: None, float_next: false, floating: vec![] };
    let (mut base, mut in_fence) = (0usize, false);
    for raw in lines_of(text) {
        if FENCE.is_match(&raw) {
            in_fence = !in_fence;
            continue;
        }
        if in_fence || trim(&raw).is_empty() {
            continue;
        }
        if FLOAT.is_match(&raw) {
            st.float_next = true;
            continue;
        }
        if COMMENT.is_match(&raw) {
            continue;
        }
        if let Some(m) = HEAD.captures(&raw) {
            let level = m[1].len() * 100;
            base = level;
            let n = mk(&mut st.a, &m[2]);
            st.attach(level, n);
        } else if let Some(m) = ITEM.captures(&raw) {
            let level = base + 1 + indent_of(&m[1]);
            let n = mk(&mut st.a, &m[2]);
            st.attach(level, n);
        } else if let (Some(m), true) = (QUOTE.captures(&raw), st.last.is_some()) {
            st.note(&m[1]);
        } else if st.last.is_some() && !RULE.is_match(&raw) {
            st.note(trim(&raw));
        }
    }
    if st.roots.is_empty() {
        return Err("No headings or list items found".into());
    }
    let flags: Vec<bool> = st.roots.iter().map(|r| st.floating.contains(r)).collect();
    let nodes = st.roots.clone().into_iter().map(|r| st.a.build(r)).collect();
    Ok(doc_from_roots(nodes, &flags, None))
}

fn mk(a: &mut Arena, s: &str) -> usize {
    static LINK: LazyLock<Regex> = LazyLock::new(|| re(r"\[[^\]]*\]\((https?:[^)]+)\)"));
    static CLEAN: LazyLock<[Regex; 4]> = LazyLock::new(|| [re(r"\*\*(DOT+?)\*\*"), re(r"__(DOT+?)__"), re(r"`([^`]+)`"), re(r"\[([^\]]+)\]\(([^)]+)\)")]);
    let (task, s) = take_task(s);
    let link = LINK.captures(s).map(|c| c[1].to_string());
    let mut t = s.to_string();
    for r in CLEAN.iter() {
        t = r.replace_all(&t, "${1}").into_owned();
    }
    let mut n = Node::new(trim(&t));
    n.task = task.map(Into::into);
    n.link = link;
    a.add(n)
}

// ---------- OPML ----------

pub fn to_opml(doc: &Doc) -> String {
    fn rec(n: &Node, d: usize, fl: bool) -> String {
        let pad = "  ".repeat(d + 2);
        let note = js::some_str(&n.note).map(|v| format!(r#" _note="{}""#, xml_esc(v))).unwrap_or_default() + if fl { r#" _floating="true""# } else { "" };
        if n.children.is_empty() {
            return format!("{pad}<outline text=\"{}\"{note}/>\n", xml_esc(&n.text));
        }
        let kids: String = n.children.iter().map(|c| rec(c, d + 1, false)).collect();
        format!("{pad}<outline text=\"{}\"{note}>\n{kids}{pad}</outline>\n", xml_esc(&n.text))
    }
    let body: String = doc.trees().enumerate().map(|(i, t)| rec(t, 0, i > 0)).collect();
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<opml version=\"2.0\">\n  <head><title>{}</title></head>\n  <body>\n{body}  </body>\n</opml>\n",
        xml_esc(&doc.root.text)
    )
}

pub fn from_opml(text: &str) -> Result<Doc, String> {
    let x = parse_xml(text)?;
    let body = x.deep_first(0, "body").ok_or("Not an OPML file")?;
    fn conv(x: &Xml, el: usize) -> Node {
        let t = x.attr(el, "text").filter(|v| !v.is_empty()).or(x.attr(el, "title").filter(|v| !v.is_empty())).unwrap_or("");
        let mut n = Node::new(t);
        n.note = x.attr(el, "_note").filter(|v| !v.is_empty()).map(Into::into);
        n.children = x.kids(el, "outline").map(|c| conv(x, c)).collect();
        n
    }
    let els: Vec<usize> = x.kids(body, "outline").collect();
    if els.is_empty() {
        return Err("OPML has no outlines".into());
    }
    let roots = els.iter().map(|&e| conv(&x, e)).collect();
    let flags: Vec<bool> = els.iter().map(|&e| x.attr(e, "_floating") == Some("true")).collect();
    let title = x.deep_first(0, "title").map(|h| trim(&x.els[h].text).to_string());
    Ok(doc_from_roots(roots, &flags, title.as_deref()))
}

// ---------- FreeMind / Freeplane (.mm) ----------

pub fn to_freemind(doc: &Doc) -> String {
    fn rec(doc: &Doc, n: &Node, d: usize, side: &str, free: bool) -> String {
        let pad = "  ".repeat(d + 1);
        let pos = if d == 1 { format!(r#" POSITION="{side}""#) } else { String::new() };
        let link = js::some_str(&n.link).map(|l| format!(r#" LINK="{}""#, xml_esc(l))).unwrap_or_default();
        let folded = if n.is_collapsed() { r#" FOLDED="true""# } else { "" };
        let head = format!("{pad}<node TEXT=\"{}\"{pos}{link}{folded}", xml_esc(&n.text));
        let note = js::some_str(&n.note);
        if n.children.is_empty() && note.is_none() && !free && !(d == 0 && !doc.floating.is_empty()) {
            return head + "/>\n";
        }
        let note =
            note.map(|v| format!("{pad}  <richcontent TYPE=\"NOTE\"><html><body><p>{}</p></body></html></richcontent>\n", xml_esc(v))).unwrap_or_default();
        let hook = if free { format!("{pad}  <hook NAME=\"FreeNode\"/>\n") } else { String::new() };
        let kids: String = n
            .children
            .iter()
            .enumerate()
            .map(|(i, c)| rec(doc, c, d + 1, js::some_str(&c.side).unwrap_or(if i % 2 == 1 { "left" } else { "right" }), false))
            .collect();
        let floats: String = if d == 0 { doc.floating.iter().map(|f| rec(doc, f, 1, "right", true)).collect() } else { String::new() };
        format!("{head}>\n{hook}{note}{kids}{floats}{pad}</node>\n")
    }
    format!("<map version=\"1.0.1\">\n{}</map>\n", rec(doc, &doc.root, 0, "right", false))
}

pub fn from_freemind(text: &str) -> Result<Doc, String> {
    let mut x = parse_xml(text)?;
    let top = match x.deep_first(0, "map") {
        Some(m) => x.first(m, "node"),
        None => x.deep_first(0, "node"),
    }
    .ok_or("Not a FreeMind file")?;
    fn rich(x: &Xml, el: usize, ty: &str) -> Option<usize> {
        x.kids(el, "richcontent").find(|&r| x.attr(r, "TYPE") == Some(ty))
    }
    fn conv(x: &Xml, el: usize) -> Node {
        let t = match x.attr(el, "TEXT") {
            Some(t) => t.to_string(),
            None => rich(x, el, "NODE").map(|r| x.flat_text(r)).unwrap_or_default(),
        };
        let mut n = Node::new(t);
        n.link = x.attr(el, "LINK").filter(|v| !v.is_empty()).map(Into::into);
        if x.attr(el, "FOLDED") == Some("true") {
            n.collapsed = Some(true);
        }
        n.note = rich(x, el, "NOTE").map(|r| trim(&x.flat_text(r)).to_string());
        if x.attr(el, "POSITION") == Some("left") {
            n.side = Some("left".into());
        }
        n.children = x.kids(el, "node").map(|c| conv(x, c)).collect();
        n
    }
    let free_els: Vec<usize> = x.kids(top, "node").filter(|&c| x.kids(c, "hook").any(|h| x.attr(h, "NAME") == Some("FreeNode"))).collect();
    x.els[top].children.retain(|c| !free_els.contains(c));
    let floating = free_els
        .iter()
        .map(|&e| {
            let mut f = conv(&x, e);
            f.side = None;
            f.place = Some(true);
            f
        })
        .collect();
    Ok(doc_from_tree(conv(&x, top), floating))
}

// ---------- XMind (.xmind zip: content.json for XMind Zen/2020+, content.xml for XMind 8) ----------

pub struct Zip<'a> {
    files: Vec<(String, u16, &'a [u8])>,
}

impl<'a> Zip<'a> {
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|f| f.0.as_str())
    }
    /// A file's text, or `None` when the archive has no such file.
    pub fn read(&self, name: &str) -> Result<Option<String>, String> {
        let Some(&(_, method, data)) = self.files.iter().rev().find(|f| f.0 == name) else { return Ok(None) };
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(data).map_err(|_| "The compressed data is corrupt".to_string())?,
            _ => return Err("Unsupported zip compression".into()),
        };
        let s = String::from_utf8_lossy(&bytes);
        Ok(Some(s.strip_prefix('\u{FEFF}').unwrap_or(&s).to_string()))
    }
}

pub fn unzip(u8s: &[u8]) -> Result<Zip<'_>, String> {
    let bad = || "Not a zip archive".to_string();
    let u16at = |p: usize| u8s.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]])).ok_or_else(bad);
    let u32at = |p: usize| u8s.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])).ok_or_else(bad);
    let len = u8s.len();
    let mut eocd = None;
    if len >= 22 {
        let lo = len.saturating_sub(65557);
        for i in (lo..=len - 22).rev() {
            if u32at(i)? == 0x0605_4b50 {
                eocd = Some(i);
                break;
            }
        }
    }
    let eocd = eocd.ok_or_else(bad)?;
    let count = u16at(eocd + 10)?;
    let mut p = u32at(eocd + 16)? as usize;
    let mut files = vec![];
    for _ in 0..count {
        if u32at(p)? != 0x0201_4b50 {
            break;
        }
        let method = u16at(p + 10)?;
        let csize = u32at(p + 20)? as usize;
        let (nlen, elen, clen) = (u16at(p + 28)? as usize, u16at(p + 30)? as usize, u16at(p + 32)? as usize);
        let off = u32at(p + 42)? as usize;
        let name = String::from_utf8_lossy(u8s.get(p + 46..p + 46 + nlen).ok_or_else(bad)?).into_owned();
        let (lnlen, lelen) = (u16at(off + 26)? as usize, u16at(off + 28)? as usize);
        let start = off + 30 + lnlen + lelen;
        let data = u8s.get(start..(start + csize).min(len)).unwrap_or(&[]);
        files.push((name, method, data));
        p += 46 + nlen + elen + clen;
    }
    Ok(Zip { files })
}

fn js_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => js::num(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Array(a) => a.iter().map(|x| if x.is_null() { String::new() } else { js_text(x) }).collect::<Vec<_>>().join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}
fn truthy(v: Option<&Value>) -> Option<&Value> {
    v.filter(|v| match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    })
}

pub fn from_xmind(bytes: &[u8]) -> Result<Doc, String> {
    let z = unzip(bytes)?;
    if let Some(json) = z.read("content.json")? {
        let sheets: Value = serde_json::from_str(&json).map_err(|e| format!("This .xmind file is damaged ({e})"))?;
        let sheet = match &sheets {
            Value::Array(a) => a.first().cloned().unwrap_or(Value::Null),
            v => v.clone(),
        };
        static TAGS: LazyLock<Regex> = LazyLock::new(|| re(r"<[^>]+>"));
        fn conv(t: &Value, detached: &mut Vec<Node>) -> Node {
            let mut n = Node::new(truthy(t.get("title")).map(js_text).unwrap_or_default());
            let notes = truthy(t.get("notes")).and_then(|ns| truthy(ns.get("plain")).or(truthy(ns.get("realHTML"))));
            if let Some(c) = notes.and_then(|ns| truthy(ns.get("content"))) {
                n.note = Some(TAGS.replace_all(&js_text(c), "").into_owned());
            }
            if let Some(Value::Array(l)) = truthy(t.get("labels")) {
                if !l.is_empty() {
                    n.tags = Some(l.iter().map(js_text).collect());
                }
            }
            if let Some(Value::String(h)) = t.get("href") {
                if h.starts_with("http:") || h.starts_with("https:") {
                    n.link = Some(h.clone());
                }
            }
            if t.get("branch").and_then(Value::as_str) == Some("folded") {
                n.collapsed = Some(true);
            }
            let list = |k: &str| t.get("children").and_then(|c| c.get(k)).and_then(Value::as_array).cloned().unwrap_or_default();
            n.children = list("attached").iter().map(|c| conv(c, &mut vec![])).collect();
            for d in list("detached") {
                let mut f = conv(&d, &mut vec![]);
                f.place = Some(true);
                detached.push(f);
            }
            n
        }
        let top = sheet.get("rootTopic").filter(|v| v.is_object()).ok_or("This .xmind file has no topics")?;
        let mut floating = vec![];
        let root = conv(top, &mut floating);
        return Ok(doc_from_tree(root, floating));
    }
    let xml = z.read("content.xml")?.ok_or("This .xmind file has no content")?;
    let x = parse_xml(&xml)?;
    let top = match x.deep_first(0, "sheet") {
        Some(s) => x.first(s, "topic"),
        None => x.deep_first(0, "topic"),
    }
    .ok_or("This .xmind file has no topics")?;
    fn conv(x: &Xml, el: usize) -> Node {
        let mut n = Node::new(x.first(el, "title").map(|t| x.els[t].text.clone()).unwrap_or_default());
        if let Some(plain) = x.first(el, "notes").and_then(|ns| x.first(ns, "plain")) {
            n.note = Some(trim(&x.els[plain].text).to_string());
        }
        if x.attr(el, "branch") == Some("folded") {
            n.collapsed = Some(true);
        }
        if let Some(ch) = x.first(el, "children") {
            let att = x.kids(ch, "topics").find(|&t| x.attr(t, "type") == Some("attached")).or_else(|| x.first(ch, "topics"));
            if let Some(att) = att {
                n.children = x.kids(att, "topic").map(|c| conv(x, c)).collect();
            }
        }
        n
    }
    Ok(doc_from_tree(conv(&x, top), vec![]))
}

// ---------- detection ----------

/// `/^#\s/m`: a line starting with `#` and a space.
fn has_heading(s: &str) -> bool {
    let mut at_start = true;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if at_start && c == '#' && it.peek().is_some_and(|n| is_space(*n)) {
            return true;
        }
        at_start = matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}');
    }
    false
}

/// Open any supported text format, chosen by file extension or content.
pub fn import_text(name: &str, text: &str) -> Result<Doc, String> {
    static OPML: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)<opml(?:WS|>)"));
    static MAP: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^<map(?:WS|>)"));
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    let trimmed = trim(text);
    if ext == "mindloom" || ext == "json" || trimmed.starts_with('{') {
        let d: Value = serde_json::from_str(text).map_err(|e| format!("This file is not valid JSON ({e})"))?;
        let inner = d.get("doc").filter(|x| truthy(Some(x)).is_some() && truthy(x.get("root")).is_some()).cloned();
        return model::normalize_value(inner.unwrap_or(d));
    }
    if ext == "opml" || OPML.is_match(slice16(trimmed, 300)) {
        return from_opml(text);
    }
    if ext == "mm" || MAP.is_match(trimmed) {
        return from_freemind(text);
    }
    if ext == "md" || ext == "markdown" || has_heading(trimmed) {
        return from_markdown(text);
    }
    from_text(text)
}

/// The `.mindloom` file text (one-space indented JSON, as 1.x writes it).
pub fn serialize(doc: &Doc) -> String {
    use serde::Serialize;
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b" "));
    if doc.serialize(&mut ser).is_err() {
        return String::new();
    }
    String::from_utf8(buf).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entities() {
        assert_eq!(decode_entities("a &amp; &#x41;&#66; &NBSP; &bogus; &#xZZ;").unwrap(), "a & AB \u{A0} &bogus; &#xZZ;");
        assert!(decode_entities("&#99999999;").is_err());
    }

    #[test]
    fn xml_parses_nested() {
        let x = parse_xml(r#"<?xml?><a k="1 &lt; 2"><b/><c>t<![CDATA[<raw>]]></c></a>"#).unwrap();
        let a = x.first(0, "A").unwrap();
        assert_eq!(x.attr(a, "k"), Some("1 < 2"));
        assert_eq!(x.els[x.first(a, "c").unwrap()].text, "t<raw>");
    }

    #[test]
    fn xmind_json_and_xml() {
        // stored (method 0) zip with one file
        fn zip(name: &str, data: &[u8]) -> Vec<u8> {
            let mut z = vec![];
            let lh = |z: &mut Vec<u8>| {
                z.extend(0x0403_4b50u32.to_le_bytes());
                z.extend([20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
                z.extend((data.len() as u32).to_le_bytes());
                z.extend((data.len() as u32).to_le_bytes());
                z.extend((name.len() as u16).to_le_bytes());
                z.extend([0, 0]);
            };
            lh(&mut z);
            z.extend(name.as_bytes());
            z.extend(data);
            let cd = z.len();
            z.extend(0x0201_4b50u32.to_le_bytes());
            z.extend([20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
            z.extend((data.len() as u32).to_le_bytes());
            z.extend((data.len() as u32).to_le_bytes());
            z.extend((name.len() as u16).to_le_bytes());
            z.extend([0; 12]);
            z.extend(0u32.to_le_bytes());
            z.extend(name.as_bytes());
            let cdl = z.len() - cd;
            z.extend(0x0605_4b50u32.to_le_bytes());
            z.extend([0, 0, 0, 0, 1, 0, 1, 0]);
            z.extend((cdl as u32).to_le_bytes());
            z.extend((cd as u32).to_le_bytes());
            z.extend([0, 0]);
            z
        }
        let j = r#"[{"rootTopic":{"title":"R","labels":["a"],"notes":{"plain":{"content":"<b>n</b>"}},"children":{"attached":[{"title":"C","branch":"folded"}],"detached":[{"title":"F"}]}}}]"#;
        let d = from_xmind(&zip("content.json", j.as_bytes())).unwrap();
        assert_eq!(d.root.text, "R");
        assert_eq!(d.root.note.as_deref(), Some("n"));
        assert_eq!(d.root.children[0].collapsed, Some(true));
        assert_eq!(d.floating[0].text, "F");
        let x = r#"<xmap-content><sheet><topic><title>Top</title><children><topics type="attached"><topic><title>Kid</title></topic></topics></children></topic></sheet></xmap-content>"#;
        let d = from_xmind(&zip("content.xml", x.as_bytes())).unwrap();
        assert_eq!(d.root.children[0].text, "Kid");
        assert!(from_xmind(b"nope").is_err());
    }

    #[test]
    fn detection() {
        assert!(has_heading("x\n# y"));
        assert!(!has_heading("x #y"));
    }
}
