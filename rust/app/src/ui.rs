//! Everything around the canvas (port of `panels.js` and the toolbar): menus, toolbar, inspector,
//! outline, status bar, dialogs, the AI assistant and keyboard shortcuts.

use crate::ai;
use crate::canvas::{self, alpha, Canvas, PaintOpts, View, ACCENT};
use crate::state::{After, App, Dialog, Tab};
use egui::{pos2, vec2, Align2, Color32, CornerRadius, Event, FontId, Id, Key, Modifiers, RichText, Sense, Stroke, StrokeKind, Ui};
use egui_phosphor::regular as icon;
use mindloom_core::layout::Icon;
use mindloom_core::model::{self, Doc};
use mindloom_core::themes::{self, FONTS, LAYOUTS, LINE_STYLES, MAP_STYLES, MARKER_GROUPS, PATTERNS, SHAPES, SWATCHES, TEMPLATES, THEMES};
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::Receiver;

/// An icon glyph in the icon font.
pub fn ic(g: &str) -> RichText {
    RichText::new(g).family(egui::FontFamily::Name("icons".into()))
}

/// Shortcut text for this platform (`Mod+` is ⌘ on macOS, Ctrl elsewhere).
pub fn kb(s: &str) -> String {
    if cfg!(target_os = "macos") {
        s.replace("Mod+", "⌘").replace("Shift+", "⇧").replace("Alt+", "⌥")
    } else {
        s.replace("Mod+", "Ctrl+")
    }
}

// ---------- look ----------

pub fn apply_visuals(ctx: &egui::Context, pref: &str) {
    ctx.options_mut(|o| {
        o.theme_preference = match pref {
            "dark" => egui::ThemePreference::Dark,
            "light" => egui::ThemePreference::Light,
            _ => egui::ThemePreference::System,
        }
    });
    ctx.all_styles_mut(|s| {
        let dark = s.visuals.dark_mode;
        let accent = if dark { Color32::from_rgb(0x6b, 0x8c, 0xff) } else { Color32::from_rgb(0x3b, 0x6c, 0xf6) };
        s.visuals.selection.bg_fill = alpha(accent, if dark { 0.45 } else { 0.25 });
        s.visuals.selection.stroke = Stroke::new(1.0, accent);
        s.visuals.hyperlink_color = accent;
        s.visuals.widgets.hovered.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.inactive.corner_radius = CornerRadius::same(6);
        s.visuals.widgets.active.corner_radius = CornerRadius::same(6);
        s.visuals.window_corner_radius = CornerRadius::same(12);
        s.visuals.panel_fill = if dark { Color32::from_rgb(0x1d, 0x1f, 0x24) } else { Color32::WHITE };
        s.visuals.window_fill = s.visuals.panel_fill;
        s.visuals.extreme_bg_color = if dark { Color32::from_rgb(0x25, 0x28, 0x2e) } else { Color32::from_rgb(0xf5, 0xf6, 0xf8) };
        s.spacing.item_spacing = vec2(6.0, 6.0);
        s.spacing.button_padding = vec2(8.0, 4.0);
    });
}

// ---------- the whole frame ----------

pub fn frame(app: &mut App, ui: &mut Ui) {
    shortcuts(app, ui.ctx());
    let presenting = app.present.is_some();
    if !presenting {
        egui::Panel::top("menubar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| menus(app, ui));
        });
        egui::Panel::top("toolbar").frame(egui::Frame::side_top_panel(ui.style()).inner_margin(egui::Margin::symmetric(8, 5))).show(ui, |ui| toolbar(app, ui));
        egui::Panel::bottom("status").show(ui, |ui| status(app, ui));
        if app.prefs.show_outline {
            egui::Panel::left("outline").resizable(true).default_size(250.0).show(ui, |ui| outline(app, ui));
        }
        if app.prefs.show_inspector {
            egui::Panel::right("inspector").resizable(true).default_size(300.0).show(ui, |ui| inspector(app, ui));
        }
    }
    egui::CentralPanel::no_frame().show(ui, |ui| crate::stage::show(app, ui));
    dialogs(app, ui.ctx());
    toasts(app, ui.ctx());
}

fn toasts(app: &mut App, ctx: &egui::Context) {
    let now = app.now;
    app.toasts.retain(|t| t.1 > now);
    if let Some((msg, _)) = app.toasts.last() {
        egui::Area::new(Id::new("toast")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -70.0)).order(egui::Order::Tooltip).show(ctx, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(0x1f, 0x23, 0x29))
                .corner_radius(CornerRadius::same(8))
                .inner_margin(egui::Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(RichText::new(msg).color(Color32::WHITE));
                });
        });
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

// ---------- menus ----------

fn mi(ui: &mut Ui, label: &str, key: &str) -> bool {
    let r = ui.add(egui::Button::new(label).shortcut_text(kb(key)));
    if r.clicked() {
        ui.close();
    }
    r.clicked()
}
fn mi_on(ui: &mut Ui, label: &str, key: &str, enabled: bool) -> bool {
    let r = ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(kb(key)));
    if r.clicked() {
        ui.close();
    }
    r.clicked()
}

fn menus(app: &mut App, ui: &mut Ui) {
    ui.menu_button("File", |ui| {
        if mi(ui, "New from template…", "Mod+N") {
            app.dialog = Some(Dialog::Templates);
        }
        if mi(ui, "New window", "Mod+Shift+N") {
            new_window();
        }
        if mi(ui, "Open…", "Mod+O") {
            app.guard(After::Open);
        }
        ui.menu_button("Open recent", |ui| {
            let rec = crate::files::recent();
            if rec.is_empty() {
                ui.add_enabled(false, egui::Button::new("No recent files"));
            }
            for p in rec {
                if ui.button(&p).clicked() {
                    ui.close();
                    app.guard(After::OpenPath(p.into()));
                }
            }
            ui.separator();
            if ui.button("Clear recent").clicked() {
                crate::files::clear_recent();
                ui.close();
            }
        });
        ui.separator();
        if mi(ui, "Save", "Mod+S") {
            app.save(false);
        }
        if mi(ui, "Save as…", "Mod+Shift+S") {
            app.save(true);
        }
        ui.separator();
        ui.menu_button("Export", |ui| export_items(app, ui));
        ui.separator();
        if mi(ui, if cfg!(target_os = "macos") { "Quit Mindloom" } else { "Exit" }, "Mod+Q") {
            app.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    });
    ui.menu_button("Edit", |ui| {
        if mi_on(ui, "Undo", "Mod+Z", app.history.can_undo()) {
            app.undo();
        }
        if mi_on(ui, "Redo", if cfg!(target_os = "macos") { "Mod+Shift+Z" } else { "Mod+Y" }, app.history.can_redo()) {
            app.redo();
        }
        ui.separator();
        if mi(ui, "Cut", "Mod+X") {
            if let Some(t) = app.cut() {
                ui.ctx().copy_text(t);
            }
        }
        if mi(ui, "Copy", "Mod+C") {
            if let Some(t) = app.copy() {
                ui.ctx().copy_text(t);
            }
        }
        if mi(ui, "Paste", "Mod+V") {
            paste_from_system(app);
        }
        if mi(ui, "Duplicate", "Mod+D") {
            app.duplicate();
        }
        if mi(ui, "Delete", "Del") {
            app.delete_sel();
        }
        ui.separator();
        if mi(ui, "Copy style", "Mod+Alt+C") {
            app.copy_style();
        }
        if mi(ui, "Paste style", "Mod+Alt+V") {
            app.paste_style();
        }
        ui.separator();
        if mi(ui, "Find…", "Mod+F") {
            open_search(app);
        }
    });
    ui.menu_button("Insert", |ui| {
        if mi(ui, "Topic", "Enter") {
            app.add_sibling(false);
        }
        if mi(ui, "Subtopic", "Tab") {
            app.add_child();
        }
        if mi(ui, "Parent topic", "Mod+Enter") {
            app.add_parent();
        }
        if mi(ui, "Floating topic", "") {
            let c = app.to_world(app.stage.center());
            app.floating_at(c.0, c.1);
        }
        ui.separator();
        if mi(ui, "Relationship", "Mod+Shift+R") {
            app.relation();
        }
        if mi(ui, "Boundary", "Mod+Shift+B") {
            app.boundary();
        }
        if mi(ui, "Note", "Mod+Shift+M") {
            show_note(app);
        }
        if mi(ui, "Link…", "Mod+K") {
            link_dialog(app);
        }
        if mi(ui, "Image…", "") {
            app.pick_image();
        }
        if mi(ui, "Task checkbox", "Mod+Shift+T") {
            app.task();
        }
        if mi(ui, "Markers…", "") {
            show_tab(app, Tab::Markers);
        }
        ui.separator();
        if mi(ui, "AI assistant…", "") {
            open_ai(app, false);
        }
    });
    ui.menu_button("View", |ui| {
        if mi(ui, "Zoom in", "Mod+=") {
            app.zoom_at(1.2, None);
        }
        if mi(ui, "Zoom out", "Mod+-") {
            app.zoom_at(1.0 / 1.2, None);
        }
        if mi(ui, "Actual size", "") {
            app.actual_size();
        }
        if mi(ui, "Fit map", "Mod+0") {
            app.fit(true);
        }
        ui.separator();
        if mi(ui, "Focus on branch", "Mod+Shift+F") {
            app.focus();
        }
        if mi(ui, "Presentation", "F5") {
            app.present();
        }
        ui.separator();
        if mi(ui, "Expand all", "") {
            app.expand_all();
        }
        if mi(ui, "Collapse all", "") {
            app.collapse_all();
        }
        ui.separator();
        if mi(ui, "Outline panel", "Mod+\\") {
            toggle_outline(app);
        }
        if mi(ui, "Format panel", "Mod+Shift+\\") {
            toggle_inspector(app);
        }
        if mi(ui, "Mini map", "") {
            app.prefs.show_minimap = !app.prefs.show_minimap;
            crate::files::save_prefs(&app.prefs);
        }
        ui.menu_button("Interface", |ui| {
            for (v, l) in [("auto", "Follow system"), ("light", "Light"), ("dark", "Dark")] {
                if ui.radio(app.prefs.ui_theme == v, l).clicked() {
                    app.prefs.ui_theme = v.into();
                    crate::files::save_prefs(&app.prefs);
                    apply_visuals(ui.ctx(), v);
                    ui.close();
                }
            }
        });
    });
    ui.menu_button("Help", |ui| {
        if mi(ui, "Keyboard shortcuts", "F1") {
            app.dialog = Some(Dialog::Shortcuts);
        }
        if mi(ui, "Mindloom on GitHub", "") {
            ui.ctx().open_url(egui::OpenUrl::new_tab("https://github.com/imranseoxpert/mindloom"));
        }
        if mi(ui, "About Mindloom", "") {
            app.dialog = Some(Dialog::About);
        }
    });
}

fn export_items(app: &mut App, ui: &mut Ui) {
    for (k, l) in [("png", "PNG image"), ("png-t", "PNG (transparent)"), ("svg", "SVG vector"), ("pdf", "PDF document")] {
        if mi(ui, l, "") {
            app.export(k);
        }
    }
    ui.separator();
    for (k, l) in [("md", "Markdown (.md)"), ("opml", "OPML outline"), ("txt", "Plain text outline"), ("mm", "FreeMind / Freeplane (.mm)")] {
        if mi(ui, l, "") {
            app.export(k);
        }
    }
    ui.separator();
    if mi(ui, "Copy map as image", "") {
        app.export("copy-image");
    }
}

pub fn new_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--new").spawn();
    }
}

