//! The canvas: painting the map, pointer interaction, the inline text editor, overlays and context menus.

use crate::canvas::{self, alpha, Canvas, PaintOpts, ACCENT};
use crate::state::{Act, App, Cam, Dialog, DropKind, Marquee, Pan, RelDrag, Target};
use egui::{pos2, vec2, Align2, CornerRadius, CursorIcon, FontId, Id, Key, Modifiers, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};
use mindloom_core::model;
use std::collections::HashSet;

pub fn show(app: &mut App, ui: &mut Ui) {
    let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
    if rect.size() != app.stage.size() || rect.min != app.stage.min {
        let resized = rect.size() != app.stage.size();
        app.stage = rect;
        // going full screen for a presentation: frame the slide for the new size
        if resized && app.present.is_some() {
            app.show_slide();
        }
    }
    if app.pending_fit && rect.width() > 50.0 {
        app.pending_fit = false;
        app.fit(false);
    }
    if app.step_anim() {
        ui.ctx().request_repaint();
    }
    input(app, ui, &resp, rect);

    let painter = ui.painter_at(rect);
    let theme = app.theme();
    canvas::paint_background(&painter, rect, app.view(), &app.doc.settings.pattern, theme);
    let faded: HashSet<String> = app.drag.as_ref().filter(|d| d.started).map(|d| d.exclude.clone()).unwrap_or_default();
    let editing = app.editing.as_ref().map(|e| e.id.clone());
    {
        let view = app.view();
        let App { doc, fonts, images, measure, ctx, focus_id, sel, matches, sel_rel, hover, .. } = app;
        let m = crate::state::measure_fn(ctx, fonts, measure);
        let lctx = mindloom_core::layout::Ctx { theme, settings: &doc.settings, measure: &m, focus_id: focus_id.as_deref(), ignore_offset_id: None };
        let l = mindloom_core::layout::layout(doc, &lctx);
        let mut c = Canvas { painter: &painter, view, fonts, images };
        let o = PaintOpts {
            theme,
            settings: &doc.settings,
            selected: sel,
            matches,
            selected_rel: sel_rel.as_deref(),
            faded: &faded,
            hover: hover.as_deref(),
            editing: editing.as_deref(),
            export: false,
        };
        canvas::paint(&mut c, &l, doc, &o);
    }
    paint_hints(app, &painter);
    editor(app, ui);
    overlays(app, ui, rect);
    context_menu(app, &resp);
}

