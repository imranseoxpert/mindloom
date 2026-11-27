//! Files on disk: preferences, recent files, crash-safe autosave, opening and saving maps,
//! images, and PNG / PDF / SVG export.

use base64::Engine;
use mindloom_core::layout::Layout;
use mindloom_core::model::{Doc, Settings};
use mindloom_core::themes::{self, Theme};
use mindloom_core::{io, render};
use resvg::usvg::fontdb::Database;
use resvg::{tiny_skia, usvg};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn b64() -> base64::engine::GeneralPurpose {
    base64::engine::general_purpose::STANDARD
}

// ---------- where things live ----------

pub fn config_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("MINDLOOM_CONFIG_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("io.github", "imranseoxpert", "Mindloom")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| std::env::temp_dir().join("Mindloom"))
}

/// Write a file so that a crash never leaves it half written.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("{}.tmp", path.extension().and_then(|e| e.to_str()).unwrap_or("")));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub show_outline: bool,
    pub show_inspector: bool,
    pub show_minimap: bool,
    /// "auto", "light" or "dark".
    pub ui_theme: String,
    pub last_path: Option<String>,
}
impl Default for Prefs {
    fn default() -> Self {
        Prefs { show_outline: false, show_inspector: true, show_minimap: false, ui_theme: "auto".into(), last_path: None }
    }
}