fn show_tab(app: &mut App, t: Tab) {
    app.prefs.show_inspector = true;
    app.tab = t;
}
fn show_note(app: &mut App) {
    if app.primary().is_none() {
        return app.toast("Select a topic first.");
    }
    show_tab(app, Tab::Notes);
    app.note_focus = true;
}
fn link_dialog(app: &mut App) {
    match app.primary().and_then(|p| model::get(&app.doc, &p).cloned()) {
        Some(n) => app.dialog = Some(Dialog::Link(n.link.unwrap_or_default())),
        None => app.toast("Select a topic first."),
    }
}
fn open_search(app: &mut App) {
    app.search_open = true;
    app.search_focus = true;
}
fn open_ai(app: &mut App, expand: bool) {
    let node = app.primary();
    app.dialog = Some(Dialog::Ai(Box::new(AiForm::new(node, expand))));
}
fn toggle_outline(app: &mut App) {
    app.prefs.show_outline = !app.prefs.show_outline;
    crate::files::save_prefs(&app.prefs);
}
fn toggle_inspector(app: &mut App) {
    app.prefs.show_inspector = !app.prefs.show_inspector;
    crate::files::save_prefs(&app.prefs);
}
fn paste_from_system(app: &mut App) {
    let mut cb = arboard::Clipboard::new().ok();
    let text = cb.as_mut().and_then(|c| c.get_text().ok()).filter(|t| !t.trim().is_empty());
    if text.is_none() {
        if let Some(img) = cb.as_mut().and_then(|c| c.get_image().ok()) {
            if let Some(buf) = image::RgbaImage::from_raw(img.width as u32, img.height as u32, img.bytes.into_owned()) {
                return app.set_image(None, image::DynamicImage::ImageRgba8(buf));
            }
        }
    }
    app.paste(text.as_deref());
}

// ---------- toolbar ----------

fn tb(ui: &mut Ui, glyph: &str, label: &str, tip: &str, on: bool) -> bool {
    let b =
        if label.is_empty() || compact(ui) { egui::Button::new(ic(glyph).size(17.0)) } else { egui::Button::new((ic(glyph).size(16.0), RichText::new(label))) };
    ui.add(b.frame(on).selected(on)).on_hover_text(kb(tip)).clicked()
}
fn tb_on(ui: &mut Ui, glyph: &str, label: &str, tip: &str, enabled: bool) -> bool {
    let b = if compact(ui) { egui::Button::new(ic(glyph).size(17.0)) } else { egui::Button::new((ic(glyph).size(16.0), RichText::new(label))) };
    ui.add_enabled(enabled, b.frame(false)).on_hover_text(kb(tip)).clicked()
}
/// Icons only when the window is too narrow for labels.
fn compact(ui: &Ui) -> bool {
    ui.ctx().data(|d| d.get_temp::<bool>(Id::new("toolbar-compact"))).unwrap_or(false)
}

fn toolbar(app: &mut App, ui: &mut Ui) {
    let narrow = ui.available_width() < 1500.0;
    ui.ctx().data_mut(|d| d.insert_temp(Id::new("toolbar-compact"), narrow));
    ui.horizontal(|ui| {
        ui.label(RichText::new("◉").color(ACCENT).size(18.0));
        ui.label(RichText::new(&app.name).strong());
        if app.dirty {
            ui.label(RichText::new("●").color(Color32::from_rgb(0xf5, 0x9f, 0x00)).size(10.0)).on_hover_text("Unsaved changes");
        }
        ui.separator();
        if tb(ui, icon::FILE_PLUS, "New", "New from template (Mod+N)", false) {
            app.dialog = Some(Dialog::Templates);
        }
        if tb(ui, icon::FOLDER_OPEN, "Open", "Open… (Mod+O)", false) {
            app.guard(After::Open);
        }
        if tb(ui, icon::FLOPPY_DISK, "Save", "Save (Mod+S)", false) {
            app.save(false);
        }
        ui.separator();
        if tb_on(ui, icon::ARROW_COUNTER_CLOCKWISE, "Undo", "Undo (Mod+Z)", app.history.can_undo()) {
            app.undo();
        }
        if tb_on(ui, icon::ARROW_CLOCKWISE, "Redo", "Redo (Mod+Shift+Z)", app.history.can_redo()) {
            app.redo();
        }
        ui.separator();
        if tb(ui, icon::PLUS_SQUARE, "Topic", "Add topic (Enter)", false) {
            app.add_sibling(false);
        }
        if tb(ui, icon::TREE_STRUCTURE, "Subtopic", "Add subtopic (Tab)", false) {
            app.add_child();
        }
        if tb(ui, icon::FLOW_ARROW, "Relation", "Relationship (Mod+Shift+R)", app.relation_from.is_some()) {
            app.relation();
        }
        if tb(ui, icon::BOUNDING_BOX, "Boundary", "Boundary (Mod+Shift+B)", false) {
            app.boundary();
        }
        if tb(ui, icon::NOTE_PENCIL, "Note", "Note (Mod+Shift+M)", false) {
            show_note(app);
        }
        if tb(ui, icon::LINK, "Link", "Link (Mod+K)", false) {
            link_dialog(app);
        }
        if tb(ui, icon::IMAGE, "Image", "Insert image", false) {
            app.pick_image();
        }
        if tb(ui, icon::STAR, "Marker", "Markers & stickers", false) {
            show_tab(app, Tab::Markers);
        }
        ui.separator();
        if tb(ui, icon::PALETTE, "Style", "Structure, theme & style", false) {
            show_tab(app, Tab::Map);
        }
        if tb(ui, icon::SPARKLE, "AI", "AI assistant", false) {
            open_ai(app, false);
        }
        if tb(ui, icon::CROSSHAIR, "Focus", "Focus on branch (Mod+Shift+F)", app.focus_id.is_some()) {
            app.focus();
        }
        if tb(ui, icon::PRESENTATION, "Present", "Presentation (F5)", false) {
            app.present();
        }
        ui.menu_button((ic(icon::EXPORT).size(16.0), RichText::new(if narrow { "" } else { "Export" })), |ui| export_items(app, ui))
            .response
            .on_hover_text("Export");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if tb(ui, icon::QUESTION, "", "Keyboard shortcuts (F1)", false) {
                app.dialog = Some(Dialog::Shortcuts);
            }
            let dark = ui.visuals().dark_mode;
            if tb(ui, if dark { icon::SUN } else { icon::MOON }, "", "Light / dark interface", false) {
                let next = match app.prefs.ui_theme.as_str() {
                    "auto" => "dark",
                    "dark" => "light",
                    _ => "auto",
                };
                app.prefs.ui_theme = next.into();
                crate::files::save_prefs(&app.prefs);
                apply_visuals(ui.ctx(), next);
                app.toast(format!("Interface: {}", if next == "auto" { "follow system" } else { next }));
            }
            if tb(ui, icon::SLIDERS, "", "Format panel (Mod+Shift+\\)", app.prefs.show_inspector) {
                toggle_inspector(app);
            }
            if tb(ui, icon::LIST_BULLETS, "", "Outline panel (Mod+\\)", app.prefs.show_outline) {
                toggle_outline(app);
            }
            if tb(ui, icon::MAGNIFYING_GLASS, "", "Find (Mod+F)", app.search_open) {
                open_search(app);
            }
        });
    });
}

fn status(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(app.stats_line()).small());
        ui.separator();
        ui.label(
            RichText::new(
                "Tab: subtopic · Enter: topic · Drag a topic to place it (it stays connected) · Drop onto a topic to re-parent · Shift+drag to select",
            )
            .small()
            .weak(),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(&app.save_status).small().weak());
        });
    });
}

// ---------- outline ----------

fn outline(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        ui.strong("Outline");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("✕").clicked() {
                toggle_outline(app);
            }
        });
    });
    ui.separator();
    let sel: HashSet<String> = app.sel.iter().cloned().collect();
    let mut rows: Vec<(String, usize, String, bool, bool, Color32)> = vec![];
    fn rec(app: &App, n: &model::Node, d: usize, rows: &mut Vec<(String, usize, String, bool, bool, Color32)>) {
        let col = app
            .geo
            .get(&n.id)
            .map(|t| if t.style.fill != "transparent" && t.depth <= 1 { canvas::color(&t.style.fill) } else { canvas::color(&t.style.line) })
            .unwrap_or(Color32::GRAY);
        rows.push((n.id.clone(), d, n.text.replace('\n', " "), !n.children.is_empty(), n.is_collapsed(), col));
        if !n.is_collapsed() {
            for c in &n.children {
                rec(app, c, d + 1, rows);
            }
        }
    }
    for t in app.doc.trees() {
        rec(app, t, 0, &mut rows);
    }
    let mut action: Option<(String, u8)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (id, d, text, kids, collapsed, col) in &rows {
            let on = sel.contains(id);
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
            if on {
                ui.painter().rect_filled(r, CornerRadius::same(5), alpha(ACCENT, 0.15));
            } else if resp.hovered() {
                ui.painter().rect_filled(r, CornerRadius::same(5), ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let x0 = r.left() + 6.0 + *d as f32 * 14.0;
            let tw = egui::Rect::from_min_size(pos2(x0, r.top()), vec2(14.0, 24.0));
            if *kids {
                ui.painter().text(
                    tw.center(),
                    Align2::CENTER_CENTER,
                    if *collapsed { "▶" } else { "▼" },
                    FontId::proportional(9.0),
                    ui.visuals().weak_text_color(),
                );
            }
            ui.painter().circle_filled(pos2(x0 + 20.0, r.center().y), 4.0, *col);
            let (label, color) =
                if text.is_empty() { ("Topic".to_string(), ui.visuals().weak_text_color()) } else { (text.clone(), ui.visuals().text_color()) };
            let clip = ui.painter().with_clip_rect(r);
            clip.text(pos2(x0 + 30.0, r.center().y), Align2::LEFT_CENTER, label, FontId::proportional(13.0), color);
            if resp.clicked() {
                let on_toggle = resp.interact_pointer_pos().is_some_and(|p| tw.contains(p));
                action = Some((id.clone(), if on_toggle && *kids { 1 } else { 0 }));
            }
            if resp.double_clicked() {
                action = Some((id.clone(), 2));
            }
            if on && app.drag.is_none() && resp.rect.is_positive() && app.editing.is_none() && ui.ctx().input(|i| i.pointer.any_pressed()) {
                // keep nothing: scrolling to the selection happens when it changes below
            }
        }
    });
    if let Some((id, kind)) = action {
        match kind {
            1 => {
                model::set_collapsed(&mut app.doc, &id, None);
                app.commit(false);
            }
            2 => app.start_edit(&id, None),
            _ => {
                if let Some(f) = app.focus_id.clone() {
                    if f != id && !model::is_ancestor(&app.doc, &f, &id) {
                        app.focus();
                    }
                }
                app.select(vec![id.clone()], false, false);
                app.center_on(&id, true);
            }
        }
    }
}

