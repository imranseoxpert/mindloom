//! Themes, shapes, layouts, patterns, markers and style resolution (port of `themes.js`).
//! The tables themselves are generated from the JavaScript into `data.rs`.

pub use crate::data::*;
use crate::model::{Node, Settings};

pub struct Level {
    pub fill: &'static str,
    pub color: &'static str,
    pub border: &'static str,
}
pub struct Theme {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    pub bg: &'static str,
    pub ink: &'static str,
    pub root: Level,
    pub main: Level,
    pub sub: Level,
    pub line: &'static str,
    pub rainbow: bool,
    pub branches: &'static [&'static str],
}
pub struct Named {
    pub id: &'static str,
    pub name: &'static str,
}
pub struct Font {
    pub id: &'static str,
    pub name: &'static str,
    pub css: &'static str,
}
pub struct MapStyle {
    pub id: &'static str,
    pub name: &'static str,
    pub layout: &'static str,
    pub line_style: &'static str,
    pub shapes: [&'static str; 3],
    pub font: Option<&'static str>,
}
pub struct MarkerGroup {
    pub name: &'static str,
    pub items: &'static [&'static str],
}
pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub desc: &'static str,
    /// The template as the JS builds it, as `.mindloom` JSON.
    pub doc: &'static str,
}

pub fn get_theme(id: &str) -> &'static Theme {
    THEMES.iter().find(|t| t.id == id).unwrap_or(&THEMES[0])
}
pub fn get_font(id: &str) -> &'static str {
    FONTS.iter().find(|f| f.id == id).unwrap_or(&FONTS[0]).css
}
pub fn font_by_css(css: &str) -> &'static Font {
    FONTS.iter().find(|f| f.css == css).unwrap_or(&FONTS[0])
}

fn hex_to_rgb(hex: &str) -> Option<[f64; 3]> {
    let h = hex.strip_prefix('#')?;
    let full: String = if h.chars().count() == 3 { h.chars().flat_map(|c| [c, c]).collect() } else { h.to_string() };
    // parseInt(h.slice(0, 6), 16): reads leading hex digits, NaN when there are none
    let digits: String = full.chars().take(6).take_while(|c| c.is_ascii_hexdigit()).collect();
    let n = u32::from_str_radix(&digits, 16).ok()?;
    Some([((n >> 16) & 255) as f64, ((n >> 8) & 255) as f64, (n & 255) as f64])
}

/// Relative luminance; 1 for anything that is not a hex colour.
pub fn luminance(hex: &str) -> f64 {
    let Some(rgb) = hex_to_rgb(hex) else { return 1.0 };
    let c = rgb.map(|v| {
        let v = v / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

pub fn contrast_text(hex: &str) -> &'static str {
    if luminance(hex) > 0.42 {
        "#1a1a1a"
    } else {
        "#ffffff"
    }
}

/// Marker group: markers in one group replace each other.
pub fn marker_group(m: &str) -> String {
    let b = m.as_bytes();
    if b.len() == 2 && b[0] == b'p' && b[1].is_ascii_digit() {
        "p".into()
    } else if m.len() > 2 && m.starts_with("pr") && m[2..].bytes().all(|c| c.is_ascii_digit()) {
        "pr".into()
    } else if m.starts_with("flag-") {
        "flag".into()
    } else if m.starts_with("star-") {
        "star".into()
    } else {
        format!("emoji:{m}")
    }
}

/// A node's resolved look.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub level: &'static str,
    pub fill: String,
    pub color: String,
    pub border: String,
    pub border_width: f64,
    pub border_dash: String,
    pub line: String,
    pub shape: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub align: String,
    /// CSS font stack.
    pub font: &'static str,
}

/// Port of `resolveStyle`. depth 0 = root (or floating root), 1 = main topic, 2+ = subtopic.
pub fn resolve_style(node: &Node, depth: usize, branch: i64, theme: &Theme, s: &Settings, floating: bool) -> Style {
    let cd = if floating { depth + 1 } else { depth };
    let (lvl, base) = match cd {
        0 => ("root", &theme.root),
        1 => ("main", &theme.main),
        _ => ("sub", &theme.sub),
    };
    let rainbow = s.rainbow.unwrap_or(theme.rainbow);
    let bi = if branch < 0 { 0 } else { branch as usize } % theme.branches.len();
    let bcol = theme.branches[bi];
    let (mut fill, mut color, mut border, mut line) = (base.fill.to_string(), base.color.to_string(), base.border.to_string(), theme.line.to_string());
    if rainbow && cd >= 1 {
        line = bcol.into();
        if cd == 1 {
            if theme.id == "neon" || theme.id == "chalk" {
                border = bcol.into();
                color = bcol.into();
            } else {
                fill = bcol.into();
                border = bcol.into();
                color = contrast_text(bcol).into();
            }
        } else {
            border = bcol.into();
        }
    } else if !rainbow && cd == 1 && base.fill == base.color {
        fill = if theme.dark { "#1f2937" } else { "#f3f4f6" }.into();
        color = if theme.dark { "#f9fafb" } else { "#111827" }.into();
        border = theme.line.into();
    }
    let st = node.style.clone().unwrap_or_default();
    let scale = if s.font_scale != 0.0 { s.font_scale } else { 1.0 };
    let fs = crate::js::round(
        if cd == 0 {
            24.0
        } else if cd == 1 {
            17.0
        } else {
            14.0
        } * scale,
    );
    let shape = crate::js::some_str(&st.shape).map(str::to_string).unwrap_or_else(|| {
        let v = match lvl {
            "root" => &s.shapes.root,
            "main" => &s.shapes.main,
            _ => &s.shapes.sub,
        };
        if v.is_empty() {
            "rounded".into()
        } else {
            v.clone()
        }
    });
    let sfill = crate::js::some_str(&st.fill);
    if let Some(f) = sfill {
        if crate::js::some_str(&st.color).is_none() && f != "transparent" {
            color = contrast_text(f).into();
        }
    }
    let font_id = crate::js::some_str(&st.font).unwrap_or(&s.font);
    Style {
        level: lvl,
        fill: sfill.map(str::to_string).unwrap_or(fill),
        color: crate::js::some_str(&st.color).map(str::to_string).unwrap_or(color),
        border: crate::js::some_str(&st.border).map(str::to_string).unwrap_or(border),
        border_width: st.border_width.unwrap_or(if depth == 0 && !floating {
            0.0
        } else if theme.dark {
            1.5
        } else {
            1.2
        }),
        border_dash: crate::js::some_str(&st.border_dash).unwrap_or("solid").to_string(),
        line: crate::js::some_str(&st.line_color).map(str::to_string).unwrap_or(line),
        shape,
        font_size: st.font_size.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(fs),
        bold: st.bold.unwrap_or(cd <= 1),
        italic: st.italic.unwrap_or(false),
        underline: st.underline.unwrap_or(false),
        strike: st.strike.unwrap_or(false),
        align: crate::js::some_str(&st.align).unwrap_or("center").to_string(),
        font: get_font(font_id),
    }
}