fn input(app: &mut App, ui: &Ui, resp: &egui::Response, rect: Rect) {
    let (pos, pressed, released, down, mods, zoom, scroll) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.primary_down(),
            i.modifiers,
            i.zoom_delta(),
            i.smooth_scroll_delta,
        )
    });
    let hovered = resp.hovered();
    // hover (expand handles show on hover)
    app.hover = pos.filter(|p| hovered && rect.contains(*p)).and_then(|p| {
        let w = app.to_world(p);
        app.geo.node_at(w.0, w.1, 0.0).map(|i| app.geo.items[i].id.clone())
    });

    if hovered {
        if zoom != 1.0 {
            app.zoom_at(zoom, pos);
        } else if scroll != egui::Vec2::ZERO && app.drag.is_none() {
            app.settle_view();
            app.cam.x += scroll.x;
            app.cam.y += scroll.y;
        }
    }

    if pressed && hovered {
        if let Some(p) = pos {
            press(app, p, mods);
        }
    }
    if down {
        if let Some(p) = pos {
            if let Some(pan) = &mut app.pan {
                let d = p - pan.start;
                if !pan.moved && d.x.abs() + d.y.abs() > 3.0 {
                    pan.moved = true;
                }
                if pan.moved {
                    app.cam = Cam { zoom: pan.cam.zoom, x: pan.cam.x + d.x, y: pan.cam.y + d.y };
                }
            } else if let Some(m) = &mut app.marquee {
                m.cur = p;
            } else if app.rel_drag.is_some() {
                app.rel_drag_move(p);
            } else if app.drag.is_some() {
                app.drag_move(p);
            }
        }
    }
    if released {
        if let Some(pan) = app.pan.take() {
            if !pan.moved && !mods.shift {
                app.sel.clear();
                app.sel_rel = None;
            }
        } else if app.marquee.is_some() {
            app.marquee_end();
        } else if let Some(rd) = app.rel_drag.take() {
            if rd.moved {
                app.commit(false);
            }
        } else if app.drag.is_some() {
            app.drag_end();
        }
    }
    if resp.double_clicked() {
        if let Some(p) = pos {
            let w = app.to_world(p);
            if app.geo.act_at(w.0, w.1).is_none() {
                if let Some(i) = app.geo.node_at(w.0, w.1, 0.0) {
                    let id = app.geo.items[i].id.clone();
                    app.drag = None;
                    app.start_edit(&id, None);
                } else if let Some(r) = app.geo.rel_at(w.0, w.1, 7.0 / app.cam.zoom as f64) {
                    let label = app.doc.relations.iter().find(|x| x.id == r).map(|x| x.label.clone()).unwrap_or_default();
                    app.sel_rel = Some(r);
                    app.dialog = Some(Dialog::RelLabel(label));
                } else {
                    app.floating_at(w.0, w.1);
                }
            }
        }
    }
    if resp.secondary_clicked() {
        if let Some(p) = pos {
            if app.editing.is_some() {
                app.commit_edit();
            }
            let w = app.to_world(p);
            app.ctx_world = w;
            if let Some(i) = app.geo.node_at(w.0, w.1, 0.0) {
                let id = app.geo.items[i].id.clone();
                if !app.sel.contains(&id) {
                    app.select(vec![id], false, false);
                }
                app.ctx_target = Target::Node;
            } else if let Some(r) = app.geo.rel_at(w.0, w.1, 7.0 / app.cam.zoom as f64) {
                app.sel_rel = Some(r);
                app.sel.clear();
                app.ctx_target = Target::Rel;
            } else {
                app.ctx_target = Target::Canvas;
            }
        }
    }
    // cursor
    if app.pan.as_ref().is_some_and(|p| p.moved) || app.drag.as_ref().is_some_and(|d| d.started) {
        ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
    } else if hovered && app.relation_from.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
    } else if hovered && app.hover.is_some() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn press(app: &mut App, p: Pos2, mods: Modifiers) {
    app.settle_view();
    if app.editing.is_some() {
        app.commit_edit();
    }
    let w = app.to_world(p);
    let node = app.geo.node_at(w.0, w.1, 0.0).map(|i| app.geo.items[i].id.clone());
    if app.relation_from.is_some() {
        app.finish_relation(node);
        return;
    }
    if let Some((i, act)) = app.geo.act_at(w.0, w.1) {
        let id = app.geo.items[i].id.clone();
        match act {
            Act::Toggle => return app.toggle_collapse_of(&id),
            Act::Task => {
                if let Some(n) = model::get_mut(&mut app.doc, &id) {
                    n.task = Some(if n.task.as_deref() == Some("done") { "todo" } else { "done" }.into());
                }
                return app.commit(false);
            }
            Act::Link(url) => {
                app.ctx.open_url(egui::OpenUrl::new_tab(url));
                return;
            }
            Act::Note => {
                app.select(vec![id], false, false);
                app.prefs.show_inspector = true;
                app.tab = crate::state::Tab::Notes;
                return;
            }
            Act::Marker(m) => {
                if let Some(pct) = m.strip_prefix("pr").and_then(|x| x.parse::<u32>().ok()) {
                    let next = format!("pr{}", (pct + 25) % 125);
                    if let Some(n) = model::get_mut(&mut app.doc, &id) {
                        if let Some(icons) = &mut n.icons {
                            for x in icons.iter_mut().filter(|x| **x == m) {
                                *x = next.clone();
                            }
                        }
                    }
                    return app.commit(false);
                }
                app.select(vec![id], false, false);
                app.prefs.show_inspector = true;
                app.tab = crate::state::Tab::Markers;
                return;
            }
        }
    }
    if node.is_none() {
        if let Some(r) = app.geo.rel_at(w.0, w.1, 7.0 / app.cam.zoom as f64) {
            app.sel_rel = Some(r.clone());
            app.sel.clear();
            app.rel_drag = Some(RelDrag { id: r, start: p, moved: false });
            return;
        }
    }
    if let Some(id) = node {
        if mods.shift || mods.command {
            app.select(vec![id], true, false);
            return;
        }
        app.press_node(id, p);
        return;
    }
    if mods.shift {
        app.marquee = Some(Marquee { start: p, base: app.sel.clone(), cur: p });
        return;
    }
    app.pan = Some(Pan { start: p, cam: app.cam, moved: false });
}