// ---------- inspector ----------

fn inspector(app: &mut App, ui: &mut Ui) {
    ui.horizontal(|ui| {
        for (t, l) in [(Tab::Topic, "Topic"), (Tab::Map, "Map"), (Tab::Markers, "Markers"), (Tab::Notes, "Notes")] {
            if ui.selectable_label(app.tab == t, l).clicked() {
                app.tab = t;
            }
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.set_width(ui.available_width() - 6.0);
        match app.tab {
            Tab::Topic => topic_tab(app, ui),
            Tab::Map => map_tab(app, ui),
            Tab::Markers => markers_tab(app, ui),
            Tab::Notes => notes_tab(app, ui),
        }
    });
}

fn h4(ui: &mut Ui, s: &str) {
    ui.add_space(8.0);
    ui.label(RichText::new(s.to_uppercase()).small().strong().color(ui.visuals().weak_text_color()));
}

/// A row of colour swatches; returns the picked colour ("" for transparent where offered).
fn swatches(ui: &mut Ui, id: &str, current: Option<&str>, list: &[&str]) -> Option<String> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in list {
            let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
            let on = current == Some(*c);
            let p = ui.painter();
            if *c == "transparent" {
                p.rect(r, CornerRadius::same(5), Color32::WHITE, Stroke::new(1.0, Color32::from_gray(190)), StrokeKind::Inside);
                p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, Color32::from_rgb(0xe0, 0x31, 0x31)));
            } else {
                p.rect(r, CornerRadius::same(5), canvas::color(c), Stroke::new(1.0, Color32::from_black_alpha(40)), StrokeKind::Inside);
            }
            if on {
                p.rect_stroke(r.expand(2.0), CornerRadius::same(6), Stroke::new(2.0, ACCENT), StrokeKind::Outside);
            }
            if resp.on_hover_text(*c).clicked() {
                out = Some(c.to_string());
            }
        }
        let mut rgb = current.filter(|c| c.len() == 7 && c.starts_with('#')).map(canvas::color).unwrap_or(ACCENT);
        let r = ui.color_edit_button_srgba(&mut rgb).on_hover_text("Custom colour");
        if r.changed() {
            out = Some(format!("#{:02x}{:02x}{:02x}", rgb.r(), rgb.g(), rgb.b()));
        }
        let _ = id;
    });
    out
}

/// Segmented buttons; returns the picked value.
fn seg<T: PartialEq + Clone>(ui: &mut Ui, cur: &T, opts: &[(T, &str)]) -> Option<T> {
    let mut out = None;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (v, l) in opts {
            if ui.selectable_label(cur == v, *l).clicked() {
                out = Some(v.clone());
            }
        }
    });
    out
}

/// A slider that previews live and records one undo step when released.
fn slider(app: &mut App, ui: &mut Ui, label: &str, mut v: f64, range: std::ops::RangeInclusive<f64>, step: f64, set: impl Fn(&mut App, f64)) {
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new(label));
        let s = egui::Slider::new(&mut v, range).step_by(step);
        let r = ui.add(if step >= 1.0 { s.fixed_decimals(0) } else { s });
        if r.changed() {
            set(app, v);
            app.refresh_geo();
        }
        if r.drag_stopped() || (r.changed() && !r.dragged()) {
            app.commit(false);
            app.previews.clear();
        }
    });
}

fn shape_icon(ui: &mut Ui, shape: &str, on: bool, name: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(vec2(60.0, 46.0), Sense::click());
    let v = ui.visuals();
    let bg = if on {
        alpha(ACCENT, 0.15)
    } else if resp.hovered() {
        v.widgets.hovered.weak_bg_fill
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect(r, CornerRadius::same(6), bg, Stroke::new(1.0, if on { ACCENT } else { v.widgets.noninteractive.bg_stroke.color }), StrokeKind::Inside);
    let (w, h) = (36.0, 18.0);
    let o = pos2(r.center().x - w / 2.0, r.top() + 6.0);
    let pts: Vec<egui::Pos2> = canvas::outline(shape, w as f64, h as f64, false).into_iter().map(|(x, y)| pos2(o.x + x as f32, o.y + y as f32)).collect();
    let ink = v.text_color();
    match shape {
        "underline" => {
            ui.painter().line_segment([pos2(o.x, o.y + h), pos2(o.x + w, o.y + h)], Stroke::new(1.6, ink));
        }
        "none" => {
            ui.painter().line_segment([pos2(o.x + 6.0, o.y + h / 2.0), pos2(o.x + w - 6.0, o.y + h / 2.0)], Stroke::new(3.0, alpha(ink, 0.5)));
        }
        _ => {
            ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.4, ink)));
        }
    }
    ui.painter().text(pos2(r.center().x, r.bottom() - 9.0), Align2::CENTER_CENTER, name, FontId::proportional(10.0), v.weak_text_color());
    resp.on_hover_text(name).clicked()
}

