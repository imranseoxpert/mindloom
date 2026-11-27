//! Mindloom — a fast, friendly mind mapping app for macOS and Windows, written in Rust.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod ai;
mod canvas;
mod files;
mod fonts;
#[cfg(target_os = "macos")]
mod mac;
mod selftest;
mod stage;
mod state;
mod ui;

use state::{After, App, Dialog};
use std::path::PathBuf;

struct Mindloom {
    app: App,
    title: String,
    started: bool,
    test: Option<selftest::Tester>,
}

impl Mindloom {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let db = fonts::database();
        let (defs, map_fonts) = fonts::definitions(&db);
        cc.egui_ctx.set_fonts(defs);
        let app = App::new(cc.egui_ctx.clone(), map_fonts, db);
        #[cfg(target_os = "macos")]
        mac::set_context(&cc.egui_ctx);
        ui::apply_visuals(&cc.egui_ctx, &app.prefs.ui_theme);
        Mindloom { app, title: String::new(), started: false, test: selftest::Tester::from_env() }
    }
}

/// What the window opens with: a file named on the command line, a crashed session's unsaved work,
/// or the map that was open last time. Otherwise the welcome map.
fn startup(app: &mut App) {
    app.refresh_geo();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let fresh = args.iter().any(|a| a == "--new");
    if let Some(p) = args.iter().find(|a| !a.starts_with("--")).map(PathBuf::from) {
        if app.load_path(&p) {
            return;
        }
    }
    if fresh {
        app.dialog = Some(Dialog::Templates);
        return;
    }
    if let Some(a) = files::autosave_recover(&app.autosave_key) {
        let path = a.path.map(PathBuf::from);
        if !a.dirty {
            if let Some(p) = path.as_ref().filter(|p| p.exists()) {
                if app.load_path(p) {
                    return;
                }
            }
        }
        if let Ok(doc) = mindloom_core::model::normalize_value(a.doc) {
            app.set_doc(doc, path, Some(a.name), true);
            app.toast("Recovered your unsaved changes from the last session.");
            return;
        }
    }
    if let Some(p) = app.prefs.last_path.clone().map(PathBuf::from).filter(|p| p.exists()) {
        app.load_path(&p);
    }
}

impl eframe::App for Mindloom {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let app = &mut self.app;
        app.now = ctx.input(|i| i.time);
        if ctx.input(|i| i.viewport().close_requested()) && !app.quit_ok {
            if app.editing.is_some() {
                app.commit_edit();
            }
            if app.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                app.dialog = Some(Dialog::SaveChanges(After::Quit));
            } else {
                app.quit_ok = true;
            }
        }
        // files dropped on the window: images go onto the topic under the pointer, maps open
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let path = f.path().to_path_buf();
            if path.as_os_str().is_empty() {
                continue;
            }
            let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            if ["png", "jpg", "jpeg", "gif", "webp", "bmp"].contains(&ext.as_str()) {
                let target = app.hover.clone().or_else(|| app.primary());
                match (target, image::open(&path)) {
                    (Some(id), Ok(img)) => app.set_image(Some(id), img),
                    (None, _) => app.toast("Drop the image onto a topic."),
                    (_, Err(_)) => app.toast("Could not load that image."),
                }
            } else {
                app.guard(After::OpenPath(path));
            }
        }
        #[cfg(target_os = "macos")]
        if self.started {
            for p in mac::take() {
                app.guard(After::OpenPath(p));
            }
        }
        app.autosave_tick();
        let title = format!("{}{} — Mindloom", app.name, if app.dirty { " •" } else { "" });
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if app.autosave_due.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(300));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // text can only be measured once egui has loaded the fonts, in the first frame
        if !self.started {
            self.started = true;
            #[cfg(target_os = "macos")]
            let launched_with: Vec<PathBuf> = mac::take();
            #[cfg(not(target_os = "macos"))]
            let launched_with: Vec<PathBuf> = vec![];
            if let Some(p) = launched_with.first() {
                // double-clicked in Finder: that file, not the last session
                self.app.refresh_geo();
                self.app.load_path(p);
            } else if self.test.is_none() {
                startup(&mut self.app);
            } else {
                self.app.prefs = files::Prefs::default();
                self.app.refresh_geo();
            }
        }
        if let Some(t) = &mut self.test {
            let ctx = ui.ctx().clone();
            t.frame(&mut self.app, &ctx);
        }
        ui::frame(&mut self.app, ui);
    }

    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(t) = &mut self.test {
            raw.events.append(&mut t.events);
        }
    }

    fn on_exit(&mut self) {
        files::save_prefs(&self.app.prefs);
        // a clean close leaves nothing to recover
        files::autosave_clear(&self.app.autosave_key);
    }
}

fn icon() -> Option<egui::IconData> {
    let img = image::load_from_memory(include_bytes!("../assets/icon.png")).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

fn main() -> eframe::Result {
    #[cfg(target_os = "macos")]
    mac::install();
    let mut viewport =
        egui::ViewportBuilder::default().with_title("Mindloom").with_inner_size([1360.0, 860.0]).with_min_inner_size([760.0, 480.0]).with_drag_and_drop(true);
    if std::env::var_os("MINDLOOM_SELFTEST").is_some() {
        // the self test needs the window drawn: keep it in front while it runs
        viewport = viewport.with_window_level(egui::WindowLevel::AlwaysOnTop).with_active(true).with_position([40.0, 40.0]);
    }
    if let Some(i) = icon() {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("Mindloom", options, Box::new(|cc| Ok(Box::new(Mindloom::new(cc)))))
}