/// Drop targets, the home slot, side switching and the selection rectangle.
fn paint_hints(app: &App, painter: &egui::Painter) {
    let v = app.view();
    let c = ACCENT;
    if let Some(d) = app.drag.as_ref().filter(|d| d.started) {
        if let (Some(h), None) = (d.home, &d.drop) {
            let free_child = d.ids.len() == 1 && app.can_free(&d.id) && !d.orig.get(&d.id).is_some_and(|o| o.floating_root);
            if free_child {
                let ring: Vec<Pos2> = canvas::rounded_ring(h.2 + 6.0, h.3 + 6.0, 9.0).into_iter().map(|(x, y)| v.p(h.0 - 3.0 + x, h.1 - 3.0 + y)).collect();
                let mut closed = ring.clone();
                closed.push(ring[0]);
                painter.extend(Shape::dashed_line(
                    &closed,
                    Stroke::new(if d.snapped { 2.5 } else { 1.5 }, alpha(c, if d.snapped { 0.9 } else { 0.35 })),
                    5.0,
                    4.0,
                ));
            }
        }
        if let Some(drop) = &d.drop {
            let t = &app.geo.items[drop.target];
            match drop.kind {
                DropKind::Child => {
                    let r = v.rect(t.x - 6.0, t.y - 6.0, t.w + 12.0, t.h + 12.0);
                    painter.rect_filled(r, CornerRadius::same(10), alpha(c, 0.12));
                    painter.rect_stroke(r, CornerRadius::same(10), Stroke::new(2.5, c), StrokeKind::Middle);
                }
                _ if drop.vertical => {
                    let x = if drop.kind == DropKind::Before { t.x - 8.0 } else { t.x + t.w + 8.0 };
                    painter.line_segment([v.p(x, t.y - 4.0), v.p(x, t.y + t.h + 4.0)], Stroke::new(4.0, c));
                }
                _ => {
                    let y = if drop.kind == DropKind::Before { t.y - 7.0 } else { t.y + t.h + 7.0 };
                    painter.line_segment([v.p(t.x - 4.0, y), v.p(t.x + t.w + 4.0, y)], Stroke::new(4.0, c));
                }
            }
        } else if let (Some(flip), Some(r)) = (d.flip, app.geo.get(&app.doc.root.id)) {
            let x = if flip == "left" { r.x - 14.0 } else { r.x + r.w + 14.0 };
            painter.line_segment([v.p(x, r.y - 10.0), v.p(x, r.y + r.h + 10.0)], Stroke::new(4.0, c));
        }
    }
    if let Some(m) = &app.marquee {
        let r = Rect::from_two_pos(m.start, m.cur);
        painter.rect_filled(r, CornerRadius::ZERO, alpha(c, 0.08));
        let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
        painter.extend(Shape::dashed_line(&pts, Stroke::new(1.2, c), 4.0, 3.0));
    }
}