fn topic_tab(app: &mut App, ui: &mut Ui) {
    if let Some(rid) = app.sel_rel.clone() {
        let Some(r) = app.doc.relations.iter().find(|x| x.id == rid).cloned() else { return };
        h4(ui, "Relationship");
        let mut label = r.label.clone();
        let resp = ui.add(egui::TextEdit::singleline(&mut label).hint_text("Label (e.g. depends on)").desired_width(f32::INFINITY));
        if resp.changed() {
            if let Some(x) = app.doc.relations.iter_mut().find(|x| x.id == rid) {
                x.label = label;
            }
            app.refresh_geo();
        }
        if resp.lost_focus() {
            app.commit(false);
        }
        h4(ui, "Colour");
        if let Some(c) =
            swatches(ui, "rel", r.color.as_deref(), &["#5c677d", "#e03131", "#f76707", "#2f9e44", "#1c7ed6", "#7048e8", "#c2255c", "#000000", "#ffffff"])
        {
            if let Some(x) = app.doc.relations.iter_mut().find(|x| x.id == rid) {
                x.color = Some(c);
            }
            app.commit(false);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("⇄ Reverse").clicked() {
                app.relation_reverse();
            }
            if ui.button(RichText::new("Delete").color(Color32::from_rgb(0xe0, 0x31, 0x31))).clicked() {
                app.delete_sel();
            }
        });
        return;
    }
    let Some(id) = app.primary() else {
        ui.label(RichText::new("Select a topic to style it. Use the Map tab for themes, structures and backgrounds.").weak());
        h4(ui, "Quick start");
        for l in [
            "Tab — add subtopic",
            "Enter — add topic",
            "Space / F2 — edit text",
            "Drag a topic anywhere — it stays connected",
            "Drop it onto another topic to re-parent it",
            "Double-click the canvas — floating topic",
            "Right-click for more actions",
        ] {
            ui.label(RichText::new(l).weak());
        }
        return;
    };
    let (Some(n), Some(t)) = (model::get(&app.doc, &id).cloned(), app.geo.get(&id).cloned()) else { return };
    let st = t.style.clone();
    let ns = n.style.clone().unwrap_or_default();
    let loc = model::find(&app.doc, &id);
    let is_lvl1 = loc.as_ref().is_some_and(|l| l.depth() == 1 && !l.floating()) && app.doc.settings.layout == "mindmap";
    ui.horizontal(|ui| {
        h4(ui, "Shape");
        if ui.small_button("Reset style").clicked() {
            app.for_selected(|n| n.style = None);
            app.commit(false);
        }
    });
    let mut pick: Option<String> = None;
    egui::Grid::new("shapes").spacing(vec2(4.0, 4.0)).show(ui, |ui| {
        for (i, s) in SHAPES.iter().enumerate() {
            if shape_icon(ui, s.id, st.shape == s.id, s.name) {
                pick = Some(s.id.to_string());
            }
            if i % 4 == 3 {
                ui.end_row();
            }
        }
    });
    if let Some(s) = pick {
        app.style(move |x| x.shape = Some(s.clone()));
    }
    h4(ui, "Fill");
    if let Some(c) = swatches(ui, "fill", ns.fill.as_deref(), SWATCHES) {
        app.style(move |x| x.fill = Some(c.clone()));
    }
    h4(ui, "Text");
    ui.horizontal(|ui| {
        for (k, l) in [("bold", "B"), ("italic", "I"), ("underline", "U"), ("strike", "S")] {
            let on = match k {
                "bold" => st.bold,
                "italic" => st.italic,
                "underline" => st.underline,
                _ => st.strike,
            };
            let mut txt = RichText::new(l).strong();
            if k == "italic" {
                txt = txt.italics();
            }
            if k == "underline" {
                txt = txt.underline();
            }
            if k == "strike" {
                txt = txt.strikethrough();
            }
            if ui.selectable_label(on, txt).clicked() {
                app.toggle_style(k);
            }
        }
        ui.separator();
        if let Some(a) = seg(ui, &st.align.as_str(), &[("left", "⟸"), ("center", "≡"), ("right", "⟹")]) {
            let a = a.to_string();
            app.style(move |x| x.align = Some(a.clone()));
        }
    });
    slider(app, ui, "Size", st.font_size, 9.0..=60.0, 1.0, |app, v| {
        app.for_selected(|n| n.style.get_or_insert_with(Default::default).font_size = Some(v));
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new("Font"));
        let cur = ns.font.clone().unwrap_or_default();
        let name = FONTS.iter().find(|f| f.id == cur).map_or("Map default", |f| f.name);
        egui::ComboBox::from_id_salt("node-font").selected_text(name).show_ui(ui, |ui| {
            if ui.selectable_label(cur.is_empty(), "Map default").clicked() {
                app.style(|x| x.font = None);
            }
            for f in FONTS {
                if ui.selectable_label(cur == f.id, f.name).clicked() {
                    let id = f.id.to_string();
                    app.style(move |x| x.font = Some(id.clone()));
                }
            }
        });
    });
    let is_root = loc.as_ref().is_some_and(|l| l.depth() == 0 && !l.floating());
    let wrap = ns.width.unwrap_or(app.doc.settings.max_width + if is_root { 80.0 } else { 0.0 });
    slider(app, ui, "Wrap width", wrap, 80.0..=700.0, 10.0, |app, v| {
        app.for_selected(|n| n.style.get_or_insert_with(Default::default).width = Some(v));
    });
    h4(ui, "Text colour");
    if let Some(c) = swatches(
        ui,
        "color",
        ns.color.as_deref(),
        &["#000000", "#343a40", "#868e96", "#ffffff", "#e03131", "#f76707", "#f59f00", "#2f9e44", "#0c8599", "#1c7ed6", "#6741d9", "#c2255c"],
    ) {
        app.style(move |x| x.color = Some(c.clone()));
    }
    h4(ui, "Border");
    if let Some(c) = swatches(ui, "border", ns.border.as_deref(), SWATCHES) {
        app.style(move |x| x.border = Some(c.clone()));
    }
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new("Width"));
        let cur = ns.border_width.map(|w| w as i64);
        if let Some(v) = seg(ui, &cur, &[(None, "Auto"), (Some(0), "0"), (Some(1), "1"), (Some(2), "2"), (Some(3), "3"), (Some(4), "4")]) {
            app.style(move |x| x.border_width = v.map(|w| w as f64));
        }
    });
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new("Style"));
        if let Some(v) = seg(ui, &st.border_dash.as_str(), &[("solid", "Solid"), ("dashed", "Dashed"), ("dotted", "Dotted")]) {
            app.style(move |x| x.border_dash = if v == "solid" { None } else { Some(v.to_string()) });
        }
    });
    h4(ui, "Branch line colour");
    if let Some(c) = swatches(
        ui,
        "line",
        ns.line_color.as_deref(),
        &["#adb5bd", "#495057", "#e03131", "#f76707", "#f59f00", "#2f9e44", "#0c8599", "#1c7ed6", "#6741d9", "#c2255c"],
    ) {
        app.style(move |x| x.line_color = Some(c.clone()));
    }
    h4(ui, "Task");
    if let Some(v) = seg(ui, &n.task.clone().unwrap_or_default().as_str(), &[("", "None"), ("todo", "☐ To do"), ("done", "☑ Done")]) {
        app.for_selected(|x| x.task = if v.is_empty() { None } else { Some(v.to_string()) });
        app.commit(false);
    }
    h4(ui, "Labels");
    text_field(app, ui, "tags", n.tags.clone().unwrap_or_default().join(", "), "Comma separated, e.g. urgent, Q3", |n, v| {
        let tags: Vec<String> = v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
        n.tags = if tags.is_empty() { None } else { Some(tags) };
    });
    h4(ui, "Hyperlink");
    ui.horizontal(|ui| {
        text_field(app, ui, "link", n.link.clone().unwrap_or_default(), "https://…", |n, v| n.link = clean_link(v));
        if n.link.is_some() && ui.button("Open").clicked() {
            app.open_link();
        }
    });
    h4(ui, "Image");
    match &n.image {
        Some(im) => {
            slider(app, ui, "Width", im.w, 40.0..=600.0, 10.0, |app, v| {
                app.for_selected(|n| {
                    if let Some(im) = &mut n.image {
                        let ratio = im.h / im.w.max(1.0);
                        im.w = v;
                        im.h = (v * ratio).round();
                    }
                });
            });
            ui.horizontal(|ui| {
                if ui.button("Replace…").clicked() {
                    app.pick_image();
                }
                if ui.button(RichText::new("Remove").color(Color32::from_rgb(0xe0, 0x31, 0x31))).clicked() {
                    app.for_selected(|n| n.image = None);
                    app.commit(false);
                }
            });
        }
        None => {
            ui.horizontal(|ui| {
                if ui.button("Insert image…").clicked() {
                    app.pick_image();
                }
                ui.label(RichText::new("or paste / drop one").weak());
            });
        }
    }
    h4(ui, "Boundary");
    if let Some(v) = seg(ui, &n.has_boundary(), &[(false, "Off"), (true, "On")]) {
        app.for_selected(|x| {
            x.boundary = v.then_some(true);
            if !v {
                x.boundary_label = None;
            }
        });
        app.commit(false);
    }
    if n.has_boundary() {
        text_field(app, ui, "blabel", n.boundary_label.clone().unwrap_or_default(), "Boundary title (optional)", |n, v| {
            n.boundary_label = Some(v.trim().to_string()).filter(|s| !s.is_empty());
        });
    }
    if is_lvl1 {
        h4(ui, "Side of the map");
        if let Some(v) = seg(ui, &n.side.clone().unwrap_or_default().as_str(), &[("", "Auto"), ("left", "Left"), ("right", "Right")]) {
            if let Some(x) = model::get_mut(&mut app.doc, &id) {
                x.side = if v.is_empty() { None } else { Some(v.to_string()) };
            }
            app.commit(true);
        }
    }
}

fn clean_link(v: &str) -> Option<String> {
    let v = v.trim();
    if v.is_empty() {
        return None;
    }
    let has_scheme = v.split_once(':').is_some_and(|(s, _)| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphabetic()));
    Some(if has_scheme { v.to_string() } else { format!("https://{v}") })
}

/// A text field bound to the selected topics, applied when it loses focus or on Enter.
fn text_field(app: &mut App, ui: &mut Ui, key: &str, value: String, hint: &str, apply: impl Fn(&mut model::Node, &str)) {
    let id = Id::new("field").with(key).with(app.primary());
    let mut buf = ui.data_mut(|d| d.get_temp::<String>(id)).unwrap_or(value.clone());
    let r = ui.add(egui::TextEdit::singleline(&mut buf).hint_text(hint).desired_width(f32::INFINITY));
    if r.has_focus() || r.changed() {
        ui.data_mut(|d| d.insert_temp(id, buf.clone()));
    }
    if r.lost_focus() {
        ui.data_mut(|d| d.remove::<String>(id));
        if buf != value {
            app.for_selected(|n| apply(n, &buf));
            app.commit(false);
        }
    }
}

/// A small sample map shown in the Map tab's style cards.
fn sample_doc() -> Doc {
    let mut d = Doc::new("Idea");
    let r = d.root.id.clone();
    let a = model::add_child(&mut d, &r, "Plan", None).unwrap_or_default();
    let b = model::add_child(&mut d, &r, "Build", None).unwrap_or_default();
    let c = model::add_child(&mut d, &r, "Share", None).unwrap_or_default();
    model::add_child(&mut d, &a, "Goals", None);
    model::add_child(&mut d, &b, "Code", None);
    model::add_child(&mut d, &b, "Test", None);
    model::add_child(&mut d, &c, "Blog", None);
    let e = model::add_child(&mut d, &r, "Learn", None).unwrap_or_default();
    model::add_child(&mut d, &e, "Books", None);
    d
}

/// Paint a whole map, fitted, into `r`.
pub fn thumb(app: &mut App, ui: &Ui, r: egui::Rect, doc: &Doc, pad: f32) {
    let theme = themes::get_theme(&doc.settings.theme);
    let painter = ui.painter_at(r);
    painter.rect_filled(r, CornerRadius::same(6), canvas::color(theme.bg));
    let App { fonts, images, measure, ctx, .. } = app;
    let m = crate::state::measure_fn(ctx, fonts, measure);
    let lctx = mindloom_core::layout::Ctx { theme, settings: &doc.settings, measure: &m, focus_id: None, ignore_offset_id: None };
    let l = mindloom_core::layout::layout(doc, &lctx);
    let b = l.bounds;
    let z = ((r.width() - pad * 2.0) / b.w.max(1.0) as f32).min((r.height() - pad * 2.0) / b.h.max(1.0) as f32);
    let off = vec2(r.center().x - (b.min_x + b.w / 2.0) as f32 * z, r.center().y - (b.min_y + b.h / 2.0) as f32 * z);
    let mut c = Canvas { painter: &painter, view: View { zoom: z, off }, fonts, images };
    let none = HashSet::new();
    let o =
        PaintOpts { theme, settings: &doc.settings, selected: &[], matches: &[], selected_rel: None, faded: &none, hover: None, editing: None, export: true };
    canvas::paint(&mut c, &l, doc, &o);
}

fn card(app: &mut App, ui: &mut Ui, key: String, make: impl FnOnce(&App) -> Doc, size: egui::Vec2, on: bool, name: &str) -> bool {
    let (r, resp) = ui.allocate_exact_size(size + vec2(0.0, 18.0), Sense::click());
    if !app.previews.contains_key(&key) {
        let d = make(app);
        app.previews.insert(key.clone(), d);
    }
    let doc = app.previews.get(&key).cloned();
    let img = egui::Rect::from_min_size(r.min, size);
    if let Some(d) = doc {
        thumb(app, ui, img, &d, 6.0);
    }
    let st = if on {
        Stroke::new(2.0, ACCENT)
    } else if resp.hovered() {
        Stroke::new(1.0, ui.visuals().widgets.hovered.fg_stroke.color)
    } else {
        Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color)
    };
    ui.painter().rect_stroke(img, CornerRadius::same(6), st, StrokeKind::Inside);
    ui.painter().text(pos2(r.center().x, r.bottom() - 8.0), Align2::CENTER_CENTER, name, FontId::proportional(11.0), ui.visuals().text_color());
    resp.clicked()
}

fn set_settings(app: &mut App, f: impl FnOnce(&mut model::Settings)) {
    f(&mut app.doc.settings);
    app.commit(false);
    app.pending_fit = true;
}

