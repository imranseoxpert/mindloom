//! Self test: with `MINDLOOM_SELFTEST=<dir>` the app drives its own interface with real mouse and
//! keyboard events, checks the results, saves screenshots and a report to `<dir>`, then quits.
//! Use `MINDLOOM_CONFIG_DIR` to keep it away from your own settings.

use crate::state::{App, Dialog, Tab};
use egui::{Event, Key, Modifiers, PointerButton, Pos2};
use mindloom_core::model;
use std::path::PathBuf;

type Action = Box<dyn FnMut(&mut App, &mut Tester)>;

enum Step {
    Wait(u32),
    Shot(&'static str),
    Do(Action),
}

pub struct Tester {
    dir: PathBuf,
    steps: Vec<Step>,
    i: usize,
    wait: u32,
    pending_shot: Option<&'static str>,
    shot_wait: u32,
    pub events: Vec<Event>,
    /// Modifier keys held down (egui reads them from the input state, not from pointer events).
    pub held: Modifiers,
    pub checks: Vec<(String, bool)>,
    pub done: bool,
}

fn cmd() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers { mac_cmd: true, command: true, ..Default::default() }
    } else {
        Modifiers { ctrl: true, command: true, ..Default::default() }
    }
}

fn key(k: Key, m: Modifiers) -> Vec<Event> {
    vec![
        Event::Key { key: k, physical_key: Some(k), pressed: true, repeat: false, modifiers: m },
        Event::Key { key: k, physical_key: Some(k), pressed: false, repeat: false, modifiers: m },
    ]
}

/// Screen position of the centre of the topic with this text.
pub fn center_of(app: &App, text: &str) -> Option<Pos2> {
    let id = find_id(app, text)?;
    let t = app.geo.get(&id)?;
    Some(app.view().p(t.x + t.w / 2.0, t.y + t.h / 2.0))
}
pub fn find_id(app: &App, text: &str) -> Option<String> {
    let mut out = None;
    model::walk_doc(&app.doc, &mut |n, _| {
        if out.is_none() && n.text == text {
            out = Some(n.id.clone());
        }
    });
    out
}
fn parent_text(app: &App, text: &str) -> Option<String> {
    let id = find_id(app, text)?;
    let loc = model::find(&app.doc, &id)?;
    Some(model::at(&app.doc, &loc.parent()?).text.clone())
}

fn act(f: impl FnMut(&mut App, &mut Tester) + 'static) -> Step {
    Step::Do(Box::new(f))
}

/// Press on a topic, move in small steps, release (all through real pointer events).
fn drag(steps: &mut Vec<Step>, from: &'static str, to: impl Fn(&App, Pos2) -> Pos2 + Clone + 'static, mods: Modifiers) {
    let start = std::rc::Rc::new(std::cell::Cell::new(Pos2::ZERO));
    let s0 = start.clone();
    steps.push(act(move |app, t| {
        let p = center_of(app, from).unwrap_or(Pos2::ZERO);
        s0.set(p);
        t.events.push(Event::PointerMoved(p));
        t.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: mods });
    }));
    steps.push(Step::Wait(2));
    for k in 1..=8 {
        let (s, to) = (start.clone(), to.clone());
        steps.push(act(move |app, t| {
            let a = s.get();
            let b = to(app, a);
            let p = a + (b - a) * (k as f32 / 8.0);
            t.events.push(Event::PointerMoved(p));
        }));
        steps.push(Step::Wait(1));
    }
    let (s, to2) = (start.clone(), to.clone());
    steps.push(act(move |app, t| {
        let b = to2(app, s.get());
        t.events.push(Event::PointerButton { pos: b, button: PointerButton::Primary, pressed: false, modifiers: mods });
    }));
    steps.push(Step::Wait(4));
}