/// The text box over a topic while its text is edited.
fn editor(app: &mut App, ui: &mut Ui) {
    let Some(ed) = &app.editing else { return };
    let Some(t) = app.geo.get(&ed.id).cloned() else {
        app.editing = None;
        return;
    };
    let v = app.view();
    let z = app.cam.zoom;
    let st = &t.style;
    let fam = app.fonts.family(&t.font);
    let x = v.p(t.x + t.row.x + t.row.text_x, 0.0).x;
    let y = v.p(0.0, t.y + t.row.y + (t.row.h - t.row.text_h) / 2.0).y;
    let w = (t.row.text_w as f32 * z).max(40.0) + 14.0;
    let ink = canvas::color(&st.color);
    let fill = canvas::color(&st.fill);
    let align = match st.align.as_str() {
        "left" => egui::Align::LEFT,
        "right" => egui::Align::RIGHT,
        _ => egui::Align::Center,
    };
    // keys the editor handles itself, before the text box sees them
    let (enter, tab, shift_tab, esc) = ui.input_mut(|i| {
        let enter = i.consume_key(Modifiers::NONE, Key::Enter) || i.consume_key(Modifiers::COMMAND, Key::Enter);
        let shift_tab = i.consume_key(Modifiers::SHIFT, Key::Tab);
        let tab = i.consume_key(Modifiers::NONE, Key::Tab);
        let esc = i.consume_key(Modifiers::NONE, Key::Escape);
        (enter, tab, shift_tab, esc)
    });
    if esc {
        return app.cancel_edit();
    }
    if enter {
        return app.commit_edit();
    }
    if tab || shift_tab {
        app.commit_edit();
        return if tab { app.add_child() } else { app.add_sibling(false) };
    }
    let mut text = ed.text.clone();
    let (select_all, focused) = (ed.select_all, ed.focused);
    let id = Id::new("mindloom-editor").with(&ed.id);
    let area = egui::Area::new(Id::new("mindloom-editor-area")).order(egui::Order::Foreground).fixed_pos(pos2(x - 7.0, y - 4.0));
    let out = area.show(ui.ctx(), |ui| {
        let bg = if fill.a() > 200 { fill } else { canvas::color(app.theme().bg) };
        egui::Frame::new().fill(bg).stroke(Stroke::new(1.5, ACCENT)).corner_radius(CornerRadius::same(4)).inner_margin(egui::Margin::symmetric(6, 3)).show(
            ui,
            |ui| {
                let te = egui::TextEdit::multiline(&mut text)
                    .id(id)
                    .font(FontId::new(st.font_size as f32 * z, fam))
                    .text_color(ink)
                    .frame(egui::Frame::NONE)
                    .desired_width(w - 12.0)
                    .desired_rows(1)
                    .horizontal_align(align)
                    .lock_focus(true);
                te.show(ui)
            },
        )
    });
    let te_out = out.inner.inner;
    if !focused {
        te_out.response.request_focus();
        if select_all {
            let mut state = te_out.state.clone();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(text.chars().count()))));
            state.store(ui.ctx(), id);
        } else {
            let mut state = te_out.state.clone();
            let n = text.chars().count();
            state.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(n))));
            state.store(ui.ctx(), id);
        }
        if let Some(e) = &mut app.editing {
            e.focused = true;
        }
    }
    if te_out.response.changed() {
        app.edit_text(text);
    }
}

fn pill(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    egui::Frame::popup(ui.style()).corner_radius(CornerRadius::same(8)).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
        ui.horizontal(add);
    });
}

