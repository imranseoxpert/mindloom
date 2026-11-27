//! Fonts: Inter is bundled for the "System" choice (ADR-006); the other map fonts are found among the
//! installed fonts by the names in their CSS stack. The same font database serves PNG/PDF export.

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId};
use mindloom_core::layout::FontSpec;
use mindloom_core::themes::FONTS;
use resvg::usvg::fontdb::{self, Database, Family, Query, Weight};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

pub const INTER: [(&str, &[u8]); 4] = [
    ("Inter-Regular", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
    ("Inter-Bold", include_bytes!("../assets/fonts/Inter-Bold.ttf")),
    ("Inter-Italic", include_bytes!("../assets/fonts/Inter-Italic.ttf")),
    ("Inter-BoldItalic", include_bytes!("../assets/fonts/Inter-BoldItalic.ttf")),
];
pub const INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

/// Fonts used when a character is missing from the map font (CJK and other scripts).
const FALLBACK_FAMILIES: &[&str] = &[
    "PingFang SC",
    "Hiragino Sans",
    "Apple SD Gothic Neo",
    "Microsoft YaHei",
    "Yu Gothic UI",
    "Malgun Gothic",
    "Noto Sans CJK SC",
    "Noto Sans",
    "Arial Unicode MS",
    "Segoe UI Symbol",
    "Nirmala UI",
];

/// The installed fonts plus Inter.
pub fn database() -> Arc<Database> {
    let mut db = Database::new();
    db.load_system_fonts();
    for (_, bytes) in INTER {
        db.load_font_data(bytes.to_vec());
    }
    db.set_sans_serif_family("Inter");
    Arc::new(db)
}

fn css_families(css: &str) -> Vec<Family<'_>> {
    css.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '\'' || c == '"'))
        .filter(|f| !f.is_empty() && !f.starts_with('-') && !f.starts_with("ui-") && *f != "BlinkMacSystemFont")
        .map(|f| match f {
            "serif" => Family::Serif,
            "sans-serif" => Family::SansSerif,
            "monospace" => Family::Monospace,
            "cursive" => Family::Cursive,
            "fantasy" => Family::Fantasy,
            name => Family::Name(name),
        })
        .collect()
}

fn face_bytes(db: &Database, id: fontdb::ID) -> Option<(Vec<u8>, u32)> {
    db.with_face_data(id, |data, index| (data.to_vec(), index))
}

/// The first installed font of a CSS stack, in the given weight and slant.
fn find_face(db: &Database, css: &str, bold: bool, italic: bool) -> Option<fontdb::ID> {
    for fam in css_families(css) {
        if let Family::Name(n) = fam {
            if !db.faces().any(|f| f.families.iter().any(|(name, _)| name.eq_ignore_ascii_case(n))) {
                continue;
            }
        }
        let q = Query {
            families: &[fam],
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
            style: if italic { fontdb::Style::Italic } else { fontdb::Style::Normal },
            ..Default::default()
        };
        if let Some(id) = db.query(&q) {
            return Some(id);
        }
    }
    None
}

/// egui families for every map font: `[regular, bold, italic, bold italic]`.
pub struct MapFonts {
    pub families: Vec<[FontFamily; 4]>,
}

impl MapFonts {
    pub fn family(&self, spec: &FontSpec) -> FontFamily {
        let fi = FONTS.iter().position(|f| f.css == spec.family).unwrap_or(0);
        let v = usize::from(spec.weight >= 600) + 2 * usize::from(spec.italic);
        self.families[fi][v].clone()
    }
}

/// Font definitions for egui: Inter for the interface, Phosphor icons, and one family per map font.
pub fn definitions(db: &Database) -> (FontDefinitions, MapFonts) {
    let mut defs = FontDefinitions::default();
    for (name, bytes) in INTER {
        defs.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    defs.font_data.insert("Inter-SemiBold".into(), Arc::new(FontData::from_static(INTER_SEMIBOLD)));
    let emoji: Vec<String> = defs.font_data.keys().filter(|k| k.to_lowercase().contains("emoji")).cloned().collect();

    let mut fallback = vec![];
    for name in FALLBACK_FAMILIES {
        if let Some(id) = db.query(&Query { families: &[Family::Name(name)], ..Default::default() }) {
            if let Some((bytes, index)) = face_bytes(db, id) {
                let key = format!("fallback-{name}");
                let mut fd = FontData::from_owned(bytes);
                fd.index = index;
                defs.font_data.insert(key.clone(), Arc::new(fd));
                fallback.push(key);
            }
        }
    }

    if let Some(p) = defs.families.get_mut(&FontFamily::Proportional) {
        p.insert(0, "Inter-Regular".into());
        p.extend(fallback.iter().cloned());
    }
    egui_phosphor::add_to_fonts(&mut defs, egui_phosphor::Variant::Regular);
    // Inter has glyphs at some of Phosphor's codepoints, so icons get a family of their own
    defs.families.insert(FontFamily::Name("icons".into()), vec!["phosphor".into(), "Inter-Regular".into()]);
    let rest: Vec<String> = defs.families.get(&FontFamily::Proportional).cloned().unwrap_or_default().into_iter().skip(1).collect();
    defs.families.insert(FontFamily::Name("ui-bold".into()), std::iter::once("Inter-SemiBold".to_string()).chain(rest).collect());

    let mut families = vec![];
    for f in FONTS {
        let mut row: Vec<FontFamily> = vec![];
        for (v, (inter, _)) in INTER.iter().enumerate() {
            let (bold, italic) = (v & 1 == 1, v & 2 == 2);
            let inter = inter.to_string();
            let primary = if f.id == "system" {
                inter.clone()
            } else {
                match find_face(db, f.css, bold, italic).and_then(|id| face_bytes(db, id).map(|b| (id, b))) {
                    Some((id, (bytes, index))) => {
                        let key = format!("face-{id:?}");
                        if !defs.font_data.contains_key(&key) {
                            let mut fd = FontData::from_owned(bytes);
                            fd.index = index;
                            defs.font_data.insert(key.clone(), Arc::new(fd));
                        }
                        key
                    }
                    None => inter.clone(),
                }
            };
            let mut list = vec![primary.clone()];
            if primary != inter {
                list.push(inter);
            }
            list.extend(fallback.iter().cloned());
            list.extend(emoji.iter().cloned());
            let fam = FontFamily::Name(format!("map-{}-{v}", f.id).into());
            defs.families.insert(fam.clone(), list);
            row.push(fam);
        }
        families.push([row[0].clone(), row[1].clone(), row[2].clone(), row[3].clone()]);
    }
    (defs, MapFonts { families })
}

/// Text widths measured with egui's fonts, cached (the layout measures the same words many times).
#[derive(Default)]
pub struct MeasureCache {
    map: RefCell<HashMap<(String, FontFamily, u64), f64>>,
}

impl MeasureCache {
    pub fn width(&self, ctx: &egui::Context, fonts: &MapFonts, text: &str, spec: &FontSpec) -> f64 {
        if text.is_empty() || spec.size <= 0.0 {
            return 0.0;
        }
        let fam = fonts.family(spec);
        let key = (text.to_string(), fam.clone(), spec.size.to_bits());
        if let Some(w) = self.map.borrow().get(&key) {
            return *w;
        }
        let w = ctx.fonts_mut(|f| f.layout_no_wrap(text.to_string(), FontId::new(spec.size as f32, fam), Color32::BLACK).size().x as f64);
        let mut m = self.map.borrow_mut();
        if m.len() > 40_000 {
            m.clear();
        }
        m.insert(key, w);
        w
    }
}