fn map_tab(app: &mut App, ui: &mut Ui) {
    let s = app.doc.settings.clone();
    let w = (ui.available_width() - 12.0) / 3.0;
    h4(ui, "Map style");
    let mut pick: Option<usize> = None;
    egui::Grid::new("mapstyles").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, ms) in MAP_STYLES.iter().enumerate() {
            let on =
                s.layout == ms.layout && s.line_style == ms.line_style && [s.shapes.root.as_str(), s.shapes.main.as_str(), s.shapes.sub.as_str()] == ms.shapes;
            let key = format!("ms:{}:{}", ms.id, s.theme);
            let theme = s.theme.clone();
            let font = ms.font.unwrap_or(&s.font).to_string();
            if card(
                app,
                ui,
                key,
                move |_| {
                    let mut d = sample_doc();
                    d.settings.layout = ms.layout.into();
                    d.settings.line_style = ms.line_style.into();
                    d.settings.shapes.root = ms.shapes[0].into();
                    d.settings.shapes.main = ms.shapes[1].into();
                    d.settings.shapes.sub = ms.shapes[2].into();
                    d.settings.theme = theme;
                    d.settings.font = font;
                    d.settings.pattern = "none".into();
                    d
                },
                vec2(w, 54.0),
                on,
                ms.name,
            ) {
                pick = Some(i);
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
    if let Some(i) = pick {
        let ms = &MAP_STYLES[i];
        set_settings(app, |s| {
            s.layout = ms.layout.into();
            s.line_style = ms.line_style.into();
            s.shapes.root = ms.shapes[0].into();
            s.shapes.main = ms.shapes[1].into();
            s.shapes.sub = ms.shapes[2].into();
            s.font = ms.font.unwrap_or("system").into();
        });
    }
    h4(ui, "Colour theme");
    let w2 = (ui.available_width() - 6.0) / 2.0;
    let mut pick: Option<&str> = None;
    egui::Grid::new("themes").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, th) in THEMES.iter().enumerate() {
            let key = format!("th:{}:{}:{}:{}:{}", th.id, s.line_style, s.shapes.root, s.shapes.main, s.shapes.sub);
            let (ls, sh) = (s.line_style.clone(), s.shapes.clone());
            if card(
                app,
                ui,
                key,
                move |_| {
                    let mut d = sample_doc();
                    d.settings.theme = th.id.into();
                    d.settings.line_style = ls;
                    d.settings.shapes = sh;
                    d.settings.pattern = "none".into();
                    d
                },
                vec2(w2, 56.0),
                s.theme == th.id,
                th.name,
            ) {
                pick = Some(th.id);
            }
            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    if let Some(t) = pick {
        set_settings(app, |s| {
            s.theme = t.into();
            s.rainbow = None;
        });
    }
    h4(ui, "Structure");
    let mut pick: Option<&str> = None;
    egui::Grid::new("layouts").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, l) in LAYOUTS.iter().enumerate() {
            let name = l.name.split(" (").next().unwrap_or(l.name);
            if card(
                app,
                ui,
                format!("lay:{}", l.id),
                move |_| {
                    let mut d = sample_doc();
                    d.settings.layout = l.id.into();
                    d.settings.theme = "classic".into();
                    d.settings.line_style = if l.id == "down" || l.id == "up" { "elbow" } else { "curve" }.into();
                    d.settings.pattern = "none".into();
                    d
                },
                vec2(w, 44.0),
                s.layout == l.id,
                name,
            ) {
                pick = Some(l.id);
            }
            if i % 3 == 2 {
                ui.end_row();
            }
        }
    });
    if let Some(l) = pick {
        set_settings(app, |s| s.layout = l.into());
    }
    h4(ui, "Branch lines");
    let opts: Vec<(&str, &str)> = LINE_STYLES.iter().map(|l| (l.id, l.name.split(' ').next().unwrap_or(l.name))).collect();
    if let Some(v) = seg(ui, &s.line_style.as_str(), &opts) {
        set_settings(app, |s| s.line_style = v.into());
    }
    slider(app, ui, "Thickness", s.line_width, 1.0..=6.0, 0.5, |app, v| app.doc.settings.line_width = v);
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new("Rainbow"));
        if let Some(v) = seg(ui, &s.rainbow, &[(None, "Theme"), (Some(true), "On"), (Some(false), "Off")]) {
            set_settings(app, |s| s.rainbow = v);
        }
    });
    h4(ui, "Topic shapes");
    for (lvl, label) in [("root", "Central"), ("main", "Main"), ("sub", "Subtopics")] {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(78.0, 18.0), egui::Label::new(label));
            let cur = match lvl {
                "root" => s.shapes.root.clone(),
                "main" => s.shapes.main.clone(),
                _ => s.shapes.sub.clone(),
            };
            let name = SHAPES.iter().find(|x| x.id == cur).map_or("Rounded", |x| x.name);
            egui::ComboBox::from_id_salt(format!("shape-{lvl}")).selected_text(name).show_ui(ui, |ui| {
                for sh in SHAPES {
                    if ui.selectable_label(cur == sh.id, sh.name).clicked() {
                        set_settings(app, |s| match lvl {
                            "root" => s.shapes.root = sh.id.into(),
                            "main" => s.shapes.main = sh.id.into(),
                            _ => s.shapes.sub = sh.id.into(),
                        });
                    }
                }
            });
        });
    }
    h4(ui, "Background");
    let theme = app.theme();
    let w4 = (ui.available_width() - 18.0) / 4.0;
    egui::Grid::new("patterns").spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        for (i, p) in PATTERNS.iter().enumerate() {
            let (r, resp) = ui.allocate_exact_size(vec2(w4, 52.0), Sense::click());
            let img = egui::Rect::from_min_size(r.min, vec2(w4, 34.0));
            canvas::paint_background(&ui.painter_at(img), img, View { zoom: 1.0, off: img.min.to_vec2() }, p.id, theme);
            let on = s.pattern == p.id;
            ui.painter().rect_stroke(
                img,
                CornerRadius::same(5),
                if on { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color) },
                StrokeKind::Inside,
            );
            ui.painter().text(pos2(r.center().x, r.bottom() - 8.0), Align2::CENTER_CENTER, p.name, FontId::proportional(10.5), ui.visuals().text_color());
            if resp.clicked() {
                set_settings(app, |s| s.pattern = p.id.into());
            }
            if i % 4 == 3 {
                ui.end_row();
            }
        }
    });
    h4(ui, "Typography & spacing");
    ui.horizontal(|ui| {
        ui.add_sized(vec2(78.0, 18.0), egui::Label::new("Font"));
        let name = FONTS.iter().find(|f| f.id == s.font).map_or("System", |f| f.name);
        egui::ComboBox::from_id_salt("map-font").selected_text(name).show_ui(ui, |ui| {
            for f in FONTS {
                if ui.selectable_label(s.font == f.id, f.name).clicked() {
                    set_settings(app, |s| s.font = f.id.into());
                }
            }
        });
    });
    slider(app, ui, "Text size", s.font_scale, 0.7..=1.6, 0.05, |app, v| app.doc.settings.font_scale = v);
    slider(app, ui, "Horizontal", s.h_gap, 20.0..=160.0, 2.0, |app, v| app.doc.settings.h_gap = v);
    slider(app, ui, "Vertical", s.v_gap, 2.0..=60.0, 1.0, |app, v| app.doc.settings.v_gap = v);
    slider(app, ui, "Topic width", s.max_width, 100.0..=600.0, 10.0, |app, v| app.doc.settings.max_width = v);
    ui.add_space(8.0);
    ui.horizontal_wrapped(|ui| {
        if ui.button("Reset hand-placed topics").clicked() {
            app.reset_all_positions();
        }
        if ui.button("Clear custom topic styles").clicked() {
            model::walk_doc_mut(&mut app.doc, &mut |n, _| n.style = None);
            app.commit(false);
        }
    });
}

fn markers_tab(app: &mut App, ui: &mut Ui) {
    let have: HashSet<String> = app.primary().and_then(|p| model::get(&app.doc, &p).and_then(|n| n.icons.clone())).unwrap_or_default().into_iter().collect();
    if app.primary().is_none() {
        ui.label(RichText::new("Select one or more topics, then click a marker to add or remove it.").weak());
    }
    let mut pick: Option<String> = None;
    for g in MARKER_GROUPS {
        h4(ui, g.name);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for m in g.items {
                let (r, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let on = have.contains(*m);
                let bg = if on {
                    alpha(ACCENT, 0.18)
                } else if resp.hovered() {
                    ui.visuals().widgets.hovered.weak_bg_fill
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect(r, CornerRadius::same(6), bg, if on { Stroke::new(1.5, ACCENT) } else { Stroke::NONE }, StrokeKind::Inside);
                let painter = ui.painter().clone();
                let App { fonts, images, .. } = &mut *app;
                let mut c = Canvas { painter: &painter, view: View { zoom: 1.0, off: egui::Vec2::ZERO }, fonts, images };
                canvas::paint_icon(&mut c, &Icon::Marker(m.to_string()), (r.left() + 5.0) as f64, (r.top() + 5.0) as f64, 20.0, ui.visuals().text_color());
                if resp.on_hover_text(*m).clicked() {
                    pick = Some(m.to_string());
                }
            }
        });
    }
    if let Some(m) = pick {
        app.marker(&m);
    }
    h4(ui, "Task checkbox");
    if ui.button("Toggle task (☐ → ☑ → none)").clicked() {
        app.task();
    }
    ui.add_space(6.0);
    if ui.button(RichText::new("Remove all markers from selection").color(Color32::from_rgb(0xe0, 0x31, 0x31))).clicked() {
        app.for_selected(|n| n.icons = None);
        app.commit(false);
    }
}

fn notes_tab(app: &mut App, ui: &mut Ui) {
    let Some(id) = app.primary() else {
        ui.label(RichText::new("Select a topic to write notes for it. Topics with notes show a note icon; click it to open them here.").weak());
        return;
    };
    let Some(n) = model::get(&app.doc, &id).cloned() else { return };
    let title: String = if n.text.is_empty() { "Topic".into() } else { n.text.chars().take(40).collect() };
    h4(ui, &format!("Notes for “{title}”"));
    let mut note = n.note.clone().unwrap_or_default();
    let r = ui.add(
        egui::TextEdit::multiline(&mut note)
            .id(Id::new("note-area").with(&id))
            .hint_text("Write details, ideas, links…")
            .desired_width(f32::INFINITY)
            .desired_rows(14),
    );
    if app.note_focus {
        r.request_focus();
        app.note_focus = false;
    }
    if r.changed() {
        if let Some(x) = model::get_mut(&mut app.doc, &id) {
            x.note = if note.is_empty() { None } else { Some(note) };
        }
        app.refresh_geo();
    }
    if r.lost_focus() {
        app.commit(false);
    }
    ui.label(RichText::new("Notes are exported to Markdown, OPML and FreeMind.").weak().small());
}

// ---------- keyboard ----------