pub fn load_prefs() -> Prefs {
    std::fs::read_to_string(config_dir().join("prefs.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}
pub fn save_prefs(p: &Prefs) {
    if let Ok(s) = serde_json::to_string_pretty(p) {
        let _ = write_atomic(&config_dir().join("prefs.json"), s.as_bytes());
    }
}

pub fn recent() -> Vec<String> {
    let list: Vec<String> = std::fs::read_to_string(config_dir().join("recent.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
    list.into_iter().filter(|p| Path::new(p).exists()).collect()
}
pub fn add_recent(path: &Path) {
    let p = path.to_string_lossy().to_string();
    let mut list = recent();
    list.retain(|x| *x != p);
    list.insert(0, p);
    list.truncate(12);
    if let Ok(s) = serde_json::to_string(&list) {
        let _ = write_atomic(&config_dir().join("recent.json"), s.as_bytes());
    }
}
pub fn clear_recent() {
    let _ = write_atomic(&config_dir().join("recent.json"), b"[]");
}

// ---------- autosave ----------

fn autosave_dir() -> PathBuf {
    config_dir().join("autosave")
}

#[derive(Serialize, Deserialize)]
pub struct Autosave {
    pub doc: serde_json::Value,
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
    pub t: u64,
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// This window's autosave file (unique per process).
pub fn autosave_key() -> String {
    format!("{:x}-{}", now_ms(), std::process::id())
}
pub fn autosave_write(key: &str, a: &Autosave) -> bool {
    match serde_json::to_vec(a) {
        Ok(bytes) => write_atomic(&autosave_dir().join(format!("{key}.json")), &bytes).is_ok(),
        Err(_) => false,
    }
}
/// A clean close: nothing to recover next time.
pub fn autosave_clear(key: &str) {
    let _ = std::fs::remove_file(autosave_dir().join(format!("{key}.json")));
}
/// The newest autosave left by a session that did not close cleanly (older ones are removed).
pub fn autosave_recover(own: &str) -> Option<Autosave> {
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = std::fs::read_dir(autosave_dir())
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && !p.file_stem().is_some_and(|s| s == own))
        .filter_map(|p| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (p, t)))
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    let first = files.first()?.0.clone();
    let found = std::fs::read_to_string(&first).ok().and_then(|s| serde_json::from_str::<Autosave>(&s).ok());
    for (p, t) in &files {
        let old = t.elapsed().map(|d| d.as_secs() > 7 * 86_400).unwrap_or(false);
        if *p == first || old {
            let _ = std::fs::remove_file(p);
        }
    }
    found
}

// ---------- opening and saving ----------

pub const OPEN_EXTS: &[&str] = &["mindloom", "xmind", "mm", "opml", "md", "markdown", "txt", "json", "csv"];

/// Read any supported map file.
pub fn read_map(path: &Path) -> Result<Doc, String> {
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if name.to_lowercase().ends_with(".xmind") {
        return io::from_xmind(&bytes);
    }
    let text = String::from_utf8_lossy(&bytes);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    io::import_text(&name, text)
}

pub fn is_native(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mindloom"))
}

/// A safe file name from a map name.
pub fn file_base(name: &str) -> String {
    let s: String = name.chars().map(|c| if "\\/:*?\"<>|".contains(c) || c.is_control() { '-' } else { c }).collect();
    let s = s.trim().to_string();
    if s.is_empty() {
        "Untitled".into()
    } else {
        s
    }
}

// ---------- images ----------

pub fn data_url_bytes(src: &str) -> Option<Vec<u8>> {
    let (_, data) = src.strip_prefix("data:")?.split_once(";base64,")?;
    b64().decode(data.trim()).ok()
}

/// An image file or clipboard image as a downscaled data URL, with the size it should show at.
pub fn image_data_url(img: image::DynamicImage) -> Result<(String, f64, f64), String> {
    let (w, h) = (img.width().max(1), img.height().max(1));
    let k = (900.0 / w.max(h) as f64).min(1.0);
    let img = if k < 1.0 {
        img.resize(((w as f64 * k).round() as u32).max(1), ((h as f64 * k).round() as u32).max(1), image::imageops::FilterType::Lanczos3)
    } else {
        img
    };
    let mut buf = std::io::Cursor::new(Vec::new());
    let has_alpha = img.color().has_alpha();
    let mime = if has_alpha {
        img.write_to(&mut buf, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        "image/png"
    } else {
        let rgb = img.to_rgb8();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buf, 88).encode_image(&rgb).map_err(|e| e.to_string())?;
        "image/jpeg"
    };
    let (iw, ih) = (img.width() as f64, img.height() as f64);
    let sw = iw.min(240.0);
    Ok((format!("data:{mime};base64,{}", b64().encode(buf.into_inner())), sw, (ih * sw / iw).round()))
}

// ---------- export ----------

/// The map as a standalone SVG. Text in the "System" font names Inter first, the font it was measured with.
pub fn export_svg(l: &Layout, doc: &Doc, theme: &Theme, settings: &Settings, transparent: bool, padding: Option<f64>) -> render::Exported {
    let mut out = render::export_svg(l, doc, theme, settings, transparent, padding);
    let sys = render::esc(themes::FONTS[0].css);
    out.svg = out.svg.replace(&format!("font-family=\"{sys}\""), &format!("font-family=\"Inter, {sys}\""));
    out
}

fn tree(svg: &str, db: &Arc<Database>) -> Result<usvg::Tree, String> {
    let opt = usvg::Options { fontdb: db.clone(), ..Default::default() };
    usvg::Tree::from_str(svg, &opt).map_err(|e| e.to_string())
}

/// Rasterise an exported SVG. Returns (width, height, premultiplied RGBA pixmap).
pub fn raster(svg: &str, w: f64, h: f64, scale: f64, db: &Arc<Database>) -> Result<tiny_skia::Pixmap, String> {
    let t = tree(svg, db)?;
    let k = scale.min(16000.0 / w).min(16000.0 / h).min((120e6 / (w * h)).sqrt());
    let (pw, ph) = ((w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32);
    let mut pm = tiny_skia::Pixmap::new(pw, ph).ok_or("The map is too large to export as an image.")?;
    resvg::render(&t, tiny_skia::Transform::from_scale(k as f32, k as f32), &mut pm.as_mut());
    Ok(pm)
}

pub fn png(svg: &str, w: f64, h: f64, scale: f64, db: &Arc<Database>) -> Result<Vec<u8>, String> {
    raster(svg, w, h, scale, db)?.encode_png().map_err(|e| e.to_string())
}

/// Straight (not premultiplied) RGBA, for the clipboard.
pub fn rgba(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    pm.pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

pub fn pdf(svg: &str, db: &Arc<Database>) -> Result<Vec<u8>, String> {
    let t = tree(svg, db)?;
    svg2pdf::to_pdf(&t, svg2pdf::ConversionOptions::default(), svg2pdf::PageOptions::default()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_safe() {
        assert_eq!(file_base("a/b: c?"), "a-b- c-");
        assert_eq!(file_base("  "), "Untitled");
    }

    #[test]
    fn images_round_trip() {
        let img = image::DynamicImage::new_rgba8(1200, 600);
        let (url, w, h) = image_data_url(img).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));
        assert_eq!((w, h), (240.0, 120.0));
        let back = image::load_from_memory(&data_url_bytes(&url).unwrap()).unwrap();
        assert_eq!((back.width(), back.height()), (900, 450));
    }

    #[test]
    fn exports_render() {
        let db = crate::fonts::database();
        let doc = mindloom_core::templates::build(themes::TEMPLATES[1].id);
        let theme = themes::get_theme(&doc.settings.theme);
        let m = |t: &str, f: &mindloom_core::layout::FontSpec| mindloom_core::layout::test_measure(t, f);
        let ctx = mindloom_core::layout::Ctx { theme, settings: &doc.settings, measure: &m, focus_id: None, ignore_offset_id: None, today: "2026-11-15" };
        let l = mindloom_core::layout::layout(&doc, &ctx);
        let e = export_svg(&l, &doc, theme, &doc.settings, false, None);
        assert!(e.svg.contains("Inter, "));
        let p = png(&e.svg, e.width, e.height, 1.0, &db).unwrap();
        assert_eq!(&p[1..4], b"PNG");
        let d = pdf(&e.svg, &db).unwrap();
        assert!(d.starts_with(b"%PDF"));
    }
}