fn overlays(app: &mut App, ui: &mut Ui, rect: Rect) {
    let ctx = ui.ctx().clone();
    let present = app.present.is_some();
    // zoom bar
    if !present {
        egui::Area::new(Id::new("zoombar")).fixed_pos(rect.right_bottom() - vec2(214.0, 46.0)).show(&ctx, |ui| {
            pill(ui, |ui| {
                if ui.button("−").on_hover_text("Zoom out").clicked() {
                    app.zoom_at(1.0 / 1.2, None);
                }
                if ui.button(format!("{}%", (app.cam.zoom * 100.0).round())).on_hover_text("Actual size").clicked() {
                    app.actual_size();
                }
                if ui.button("+").on_hover_text("Zoom in").clicked() {
                    app.zoom_at(1.2, None);
                }
                if ui.button(crate::ui::ic(egui_phosphor::regular::CORNERS_OUT)).on_hover_text("Fit map").clicked() {
                    app.fit(true);
                }
                if ui.selectable_label(app.prefs.show_minimap, crate::ui::ic(egui_phosphor::regular::MAP_TRIFOLD)).on_hover_text("Mini map").clicked() {
                    app.prefs.show_minimap = !app.prefs.show_minimap;
                    crate::files::save_prefs(&app.prefs);
                }
            });
        });
        if app.prefs.show_minimap {
            minimap(app, &ctx, rect);
        }
    }
    // focus bar
    if let Some(f) = app.focus_id.clone() {
        let name = model::get(&app.doc, &f)
            .map(|n| if n.text.is_empty() { "Topic".to_string() } else { n.text.lines().next().unwrap_or("").to_string() })
            .unwrap_or_default();
        egui::Area::new(Id::new("focusbar")).anchor(Align2::CENTER_TOP, vec2(0.0, rect.top() + 10.0 - ctx.content_rect().top())).show(&ctx, |ui| {
            pill(ui, |ui| {
                ui.label("Focused on");
                ui.strong(name);
                if ui.button("Show whole map").clicked() {
                    app.focus();
                }
            });
        });
    }
    if app.relation_from.is_some() {
        egui::Area::new(Id::new("modehint")).anchor(Align2::CENTER_TOP, vec2(0.0, rect.top() + 50.0 - ctx.content_rect().top())).show(&ctx, |ui| {
            pill(ui, |ui| {
                ui.label("Click the topic to connect to · Esc to cancel");
            });
        });
    }
    // search bar
    if app.search_open {
        egui::Area::new(Id::new("searchbar")).fixed_pos(pos2(rect.right() - 340.0, rect.top() + 10.0)).show(&ctx, |ui| {
            pill(ui, |ui| {
                let r = ui.add(egui::TextEdit::singleline(&mut app.search_q).hint_text("Find in map…").desired_width(170.0));
                if app.search_focus {
                    r.request_focus();
                    app.search_focus = false;
                }
                if r.changed() {
                    app.run_search();
                }
                if r.has_focus() {
                    let (enter, shift_enter, esc) = ui.input_mut(|i| {
                        (i.consume_key(Modifiers::NONE, Key::Enter), i.consume_key(Modifiers::SHIFT, Key::Enter), i.consume_key(Modifiers::NONE, Key::Escape))
                    });
                    if enter {
                        app.step_search(1);
                    }
                    if shift_enter {
                        app.step_search(-1);
                    }
                    if esc {
                        app.close_search();
                    }
                } else if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    app.step_search(1);
                    r.request_focus();
                }
                let count = if app.search_q.is_empty() {
                    String::new()
                } else if app.matches.is_empty() {
                    "none".into()
                } else {
                    format!("{}/{}", app.match_idx + 1, app.matches.len())
                };
                ui.label(egui::RichText::new(count).weak());
                if ui.small_button("↑").clicked() {
                    app.step_search(-1);
                }
                if ui.small_button("↓").clicked() {
                    app.step_search(1);
                }
                if ui.small_button("✕").clicked() {
                    app.close_search();
                }
            });
        });
    }
    // presentation bar
    if let Some(p) = &app.present {
        let (i, n) = (p.i, p.slides.len());
        egui::Area::new(Id::new("presentbar")).anchor(Align2::CENTER_BOTTOM, vec2(0.0, -18.0)).show(&ctx, |ui| {
            pill(ui, |ui| {
                if ui.button("‹").clicked() {
                    app.present_step(-1);
                }
                ui.label(format!("{} / {n}", i + 1));
                if ui.button("›").clicked() {
                    app.present_step(1);
                }
                if ui.button("Exit").clicked() {
                    app.present_exit();
                }
            });
        });
    }
}