fn shortcuts(app: &mut App, ctx: &egui::Context) {
    if app.dialog.is_some() {
        return;
    }
    if let Some(_p) = &app.present {
        let (next, prev, exit) = ctx.input_mut(|i| {
            let next = [Key::ArrowRight, Key::Space, Key::PageDown, Key::Enter].iter().any(|k| i.consume_key(Modifiers::NONE, *k));
            let prev = [Key::ArrowLeft, Key::PageUp, Key::Backspace].iter().any(|k| i.consume_key(Modifiers::NONE, *k));
            (next, prev, i.consume_key(Modifiers::NONE, Key::Escape))
        });
        if next {
            app.present_step(1);
        }
        if prev {
            app.present_step(-1);
        }
        if exit {
            app.present_exit();
        }
        return;
    }
    if app.editing.is_some() {
        return;
    }
    let typing = ctx.egui_wants_keyboard_input();
    let cmd = Modifiers::COMMAND;
    let sc = Modifiers::COMMAND | Modifiers::SHIFT;
    let ac = Modifiers::COMMAND | Modifiers::ALT;
    let k = |m: Modifiers, key: Key| ctx.input_mut(|i| i.consume_key(m, key));
    // work in text fields too
    if k(sc, Key::S) {
        app.save(true);
    } else if k(cmd, Key::S) {
        app.save(false);
    }
    if typing {
        return;
    }
    // clipboard
    let events = ctx.input(|i| i.events.clone());
    for e in &events {
        match e {
            Event::Copy => {
                if let Some(t) = app.copy() {
                    ctx.copy_text(t);
                }
            }
            Event::Cut => {
                if let Some(t) = app.cut() {
                    ctx.copy_text(t);
                }
            }
            Event::Paste(t) => {
                if t.trim().is_empty() {
                    paste_from_system(app);
                } else {
                    app.paste(Some(t));
                }
            }
            _ => {}
        }
    }
    if k(cmd, Key::V) {
        paste_from_system(app);
    }
    if k(sc, Key::Z) || k(cmd, Key::Y) {
        app.redo();
    } else if k(cmd, Key::Z) {
        app.undo();
    }
    if k(cmd, Key::O) {
        app.guard(After::Open);
    }
    if k(sc, Key::N) {
        new_window();
    } else if k(cmd, Key::N) {
        app.dialog = Some(Dialog::Templates);
    }
    if k(sc, Key::F) {
        app.focus();
    } else if k(cmd, Key::F) {
        open_search(app);
    }
    if k(cmd, Key::Equals) || k(cmd, Key::Plus) || k(sc, Key::Equals) {
        app.zoom_at(1.2, None);
    }
    if k(cmd, Key::Minus) {
        app.zoom_at(1.0 / 1.2, None);
    }
    if k(cmd, Key::Num0) {
        app.fit(true);
    }
    if k(ac, Key::C) {
        app.copy_style();
    }
    if k(ac, Key::V) {
        app.paste_style();
    }
    if k(cmd, Key::D) {
        app.duplicate();
    }
    if k(sc, Key::B) {
        app.boundary();
    } else if k(cmd, Key::B) {
        app.toggle_style("bold");
    }
    if k(cmd, Key::I) {
        app.toggle_style("italic");
    }
    if k(cmd, Key::U) {
        app.toggle_style("underline");
    }
    if k(cmd, Key::K) {
        link_dialog(app);
    }
    if k(sc, Key::M) {
        show_note(app);
    }
    if k(sc, Key::R) {
        app.relation();
    }
    if k(cmd, Key::E) {
        app.toast("Choose a format from the Export menu.");
    }
    if k(sc, Key::T) {
        app.task();
    }
    if k(cmd, Key::Slash) {
        app.toggle_collapse();
    }
    if k(sc, Key::Backslash) {
        toggle_inspector(app);
    } else if k(cmd, Key::Backslash) {
        toggle_outline(app);
    }
    if k(cmd, Key::Enter) {
        app.add_parent();
    }
    for (n, key) in [(1, Key::Num1), (2, Key::Num2), (3, Key::Num3), (4, Key::Num4), (5, Key::Num5), (6, Key::Num6)] {
        if k(cmd, key) {
            app.marker(&format!("p{n}"));
        }
    }
    if k(cmd, Key::ArrowUp) || k(Modifiers::ALT, Key::ArrowUp) {
        app.reorder(-1);
    }
    if k(cmd, Key::ArrowDown) || k(Modifiers::ALT, Key::ArrowDown) {
        app.reorder(1);
    }
    let sa = Modifiers::SHIFT | Modifiers::ALT;
    for (key, dx, dy) in [(Key::ArrowLeft, -10.0, 0.0), (Key::ArrowRight, 10.0, 0.0), (Key::ArrowUp, 0.0, -10.0), (Key::ArrowDown, 0.0, 10.0)] {
        if k(sa, key) {
            app.nudge(dx, dy);
        }
    }
    if k(Modifiers::ALT, Key::ArrowRight) {
        app.indent(false);
    }
    if k(Modifiers::ALT, Key::ArrowLeft) {
        app.indent(true);
    }
    if k(Modifiers::SHIFT, Key::Tab) {
        app.add_parent();
    }
    if k(Modifiers::NONE, Key::Tab) || k(Modifiers::NONE, Key::Insert) {
        app.add_child();
    }
    if k(Modifiers::SHIFT, Key::Enter) {
        app.add_sibling(true);
    } else if k(Modifiers::NONE, Key::Enter) {
        app.add_sibling(false);
    }
    if k(Modifiers::NONE, Key::Delete) || k(Modifiers::NONE, Key::Backspace) {
        app.delete_sel();
    }
    let space = k(Modifiers::NONE, Key::Space);
    if k(Modifiers::NONE, Key::F2) || space {
        app.edit_primary();
    }
    if k(Modifiers::NONE, Key::F5) {
        app.present();
    }
    if k(Modifiers::NONE, Key::F1) {
        app.dialog = Some(Dialog::Shortcuts);
    }
    if k(Modifiers::NONE, Key::Escape) {
        if app.drag.is_some() {
            app.cancel_drag();
        } else if app.relation_from.is_some() {
            app.relation_from = None;
        } else if app.search_open {
            app.close_search();
        } else if app.sel_rel.is_some() {
            app.sel_rel = None;
        } else if app.focus_id.is_some() {
            app.focus();
        }
    }
    for (key, dir) in [(Key::ArrowRight, "right"), (Key::ArrowLeft, "left"), (Key::ArrowUp, "up"), (Key::ArrowDown, "down")] {
        if k(Modifiers::NONE, key) {
            app.navigate(dir);
        }
    }
    if k(Modifiers::NONE, Key::Home) {
        let id = app.focus_id.clone().unwrap_or_else(|| app.doc.root.id.clone());
        app.select(vec![id.clone()], false, false);
        app.center_on(&id, true);
    }
    // start typing on a selected topic → replace its text
    if !space && app.sel.len() == 1 && app.editing.is_none() {
        let typed: String = events.iter().filter_map(|e| if let Event::Text(t) = e { Some(t.as_str()) } else { None }).collect::<String>();
        let mods = ctx.input(|i| i.modifiers);
        if !typed.is_empty() && typed != " " && !mods.command && !mods.ctrl {
            let id = app.sel[0].clone();
            app.start_edit(&id, Some(typed));
        }
    }
}

// ---------- dialogs ----------

pub struct AiForm {
    pub cfg: ai::Config,
    pub mode: ai::Mode,
    pub depth: String,
    pub topic: String,
    pub text: String,
    pub into: bool,
    pub key: String,
    pub base: String,
    pub model: String,
    pub models: Vec<ai::Model>,
    pub status: String,
    pub error: bool,
    pub busy: bool,
    pub rx: Option<Receiver<ai::Done>>,
    pub has_key: HashMap<String, bool>,
    pub replacing: bool,
    pub node: Option<String>,
    pub loaded_for: String,
}

impl AiForm {
    pub fn new(node: Option<String>, expand: bool) -> AiForm {
        let cfg = ai::load_config();
        let mut f = AiForm {
            mode: if expand && node.is_some() { ai::Mode::Expand } else { ai::Mode::Generate },
            depth: "standard".into(),
            topic: String::new(),
            text: String::new(),
            into: false,
            key: String::new(),
            base: String::new(),
            model: String::new(),
            models: vec![],
            status: String::new(),
            error: false,
            busy: false,
            rx: None,
            has_key: HashMap::new(),
            replacing: false,
            node,
            loaded_for: String::new(),
            cfg,
        };
        f.select_provider();
        f
    }
    fn provider(&self) -> &'static ai::Provider {
        ai::provider(&self.cfg.provider).unwrap_or(&ai::PROVIDERS[0])
    }
    fn select_provider(&mut self) {
        let p = self.provider();
        self.has_key.insert(p.id.into(), ai::get_key(p.id).is_some());
        self.base = self.cfg.base_urls.get(p.id).cloned().unwrap_or_else(|| p.base.to_string());
        self.model = self.cfg.models.get(p.id).cloned().unwrap_or_else(|| p.default_model.to_string());
        self.models.clear();
        self.status.clear();
        self.error = false;
        self.replacing = false;
        self.key.clear();
    }
    fn ready(&self) -> bool {
        let p = self.provider();
        p.no_key || self.has_key.get(p.id).copied().unwrap_or(false) || (p.key_optional && !self.base.is_empty())
    }
    fn say(&mut self, s: impl Into<String>, err: bool) {
        self.status = s.into();
        self.error = err;
    }
}