impl Tester {
    pub fn from_env() -> Option<Tester> {
        let dir = PathBuf::from(std::env::var_os("MINDLOOM_SELFTEST")?);
        let _ = std::fs::create_dir_all(&dir);
        let mut s: Vec<Step> = vec![Step::Wait(30), Step::Shot("01-welcome")];
        s.push(act(|app, t| t.check("welcome map has topics", app.geo.items.len() > 20)));

        // add a subtopic with Tab, type, Enter
        s.push(act(|app, _| {
            if let Some(id) = find_id(app, "Organise") {
                app.select(vec![id], false, true);
            }
        }));
        s.push(Step::Wait(2));
        s.push(act(|_, t| t.events.extend(key(Key::Tab, Modifiers::NONE))));
        s.push(Step::Wait(4));
        s.push(act(|app, t| t.check("Tab starts editing a new subtopic", app.editing.is_some())));
        s.push(act(|_, t| t.events.push(Event::Text("Typed in Rust".into()))));
        s.push(Step::Wait(3));
        s.push(act(|_, t| t.events.extend(key(Key::Enter, Modifiers::NONE))));
        s.push(Step::Wait(4));
        s.push(act(|app, t| {
            t.check("typed subtopic is saved", parent_text(app, "Typed in Rust").as_deref() == Some("Organise"));
            t.check("editing ended on Enter", app.editing.is_none());
        }));
        s.push(Step::Shot("02-new-subtopic"));

        // free drag: stays connected, gets an offset
        drag(&mut s, "Style", |_, a| a + egui::vec2(170.0, 110.0), Modifiers::NONE);
        s.push(act(|app, t| {
            let id = find_id(app, "Style").unwrap_or_default();
            let n = model::get(&app.doc, &id);
            t.check("dragged topic keeps its parent", parent_text(app, "Style").as_deref() == Some("Welcome to Mindloom"));
            t.check("dragged topic has a hand-placed offset", n.is_some_and(|n| n.offset.is_some()));
        }));
        s.push(Step::Shot("03-dragged-stays-connected"));

        // drop onto another topic: re-parent
        drag(&mut s, "Images", |app, a| center_of(app, "Share").unwrap_or(a), Modifiers::NONE);
        s.push(act(|app, t| t.check("dropping onto a topic re-parents it", parent_text(app, "Images").as_deref() == Some("Share"))));
        s.push(Step::Shot("04-reparented"));

        // undo twice with the keyboard
        s.push(act(|_, t| t.events.extend(key(Key::Z, cmd()))));
        s.push(Step::Wait(3));
        s.push(act(|_, t| t.events.extend(key(Key::Z, cmd()))));
        s.push(Step::Wait(3));
        s.push(act(|app, t| {
            t.check("undo reverts the re-parent", parent_text(app, "Images").as_deref() == Some("Enrich"));
            let n = find_id(app, "Style").and_then(|id| model::get(&app.doc, &id).cloned());
            t.check("undo reverts the drag", n.is_some_and(|n| n.offset.is_none()));
        }));

        // double-click empty canvas: floating topic
        s.push(act(|app, t| {
            let p = app.stage.left_top() + egui::vec2(60.0, 60.0);
            for _ in 0..2 {
                t.events.push(Event::PointerMoved(p));
                t.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
                t.events.push(Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
            }
        }));
        s.push(Step::Wait(4));
        s.push(act(|app, t| {
            t.check("double-click on the canvas adds a floating topic", app.doc.floating.len() == 1);
            if app.editing.is_some() {
                app.commit_edit();
            }
        }));

        // marquee selection, from empty canvas at the top right to the middle
        s.push(Step::Wait(30));
        s.push(act(|app, t| {
            app.settle_view();
            let a = app.stage.right_top() + egui::vec2(-24.0, 24.0);
            let sh = Modifiers { shift: true, ..Default::default() };
            t.held = sh;
            t.events.push(Event::ModifiersChanged(sh));
            t.events.push(Event::PointerMoved(a));
        }));
        s.push(Step::Wait(2));
        s.push(act(|app, t| {
            let a = app.stage.right_top() + egui::vec2(-24.0, 24.0);
            t.events.push(Event::PointerButton { pos: a, button: PointerButton::Primary, pressed: true, modifiers: t.held });
        }));
        s.push(Step::Wait(2));
        s.push(act(|app, t| t.events.push(Event::PointerMoved(app.stage.center()))));
        s.push(Step::Wait(2));
        s.push(Step::Shot("05-marquee"));
        s.push(act(|app, t| {
            let z = app.stage.center();
            t.events.push(Event::PointerButton { pos: z, button: PointerButton::Primary, pressed: false, modifiers: t.held });
        }));
        s.push(Step::Wait(2));
        s.push(act(|_, t| {
            t.held = Modifiers::NONE;
            t.events.push(Event::ModifiersChanged(Modifiers::NONE));
        }));
        s.push(Step::Wait(2));
        s.push(act(|app, t| t.check("shift-drag selects several topics", app.sel.len() > 2)));

        // style, panels and dialogs
        s.push(act(|app, _| {
            app.prefs.show_outline = true;
            app.prefs.show_minimap = true;
            app.tab = Tab::Map;
            app.doc.settings.theme = "midnight".into();
            app.commit(false);
            app.pending_fit = true;
        }));
        s.push(Step::Wait(20));
        s.push(Step::Shot("06-map-tab-midnight"));
        s.push(act(|app, _| {
            if let Some(id) = find_id(app, "Enrich") {
                app.select(vec![id], false, false);
            }
            app.tab = Tab::Topic;
        }));
        s.push(Step::Wait(6));
        s.push(Step::Shot("07-topic-tab"));
        s.push(act(|app, _| app.tab = Tab::Markers));
        s.push(Step::Wait(6));
        s.push(Step::Shot("08-markers-tab"));
        for (i, lay) in ["right", "down", "tree", "timeline", "fishbone", "radial"].iter().enumerate() {
            s.push(act(move |app, _| {
                app.doc.settings.layout = lay.to_string();
                app.doc.settings.theme = ["fresh", "ocean", "business", "sunset", "forest", "neon"][i].into();
                app.commit(false);
                app.pending_fit = true;
            }));
            s.push(Step::Wait(20));
            s.push(Step::Shot(["09-layout-right", "10-layout-down", "11-layout-tree", "12-layout-timeline", "13-layout-fishbone", "14-layout-radial"][i]));
        }
        s.push(act(|app, _| {
            app.doc.settings.layout = "mindmap".into();
            app.doc.settings.theme = "fresh".into();
            app.commit(false);
            app.pending_fit = true;
            app.dialog = Some(Dialog::Templates);
        }));
        s.push(Step::Wait(20));
        s.push(Step::Shot("15-templates"));
        s.push(act(|app, _| app.dialog = Some(Dialog::Ai(Box::new(crate::ui::AiForm::new(None, false))))));
        s.push(Step::Wait(12));
        s.push(Step::Shot("16-ai-assistant"));
        s.push(act(|app, _| {
            app.dialog = None;
            app.search_open = true;
            app.search_q = "topic".into();
            app.run_search();
        }));
        s.push(Step::Wait(20));
        s.push(Step::Shot("17-search"));
        s.push(act(|app, t| {
            t.check("search finds topics", !app.matches.is_empty());
            app.close_search();
            app.prefs.ui_theme = "dark".into();
            crate::ui::apply_visuals(&app.ctx, "dark");
        }));
        s.push(Step::Wait(10));
        s.push(Step::Shot("18-dark-interface"));
        // exports, straight to files
        s.push(act(|app, t| {
            let dir = t.dir.clone();
            let svg = app.layout_with(&app.doc, None, |l| crate::files::export_svg(l, &app.doc, app.theme(), &app.doc.settings, false, None));
            let png = crate::files::png(&svg.svg, svg.width, svg.height, 2.0, &app.db);
            t.check("PNG export", png.as_ref().is_ok_and(|b| std::fs::write(dir.join("export.png"), b).is_ok()));
            let pdf = crate::files::pdf(&svg.svg, &app.db);
            t.check("PDF export", pdf.as_ref().is_ok_and(|b| std::fs::write(dir.join("export.pdf"), b).is_ok()));
            let md = mindloom_core::io::to_markdown(&app.doc);
            t.check("Markdown export round-trips", mindloom_core::io::from_markdown(&md).is_ok());
            let p = dir.join("saved.mindloom");
            t.check(
                "save and reopen",
                crate::files::write_atomic(&p, mindloom_core::io::serialize(&app.doc).as_bytes()).is_ok() && crate::files::read_map(&p).is_ok(),
            );
        }));
        // a large map must stay interactive: lay out and hit-test 1,453 topics well within a frame
        s.push(act(|app, t| {
            let saved = app.doc.clone();
            let mut d = mindloom_core::model::Doc::new("Big map");
            let root = d.root.id.clone();
            for i in 0..12 {
                let a = model::add_child(&mut d, &root, &format!("Branch {i}"), None).unwrap_or_default();
                for j in 0..12 {
                    let b = model::add_child(&mut d, &a, &format!("Topic {i}.{j} with a few words"), None).unwrap_or_default();
                    for k in 0..9 {
                        model::add_child(&mut d, &b, &format!("Leaf {k}"), None);
                    }
                }
            }
            app.doc = d;
            app.refresh_geo();
            let t0 = std::time::Instant::now();
            for _ in 0..5 {
                app.refresh_geo();
            }
            let ms = t0.elapsed().as_secs_f64() * 1000.0 / 5.0;
            t.check(&format!("1,453-topic map lays out in {ms:.1} ms (under 16 ms)"), ms < 16.0 && app.geo.items.len() == 1453);
            app.doc = saved;
            app.refresh_geo();
        }));
        s.push(act(|app, _| app.present()));
        s.push(Step::Wait(30));
        s.push(Step::Shot("19-presentation"));
        s.push(act(|_, t| t.events.extend(key(Key::ArrowRight, Modifiers::NONE))));
        s.push(Step::Wait(30));
        s.push(Step::Shot("20-presentation-slide-2"));
        s.push(act(|_, t| t.events.extend(key(Key::Escape, Modifiers::NONE))));
        s.push(Step::Wait(20));
        s.push(act(|app, t| t.check("presentation exits on Esc", app.present.is_none())));
        Some(Tester { dir, steps: s, i: 0, wait: 0, pending_shot: None, shot_wait: 0, events: vec![], held: Modifiers::NONE, checks: vec![], done: false })
    }

    pub fn check(&mut self, name: &str, ok: bool) {
        self.checks.push((name.to_string(), ok));
    }

    /// One frame of the script. Call from `ui()`.
    pub fn frame(&mut self, app: &mut App, ctx: &egui::Context) {
        ctx.request_repaint();
        let shots: Vec<std::sync::Arc<egui::ColorImage>> =
            ctx.input(|i| i.events.iter().filter_map(|e| if let Event::Screenshot { image, .. } = e { Some(image.clone()) } else { None }).collect());
        if std::env::var_os("MINDLOOM_SELFTEST_TRACE").is_some() && self.pending_shot.is_some() {
            let raw = ctx.input(|i| i.raw.events.len());
            eprintln!("frame: waiting for shot, {} screenshots, {raw} raw events", shots.len());
        }
        if let (Some(name), Some(img)) = (self.pending_shot, shots.last()) {
            let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
            if let Some(buf) = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba) {
                let _ = buf.save(self.dir.join(format!("{name}.png")));
            }
            self.pending_shot = None;
        }
        if self.pending_shot.is_some() {
            // a request made before the window is shown is dropped: ask again
            self.shot_wait += 1;
            if self.shot_wait.is_multiple_of(30) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            return;
        }
        if self.done {
            return;
        }
        if self.wait > 0 {
            self.wait -= 1;
            return;
        }
        if self.i >= self.steps.len() {
            self.finish(app);
            return;
        }
        let i = self.i;
        self.i += 1;
        if std::env::var_os("MINDLOOM_SELFTEST_TRACE").is_some() {
            eprintln!("step {i}");
        }
        let mut step = std::mem::replace(&mut self.steps[i], Step::Wait(0));
        match &mut step {
            Step::Wait(n) => self.wait = *n,
            Step::Shot(name) => {
                self.pending_shot = Some(name);
                self.shot_wait = 0;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            Step::Do(f) => f(app, self),
        }
    }

    fn finish(&mut self, app: &mut App) {
        self.done = true;
        let failed = self.checks.iter().filter(|c| !c.1).count();
        let mut report = format!("{} checks, {} failed\n", self.checks.len(), failed);
        for (n, ok) in &self.checks {
            report += &format!("{} {n}\n", if *ok { "PASS" } else { "FAIL" });
        }
        let _ = std::fs::write(self.dir.join("report.txt"), &report);
        eprint!("{report}");
        app.dirty = false;
        app.quit_ok = true;
        app.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