fn minimap(app: &mut App, ctx: &egui::Context, rect: Rect) {
    let (w, h) = (200.0f32, 130.0f32);
    let at = pos2(rect.left() + 12.0, rect.bottom() - h - 12.0);
    egui::Area::new(Id::new("minimap")).fixed_pos(at).show(ctx, |ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click_and_drag());
        let p = ui.painter_at(r);
        let theme = app.theme();
        p.rect(r, CornerRadius::same(8), canvas::color(theme.bg), Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
        let (bx, by, bw, bh) = app.geo.bounds;
        let pad = 8.0;
        let k = ((w - pad * 2.0) / bw.max(1.0) as f32).min((h - pad * 2.0) / bh.max(1.0) as f32);
        let ox = r.left() + pad + (w - pad * 2.0 - bw as f32 * k) / 2.0 - bx as f32 * k;
        let oy = r.top() + pad + (h - pad * 2.0 - bh as f32 * k) / 2.0 - by as f32 * k;
        for t in &app.geo.items {
            let st = &t.style;
            let col = if !st.fill.is_empty() && st.fill != "transparent" && st.fill != theme.bg { canvas::color(&st.fill) } else { canvas::color(&st.line) };
            p.rect_filled(
                Rect::from_min_size(pos2(ox + t.x as f32 * k, oy + t.y as f32 * k), vec2((t.w as f32 * k).max(2.0), (t.h as f32 * k).max(1.5))),
                CornerRadius::ZERO,
                col,
            );
        }
        let z = app.cam.zoom;
        let vr = Rect::from_min_size(pos2(ox + (-app.cam.x / z) * k, oy + (-app.cam.y / z) * k), vec2(app.stage.width() / z * k, app.stage.height() / z * k));
        p.rect_filled(vr, CornerRadius::ZERO, alpha(ACCENT, 0.08));
        p.rect_stroke(vr, CornerRadius::ZERO, Stroke::new(1.5, ACCENT), StrokeKind::Middle);
        if resp.is_pointer_button_down_on() {
            if let Some(pp) = resp.interact_pointer_pos() {
                let (wx, wy) = ((pp.x - ox) / k, (pp.y - oy) / k);
                app.settle_view();
                app.cam.x = app.stage.width() / 2.0 - wx * z;
                app.cam.y = app.stage.height() / 2.0 - wy * z;
            }
        }
    });
}