fn dialogs(app: &mut App, ctx: &egui::Context) {
    let Some(dialog) = app.dialog.take() else { return };
    let mut keep = true;
    let mut next: Option<Dialog> = None;
    let dialog = match dialog {
        Dialog::Templates => {
            let r = egui::Modal::new(Id::new("templates")).show(ctx, |ui| {
                ui.set_width(760.0);
                ui.heading("Create a new mind map");
                ui.add_space(6.0);
                let mut chosen: Option<String> = None;
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    egui::Grid::new("tpl").spacing(vec2(10.0, 10.0)).show(ui, |ui| {
                        for (i, t) in TEMPLATES.iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(vec2(240.0, 170.0), Sense::click());
                            let img = egui::Rect::from_min_size(r.min, vec2(240.0, 120.0));
                            let key = format!("tpl:{}", t.id);
                            if !app.previews.contains_key(&key) {
                                app.previews.insert(key.clone(), mindloom_core::templates::build(t.id));
                            }
                            if let Some(d) = app.previews.get(&key).cloned() {
                                thumb(app, ui, img, &d, 10.0);
                            }
                            let st =
                                if resp.hovered() { Stroke::new(2.0, ACCENT) } else { Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color) };
                            ui.painter().rect_stroke(r, CornerRadius::same(8), st, StrokeKind::Inside);
                            ui.painter().text(
                                pos2(r.left() + 10.0, r.top() + 132.0),
                                Align2::LEFT_CENTER,
                                format!("{} {}", t.icon, t.name),
                                FontId::new(13.0, egui::FontFamily::Name("ui-bold".into())),
                                ui.visuals().text_color(),
                            );
                            let clip = ui.painter().with_clip_rect(r.shrink(4.0));
                            clip.text(
                                pos2(r.left() + 10.0, r.top() + 154.0),
                                Align2::LEFT_CENTER,
                                t.desc,
                                FontId::proportional(11.0),
                                ui.visuals().weak_text_color(),
                            );
                            if resp.clicked() {
                                chosen = Some(t.id.to_string());
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                    let rec = crate::files::recent();
                    if !rec.is_empty() {
                        ui.add_space(10.0);
                        ui.strong("Recent files");
                        for p in rec.into_iter().take(8) {
                            let name = std::path::Path::new(&p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                            if ui.link(format!("{name} — {p}")).clicked() {
                                next = Some(Dialog::SaveChanges(After::OpenPath(p.into())));
                            }
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Tip: you can also import Markdown, OPML, FreeMind and XMind files.").weak());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Open file…").clicked() {
                            next = Some(Dialog::SaveChanges(After::Open));
                        }
                        if ui.button("Cancel").clicked() {
                            keep = false;
                        }
                    });
                });
                chosen
            });
            if r.should_close() {
                keep = false;
            }
            if let Some(id) = r.inner {
                keep = false;
                app.guard(After::Template(id));
            }
            if let Some(Dialog::SaveChanges(a)) = next.take() {
                keep = false;
                app.guard(a);
            }
            Dialog::Templates
        }
        Dialog::Shortcuts => {
            let r = egui::Modal::new(Id::new("shortcuts")).show(ctx, |ui| {
                ui.set_width(640.0);
                ui.heading("Keyboard shortcuts");
                let rows = [
                    ("Add subtopic", "Tab".to_string()),
                    ("Add topic (sibling)", "Enter".into()),
                    ("Add topic before", "Shift+Enter".into()),
                    ("Add parent topic", kb("Mod+Enter")),
                    ("Edit topic", "F2 / Space / just type".into()),
                    ("New line while editing", "Shift+Enter".into()),
                    ("Delete topic", "Delete / Backspace".into()),
                    ("Navigate", "Arrow keys".into()),
                    ("Move topic up / down", "Alt+↑ / Alt+↓".into()),
                    ("Indent / outdent", "Alt+→ / Alt+←".into()),
                    ("Collapse / expand", kb("Mod+/")),
                    ("Duplicate", kb("Mod+D")),
                    ("Copy / cut / paste", kb("Mod+C / X / V")),
                    ("Undo / redo", kb("Mod+Z / Mod+Shift+Z")),
                    ("Bold / italic / underline", kb("Mod+B / I / U")),
                    ("Priority 1–6", kb("Mod+1 … 6")),
                    ("Task checkbox", kb("Mod+Shift+T")),
                    ("Hyperlink", kb("Mod+K")),
                    ("Note", kb("Mod+Shift+M")),
                    ("Relationship", kb("Mod+Shift+R")),
                    ("Boundary", kb("Mod+Shift+B")),
                    ("Focus on branch", kb("Mod+Shift+F")),
                    ("Find", kb("Mod+F")),
                    ("Zoom in / out", kb("Mod+= / Mod+-")),
                    ("Fit map", kb("Mod+0")),
                    ("Outline / format panel", kb("Mod+\\ / Mod+Shift+\\")),
                    ("Presentation", "F5".into()),
                    ("New / Open / Save", kb("Mod+N / O / S")),
                    ("Nudge a topic", "Shift+Alt+arrows".into()),
                    ("Copy / paste style", kb("Mod+Alt+C / Mod+Alt+V")),
                    ("Floating topic", "Double-click canvas".into()),
                    ("Select with a rectangle", "Shift+drag canvas".into()),
                    ("Pan", "Drag canvas / scroll".into()),
                    ("Zoom", kb("Mod+scroll / pinch")),
                ];
                egui::ScrollArea::vertical().max_height(520.0).show(ui, |ui| {
                    egui::Grid::new("kbd").num_columns(2).striped(true).spacing(vec2(24.0, 6.0)).show(ui, |ui| {
                        for (a, b) in rows {
                            ui.label(a);
                            ui.label(RichText::new(b).monospace());
                            ui.end_row();
                        }
                    });
                });
            });
            if r.should_close() {
                keep = false;
            }
            Dialog::Shortcuts
        }
        Dialog::About => {
            let r = egui::Modal::new(Id::new("about")).show(ctx, |ui| {
                ui.set_width(380.0);
                ui.heading("About Mindloom");
                ui.label("Mindloom — a fast, friendly mind mapping app for macOS and Windows.");
                ui.label(
                    RichText::new(format!(
                        "Version {}. Written in Rust. Open source under the MIT licence. Inter font © The Inter Project Authors (SIL OFL).",
                        env!("CARGO_PKG_VERSION")
                    ))
                    .weak(),
                );
                ui.add_space(6.0);
                if ui.button("OK").clicked() {
                    ui.close();
                }
            });
            if r.should_close() {
                keep = false;
            }
            Dialog::About
        }
        Dialog::SaveChanges(after) => {
            let mut choice: Option<u8> = None;
            let r = egui::Modal::new(Id::new("savechanges")).show(ctx, |ui| {
                ui.set_width(360.0);
                ui.heading("Save changes?");
                ui.label(format!("“{}” has unsaved changes.", app.name));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        choice = Some(0);
                    }
                    if ui.button(RichText::new("Don't save").color(Color32::from_rgb(0xe0, 0x31, 0x31))).clicked() {
                        choice = Some(1);
                    }
                    if ui.add(egui::Button::new(RichText::new("Save").color(Color32::WHITE)).fill(ACCENT)).clicked() {
                        choice = Some(2);
                    }
                });
            });
            if r.should_close() {
                choice = Some(0);
            }
            match choice {
                Some(0) => keep = false,
                Some(1) => {
                    keep = false;
                    app.dirty = false;
                    app.run_after(after.clone());
                }
                Some(_) => {
                    keep = false;
                    if app.save(false) {
                        app.run_after(after.clone());
                    }
                }
                None => {}
            }
            Dialog::SaveChanges(after)
        }
        Dialog::Link(mut v) => {
            let mut done: Option<bool> = None;
            let r = egui::Modal::new(Id::new("link")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading("Hyperlink");
                ui.label(RichText::new("Web address for this topic (leave empty to remove):").weak());
                let te = ui.add(egui::TextEdit::singleline(&mut v).hint_text("https://example.com").desired_width(f32::INFINITY));
                te.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = Some(true);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                });
            });
            if r.should_close() {
                done = Some(false);
            }
            if let Some(ok) = done {
                keep = false;
                if ok {
                    let url = clean_link(&v);
                    app.for_selected(|n| n.link = url.clone());
                    app.commit(true);
                }
            }
            Dialog::Link(v)
        }
        Dialog::RelLabel(mut v) => {
            let mut done: Option<bool> = None;
            let r = egui::Modal::new(Id::new("rellabel")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.heading("Relationship label");
                ui.label(RichText::new("Describe how these topics relate:").weak());
                let te = ui.add(egui::TextEdit::singleline(&mut v).hint_text("e.g. depends on").desired_width(f32::INFINITY));
                te.request_focus();
                if ui.input(|i| i.key_pressed(Key::Enter)) {
                    done = Some(true);
                }
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        done = Some(false);
                    }
                    if ui.button("OK").clicked() {
                        done = Some(true);
                    }
                });
            });
            if r.should_close() {
                done = Some(false);
            }
            if let Some(ok) = done {
                keep = false;
                if let (true, Some(rid)) = (ok, app.sel_rel.clone()) {
                    if let Some(x) = app.doc.relations.iter_mut().find(|x| x.id == rid) {
                        x.label = v.trim().to_string();
                    }
                    app.commit(false);
                }
            }
            Dialog::RelLabel(v)
        }
        Dialog::Ai(mut f) => {
            keep = ai_dialog(app, ctx, &mut f);
            Dialog::Ai(f)
        }
    };
    if keep && app.dialog.is_none() {
        app.dialog = Some(dialog);
    }
}

fn ai_dialog(app: &mut App, ctx: &egui::Context, f: &mut AiForm) -> bool {
    // results from the background request
    if let Some(rx) = &f.rx {
        if let Ok(done) = rx.try_recv() {
            f.rx = None;
            f.busy = false;
            match done {
                ai::Done::Models(pid, _) if pid != f.provider().id => {}
                ai::Done::Models(_, Ok(list)) => {
                    let n = list.len();
                    if f.model.is_empty() {
                        if let Some(m) = list.first() {
                            f.model = m.id.clone();
                            let pid = f.provider().id.to_string();
                            f.cfg.models.insert(pid, f.model.clone());
                            ai::save_config(&f.cfg);
                        }
                    }
                    let missing = !f.model.is_empty() && !list.iter().any(|m| m.id == f.model);
                    f.models = list;
                    f.say(
                        if n > 0 {
                            format!("{n} models available{}.", if missing { " — the current model is not in the list, but you can still use it" } else { "" })
                        } else {
                            "The provider returned no chat models. You can type a model name.".into()
                        },
                        false,
                    );
                }
                ai::Done::Models(_, Err(e)) => f.say(e, true),
                ai::Done::Generate(req, Ok(outline)) => {
                    let pid = req.provider.clone();
                    f.cfg.models.insert(pid, req.model.clone());
                    ai::save_config(&f.cfg);
                    apply_ai(app, f, &req, &outline);
                    return false;
                }
                ai::Done::Generate(_, Err(e)) => f.say(e, true),
            }
        }
    }
    let mut keep = true;
    let r = egui::Modal::new(Id::new("ai")).show(ctx, |ui| {
        ui.set_width(560.0);
        ui.horizontal(|ui| {
            ui.label(ic(icon::SPARKLE).size(20.0).color(ACCENT));
            ui.heading("AI assistant");
        });
        ui.add_space(6.0);
        let p = f.provider();
        egui::Grid::new("ai-grid").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
            ui.label("Provider");
            let mut pick: Option<&str> = None;
            egui::ComboBox::from_id_salt("ai-prov").width(330.0).selected_text(p.name).show_ui(ui, |ui| {
                for (g, label) in [(ai::Group::Cloud, "Cloud providers"), (ai::Group::Local, "On your computer"), (ai::Group::Custom, "Other")] {
                    ui.label(RichText::new(label).small().weak());
                    for x in ai::PROVIDERS.iter().filter(|x| x.group == g) {
                        if ui.selectable_label(x.id == p.id, x.name).clicked() {
                            pick = Some(x.id);
                        }
                    }
                }
            });
            if let Some(id) = pick {
                f.cfg.provider = id.into();
                ai::save_config(&f.cfg);
                f.select_provider();
            }
            ui.end_row();
            let p = f.provider();
            ui.label("API key");
            ui.horizontal(|ui| {
                if p.no_key {
                    ui.label(RichText::new("No key needed — it runs on your computer.").weak());
                    if ui.link(format!("Get {}", p.name.split(' ').next().unwrap_or(p.name))).clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(p.key_url));
                    }
                } else if f.has_key.get(p.id).copied().unwrap_or(false) && !f.replacing {
                    ui.label(RichText::new("✓ Key saved").color(Color32::from_rgb(0x2f, 0x9e, 0x44)).strong());
                    if ui.button("Replace").clicked() {
                        f.replacing = true;
                    }
                    if ui.button(RichText::new("Remove").color(Color32::from_rgb(0xe0, 0x31, 0x31))).clicked() {
                        ai::clear_key(p.id);
                        f.has_key.insert(p.id.into(), ai::get_key(p.id).is_some());
                        f.say("Key removed from this computer.", false);
                    }
                } else {
                    let hint = if p.key_hint.is_empty() { "Paste your API key".to_string() } else { p.key_hint.to_string() }
                        + if p.key_optional { " (optional)" } else { "" };
                    let te = ui.add(egui::TextEdit::singleline(&mut f.key).password(true).hint_text(hint).desired_width(220.0));
                    let enter = te.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
                    if ui.button("Save").clicked() || enter {
                        save_key(f);
                    }
                    if !p.key_url.is_empty() && ui.link("Get a key").clicked() {
                        ctx.open_url(egui::OpenUrl::new_tab(p.key_url));
                    }
                }
            });
            ui.end_row();
            if p.editable_base {
                ui.label("Address");
                let r = ui.add(egui::TextEdit::singleline(&mut f.base).hint_text("https://api.example.com/v1").desired_width(330.0));
                if r.lost_focus() {
                    let base = f.base.trim().to_string();
                    if base.is_empty() {
                        f.cfg.base_urls.remove(p.id);
                        ai::save_config(&f.cfg);
                    } else {
                        match ai::validate_base_url(&base) {
                            Ok(b) => {
                                f.cfg.base_urls.insert(p.id.into(), b.clone());
                                f.base = b;
                                ai::save_config(&f.cfg);
                                f.say("Address saved.", false);
                            }
                            Err(e) => f.say(e, true),
                        }
                    }
                }
                ui.end_row();
            }
            ui.label("Model");
            ui.horizontal(|ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut f.model).hint_text("Model name").desired_width(200.0));
                if r.lost_focus() {
                    f.cfg.models.insert(p.id.into(), f.model.trim().to_string());
                    ai::save_config(&f.cfg);
                }
                if !f.models.is_empty() {
                    let mut pick: Option<String> = None;
                    egui::ComboBox::from_id_salt("ai-models").width(30.0).selected_text("").show_ui(ui, |ui| {
                        for m in &f.models {
                            let label = if m.name != m.id { format!("{} — {}", m.id, m.name) } else { m.id.clone() };
                            if ui.selectable_label(m.id == f.model, label).clicked() {
                                pick = Some(m.id.clone());
                            }
                        }
                    });
                    if let Some(m) = pick {
                        f.model = m;
                        f.cfg.models.insert(p.id.into(), f.model.clone());
                        ai::save_config(&f.cfg);
                    }
                }
                if ui.add_enabled(!f.busy, egui::Button::new("Load models")).on_hover_text("Fetch the models your key can use").clicked() {
                    f.busy = true;
                    f.say("Loading models…", false);
                    f.rx = Some(ai::spawn(ai::Job::Models(p.id.into()), f.cfg.clone(), ctx.clone()));
                }
            });
            ui.end_row();
        });
        // load the model list once when a key is ready
        let p = f.provider();
        if f.loaded_for != p.id && f.ready() && !f.busy {
            f.loaded_for = p.id.into();
            f.busy = true;
            f.rx = Some(ai::spawn(ai::Job::Models(p.id.into()), f.cfg.clone(), ctx.clone()));
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.selectable_label(f.mode == ai::Mode::Generate, "From a topic").clicked() {
                f.mode = ai::Mode::Generate;
            }
            if ui.selectable_label(f.mode == ai::Mode::Text, "From my notes").clicked() {
                f.mode = ai::Mode::Text;
            }
            if ui
                .add_enabled(f.node.is_some(), egui::Button::selectable(f.mode == ai::Mode::Expand, "Expand selected topic"))
                .on_disabled_hover_text("Select a topic first")
                .clicked()
            {
                f.mode = ai::Mode::Expand;
            }
        });
        let mut go = false;
        match f.mode {
            ai::Mode::Generate => {
                let r =
                    ui.add(egui::TextEdit::singleline(&mut f.topic).hint_text("What is the map about? e.g. Launching a podcast").desired_width(f32::INFINITY));
                if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    go = true;
                }
            }
            ai::Mode::Text => {
                ui.add(
                    egui::TextEdit::multiline(&mut f.text)
                        .hint_text("Paste notes, an article, a transcript or meeting minutes. The AI turns them into a mind map.")
                        .desired_rows(7)
                        .desired_width(f32::INFINITY),
                );
            }
            ai::Mode::Expand => {
                let name = f
                    .node
                    .as_ref()
                    .and_then(|id| model::get(&app.doc, id))
                    .map(|n| if n.text.is_empty() { "Topic".into() } else { n.text.clone() })
                    .unwrap_or_default();
                ui.label(RichText::new(format!("Suggest new subtopics for “{name}”, using the rest of the map as context.")).weak());
            }
        }
        if f.mode != ai::Mode::Expand {
            ui.horizontal(|ui| {
                ui.label("Detail");
                for (v, l) in [("quick", "Quick"), ("standard", "Standard"), ("deep", "Deep")] {
                    if ui.selectable_label(f.depth == v, l).clicked() {
                        f.depth = v.into();
                    }
                }
            });
            ui.checkbox(&mut f.into, "Add to this map instead of starting a new one");
        }
        if !f.status.is_empty() {
            let t =
                if f.error { RichText::new(format!("⚠ {}", f.status)).color(Color32::from_rgb(0xe0, 0x31, 0x31)) } else { RichText::new(&f.status).weak() };
            ui.horizontal(|ui| {
                if f.busy {
                    ui.spinner();
                }
                ui.label(t);
            });
        }
        let p = f.provider();
        ui.label(
            RichText::new(if p.group == ai::Group::Local {
                "Your map stays on this computer: the request goes to the local model server only.".to_string()
            } else {
                format!(
                    "Your request is sent straight from this computer to {}. Keys are stored in the system keychain and are never shared with anyone else.",
                    p.name
                )
            })
            .small()
            .weak(),
        );
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                keep = false;
            }
            let label = if f.mode == ai::Mode::Expand { "Suggest subtopics" } else { "Generate map" };
            if ui.add_enabled(!f.busy, egui::Button::new(RichText::new(label).color(Color32::WHITE)).fill(ACCENT)).clicked() {
                go = true;
            }
        });
        if go && !f.busy {
            start_ai(app, f, ctx);
        }
    });
    if r.should_close() {
        keep = false;
    }
    keep
}

fn save_key(f: &mut AiForm) -> bool {
    let p = f.provider();
    if f.key.trim().is_empty() {
        return true;
    }
    match ai::set_key(p.id, &f.key) {
        Ok(()) => {
            f.key.clear();
            f.replacing = false;
            f.has_key.insert(p.id.into(), true);
            f.loaded_for.clear();
            f.say("Key saved securely.", false);
            true
        }
        Err(e) => {
            f.say(e, true);
            false
        }
    }
}

fn start_ai(app: &mut App, f: &mut AiForm, ctx: &egui::Context) {
    if !save_key(f) {
        return;
    }
    let p = f.provider();
    if !f.ready() {
        return f.say(format!("Paste your {} API key first.", p.name), true);
    }
    let model = f.model.trim().to_string();
    if model.is_empty() {
        return f.say("Choose a model first (press “Load models”).", true);
    }
    let mut req = ai::Request {
        provider: p.id.into(),
        model: model.clone(),
        mode: f.mode,
        depth: f.depth.clone(),
        topic: String::new(),
        text: String::new(),
        path: vec![],
        existing: vec![],
        outline: String::new(),
    };
    match f.mode {
        ai::Mode::Generate => {
            req.topic = f.topic.trim().to_string();
            if req.topic.is_empty() {
                return f.say("Type what the map is about.", true);
            }
        }
        ai::Mode::Text => {
            req.text = f.text.trim().to_string();
            if req.text.chars().count() < 20 {
                return f.say("Paste a bit more text (at least a sentence or two).", true);
            }
        }
        ai::Mode::Expand => {
            let Some(id) = f.node.clone() else { return };
            let Some(n) = model::get(&app.doc, &id) else { return };
            req.topic = n.text.clone();
            req.path = model::path_to(&app.doc, &id).iter().map(|x| x.text.clone()).collect();
            req.existing = n.children.iter().map(|c| c.text.clone()).collect();
            req.outline = mindloom_core::io::to_text(&app.doc).chars().take(6000).collect();
        }
    }
    f.busy = true;
    f.say(format!("Asking {} ({model})…", p.name), false);
    f.rx = Some(ai::spawn(ai::Job::Generate(Box::new(req)), f.cfg.clone(), ctx.clone()));
}

fn apply_ai(app: &mut App, f: &AiForm, req: &ai::Request, outline: &str) {
    if req.mode == ai::Mode::Expand {
        let Some(id) = f.node.clone() else { return };
        let (roots, _) = mindloom_core::io::parse_indented(outline);
        let mut added = 0;
        for r in roots {
            if model::insert_subtree(&mut app.doc, &id, &r, None).is_some() {
                added += 1;
            }
        }
        app.sel = vec![id];
        app.commit(true);
        app.toast(format!("Added {added} subtopic{}", if added == 1 { "" } else { "s" }));
        return;
    }
    let Ok(mut d) = mindloom_core::io::from_text(outline) else {
        return app.toast("The model's answer could not be turned into a map.");
    };
    if f.into {
        let target = app.primary().unwrap_or_else(|| app.doc.root.id.clone());
        if let Some(n) = model::insert_subtree(&mut app.doc, &target, &d.root, None) {
            app.sel = vec![n];
            app.commit(true);
            app.toast("Added the generated branch to your map");
        }
        return;
    }
    let s = &app.doc.settings;
    d.settings.theme = s.theme.clone();
    d.settings.layout = s.layout.clone();
    d.settings.line_style = s.line_style.clone();
    d.settings.shapes = s.shapes.clone();
    d.settings.pattern = s.pattern.clone();
    d.settings.font = s.font.clone();
    d.settings.rainbow = s.rainbow;
    let name = d.root.text.clone();
    app.guard(After::SetDoc(Box::new(d), name));
    if !app.dirty || app.dialog.is_none() {
        app.toast("Map generated — edit anything you like");
    }
}