fn context_menu(app: &mut App, resp: &egui::Response) {
    let k = crate::ui::kb;
    resp.context_menu(|ui| {
        ui.set_min_width(230.0);
        let item = |ui: &mut Ui, label: &str, key: &str, enabled: bool| -> bool {
            let b = egui::Button::new(label).shortcut_text(key);
            let r = ui.add_enabled(enabled, b);
            if r.clicked() {
                ui.close();
                true
            } else {
                false
            }
        };
        match app.ctx_target {
            Target::Rel => {
                if item(ui, "Edit label…", "", true) {
                    let label = app.sel_rel.as_ref().and_then(|r| app.doc.relations.iter().find(|x| x.id == *r)).map(|x| x.label.clone()).unwrap_or_default();
                    app.dialog = Some(Dialog::RelLabel(label));
                }
                if item(ui, "Reverse direction", "", true) {
                    app.relation_reverse();
                }
                ui.separator();
                if item(ui, "Delete relationship", "Del", true) {
                    app.delete_sel();
                }
            }
            Target::Node => {
                let Some(id) = app.primary() else { return };
                let Some(n) = model::get(&app.doc, &id).cloned() else { return };
                let has_parent = model::find(&app.doc, &id).is_some_and(|l| l.has_parent());
                let root = app.is_root(&id);
                if item(ui, "Add subtopic", "Tab", true) {
                    app.add_child();
                }
                if item(ui, "Add topic", "Enter", true) {
                    app.add_sibling(false);
                }
                if item(ui, "Add parent topic", &k("Mod+Enter"), !root && has_parent) {
                    app.add_parent();
                }
                if item(ui, "Edit text", "F2", true) {
                    app.edit_primary();
                }
                ui.separator();
                if item(ui, if n.is_collapsed() { "Expand branch" } else { "Collapse branch" }, &k("Mod+/"), !n.children.is_empty()) {
                    app.toggle_collapse();
                }
                if item(ui, "Focus on this branch", &k("Mod+Shift+F"), !root && has_parent) {
                    app.focus();
                }
                if item(ui, "Sort subtopics A→Z", "", n.children.len() >= 2) {
                    app.sort_children();
                }
                ui.separator();
                if item(ui, "Relationship…", &k("Mod+Shift+R"), true) {
                    app.relation();
                }
                if item(ui, if n.has_boundary() { "Remove boundary" } else { "Add boundary" }, &k("Mod+Shift+B"), true) {
                    app.boundary();
                }
                if item(ui, "Note…", &k("Mod+Shift+M"), true) {
                    app.prefs.show_inspector = true;
                    app.tab = crate::state::Tab::Notes;
                    app.note_focus = true;
                }
                if item(ui, "Link…", &k("Mod+K"), true) {
                    app.dialog = Some(Dialog::Link(n.link.clone().unwrap_or_default()));
                }
                if item(ui, if n.image.is_some() { "Replace image…" } else { "Insert image…" }, "", true) {
                    app.pick_image();
                }
                if item(ui, if n.task.is_some() { "Cycle task state" } else { "Make it a task" }, &k("Mod+Shift+T"), true) {
                    app.task();
                }
                ui.separator();
                if item(ui, "Expand with AI…", "", true) {
                    app.dialog = Some(Dialog::Ai(Box::new(crate::ui::AiForm::new(Some(id.clone()), true))));
                }
                ui.separator();
                if item(ui, "Copy", &k("Mod+C"), true) {
                    if let Some(t) = app.copy() {
                        ui.ctx().copy_text(t);
                    }
                }
                if item(ui, "Paste as subtopic", &k("Mod+V"), true) {
                    let t = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
                    app.paste(t.as_deref());
                }
                if item(ui, "Copy style", &k("Mod+Alt+C"), true) {
                    app.copy_style();
                }
                if item(ui, "Paste style", &k("Mod+Alt+V"), app.style_clip.is_some()) {
                    app.paste_style();
                }
                if item(ui, "Duplicate", &k("Mod+D"), !root && has_parent) {
                    app.duplicate();
                }
                if item(ui, "Detach as floating topic", "", !root && has_parent) {
                    app.detach();
                }
                if item(ui, "Reset position", "", n.offset.is_some()) {
                    app.reset_position();
                }
                ui.separator();
                if item(ui, "Delete", "Del", !root) {
                    app.delete_sel();
                }
            }
            Target::Canvas => {
                if item(ui, "New floating topic here", "", true) {
                    let (x, y) = app.ctx_world;
                    app.floating_at(x, y);
                }
                if item(ui, "Paste", &k("Mod+V"), true) {
                    let t = arboard::Clipboard::new().ok().and_then(|mut c| c.get_text().ok());
                    app.paste(t.as_deref());
                }
                ui.separator();
                if item(ui, "Fit map to window", &k("Mod+0"), true) {
                    app.fit(true);
                }
                if item(ui, "Center on central topic", "Home", true) {
                    let r = app.doc.root.id.clone();
                    app.center_on(&r, true);
                }
                if item(ui, "Reset all hand-placed topics", "", true) {
                    app.reset_all_positions();
                }
                ui.separator();
                if item(ui, "Expand all", "", true) {
                    app.expand_all();
                }
                if item(ui, "Collapse all", "", true) {
                    app.collapse_all();
                }
                if item(ui, "Show only level 1", "", true) {
                    app.collapse_level(1);
                }
                if item(ui, "Show levels 1–2", "", true) {
                    app.collapse_level(2);
                }
                ui.separator();
                if item(ui, "Map style & theme…", "", true) {
                    app.prefs.show_inspector = true;
                    app.tab = crate::state::Tab::Map;
                }
                if item(ui, "New map from template…", &k("Mod+N"), true) {
                    app.dialog = Some(Dialog::Templates);
                }
            }
        }
    });
}
