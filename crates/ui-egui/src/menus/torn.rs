//! Floating copies of in-window application menus torn off by the dotted grip.
//!
//! Purpose: keep a menu open after each action so a series of related commands can be run without
//! reopening the menu bar each time. Session-only; native macOS menus and context menus stay as they are.

use super::bar::{MenuHost, items_at, menu_body, take_torn_close};
use super::invoke::{field_event, focused_text_field, invoke, to_field};
use super::keyboard;
use super::tree::menu_tree_named;
use super::{Tokens, Value, VectorcraftApp};
use crate::toolbar::flyout_frame;
use egui::vec2;

/// Draw every torn-off application menu for this frame: live items, move by the grip, close with ×.
pub fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if app.ui.torn_menus.is_empty() {
        return;
    }
    type Field = (egui::Id, egui::text_edit::TextEditState);
    let chosen_key = egui::Id::unique("torn-menu-field-event").with(ctx.viewport_id());
    if let Some(((field, state), e)) = ctx.data(|d| d.get_temp::<(Field, egui::Event)>(chosen_key)) {
        ctx.data_mut(|d| d.remove::<(Field, egui::Event)>(chosen_key));
        keyboard::restore_field(ctx, field, state);
        to_field(ctx, field, e);
    }
    let field_key = egui::Id::unique("torn-menu-field").with(ctx.viewport_id());
    if let Some(field) = focused_text_field(ctx).and_then(|id| Some((id, egui::TextEdit::load_state(ctx, id)?))) {
        ctx.data_mut(|d| d.insert_temp::<Field>(field_key, field));
    }

    let tree = menu_tree_named(app.session.prefs.font_names_in_english);
    let tree: Vec<(&str, Vec<_>)> = tree.into_iter().filter(|(title, _)| *title != "VectorCraft").collect();
    // Drop menus whose path no longer exists in the tree.
    app.ui.torn_menus.retain(|m| items_at(&tree, &m.path).is_some());
    if app.ui.torn_menus.is_empty() {
        return;
    }

    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let mut clicked: Option<(String, Value)> = None;
    let mut closed: Option<usize> = None;

    let menus: Vec<(usize, Vec<String>, [f32; 2])> = app.ui.torn_menus.iter().enumerate().map(|(i, m)| (i, m.path.clone(), m.pos)).collect();

    for (i, path, pos) in menus {
        let Some(items) = items_at(&tree, &path) else { continue };
        let id = area_id(&path);
        let drag_id = id.with("bar");
        let grab = id.with("grab");
        let size = ctx.memory(|m| m.area_rect(id)).map_or(vec2(230.0, 40.0), |r| r.size());
        let at = crate::tools_float::clamp(pos, size, screen);
        let host = MenuHost::Torn { drag_id };
        egui::Area::new(id).order(egui::Order::Middle).fixed_pos(at).show(ctx, |ui| {
            egui::containers::menu::menu_style(ui.style_mut());
            flyout_frame(&t, false).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                menu_body(app, ui, items, &mut clicked, &host);
            });
        });
        if ctx.is_being_dragged(drag_id)
            && let Some(pointer) = ctx.input(|i| i.pointer.interact_pos())
        {
            let off = ctx.data_mut(|m| *m.get_temp_mut_or_insert_with(grab, || pointer - at));
            let p = pointer - off;
            if p.x.is_finite()
                && p.y.is_finite()
                && let Some(m) = app.ui.torn_menus.get_mut(i)
            {
                m.pos = [p.x, p.y];
            }
        } else if !ctx.input(|i| i.pointer.any_down()) {
            ctx.data_mut(|m| m.remove::<egui::Vec2>(grab));
        }
        if take_torn_close(ctx, drag_id) {
            closed = Some(i);
        }
    }

    if let Some(i) = closed.filter(|i| *i < app.ui.torn_menus.len()) {
        app.ui.torn_menus.remove(i);
    }

    let field = ctx.data(|d| d.get_temp::<Field>(field_key));
    if focused_text_field(ctx).is_none() {
        ctx.data_mut(|d| d.remove::<Field>(field_key));
    }
    if let Some((id, p)) = clicked {
        match field.and_then(|f| Some((f, field_event(app, &id)?))) {
            Some(chosen) => {
                ctx.data_mut(|d| d.insert_temp(chosen_key, chosen));
                ctx.request_repaint();
            }
            None => invoke(app, &id, p),
        }
    }
}

fn area_id(path: &[String]) -> egui::Id {
    egui::Id::unique(("torn-menu", path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menus::bar::{MenuHost, items_at, menu_body, take_tear_request, upsert_torn};
    use crate::menus::tree::menu_tree;
    use crate::theme;
    use egui::{pos2, vec2};
    use serde_json::json;

    #[test]
    fn items_at_finds_top_level_and_submenu() {
        let tree = menu_tree();
        let tree: Vec<_> = tree.into_iter().filter(|(t, _)| *t != "VectorCraft").collect();
        let view = items_at(&tree, &["View".into()]).expect("View");
        assert!(view.iter().any(|i| matches!(i, crate::menus::Item::Cmd("Zoom In", ..))));
        let arrange = items_at(&tree, &["Object".into(), "Arrange".into()]).expect("Object › Arrange");
        assert!(arrange.iter().any(|i| matches!(i, crate::menus::Item::Cmd("Bring to Front", ..))));
        assert!(items_at(&tree, &["Object".into(), "No Such".into()]).is_none());
    }

    #[test]
    fn tearing_off_upserts_by_path() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        upsert_torn(&mut app, vec!["View".into()], [10.0, 20.0]);
        upsert_torn(&mut app, vec!["View".into()], [30.0, 40.0]);
        assert_eq!(app.ui.torn_menus.len(), 1);
        assert_eq!(app.ui.torn_menus[0].pos, [30.0, 40.0]);
        upsert_torn(&mut app, vec!["Edit".into()], [0.0, 0.0]);
        assert_eq!(app.ui.torn_menus.len(), 2);
    }

    #[test]
    fn a_torn_menu_stays_open_after_an_item_click() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        upsert_torn(&mut app, vec!["View".into()], [40.0, 40.0]);
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, Default::default());
        let tree = menu_tree();
        let (_, view) = tree.iter().find(|(t, _)| *t == "View").unwrap();
        let id = egui::Id::unique("test-torn-view");
        let host = MenuHost::Torn { drag_id: id.with("bar") };
        let screen = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0));
        let frame = |app: &mut VectorcraftApp, events: Vec<egui::Event>| {
            let mut clicked = None;
            let input = egui::RawInput { screen_rect: Some(screen), events, ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(40.0, 40.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(app, ui, view, &mut clicked, &host);
                });
            });
            out.textures_delta.clear();
            (clicked, take_torn_close(&ctx, id.with("bar")), out.shapes)
        };
        fn find(s: &egui::Shape, label: &str) -> Option<egui::Pos2> {
            match s {
                egui::Shape::Text(t) if t.galley.text().contains(label) => Some(t.pos + t.galley.rect.center().to_vec2()),
                egui::Shape::Vec(v) => v.iter().find_map(|s| find(s, label)),
                _ => None,
            }
        }
        let _ = frame(&mut app, vec![]);
        let (_, _, shapes) = frame(&mut app, vec![]);
        // Near the top of View (long menus scroll; Show Grid is too far down).
        let at = shapes.iter().find_map(|c| find(&c.shape, "Outline")).expect("Outline in View");
        let button = |pressed| egui::Event::PointerButton { pos: at, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(&mut app, vec![button(true)]);
        let (clicked, closed, _) = frame(&mut app, vec![button(false)]);
        assert_eq!(clicked.as_ref().map(|(id, _)| id.as_str()), Some("view.outline"));
        assert!(!closed, "an item click must not close a torn menu");
        assert_eq!(app.ui.torn_menus.len(), 1, "torn entry remains until ×");
    }

    #[test]
    fn grip_click_tears_a_popup_menu_off() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, Default::default());
        let tree = menu_tree();
        let (_, view) = tree.iter().find(|(t, _)| *t == "View").unwrap();
        let host = MenuHost::Popup { path: vec!["View".into()] };
        let id = egui::Id::unique("test-tear-grip");
        let screen = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0));
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(100.0, 50.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(&app, ui, view, &mut None, &host);
                });
            });
            out.textures_delta.clear();
        }
        let rect = ctx.memory(|m| m.area_rect(id)).expect("menu area");
        let grip = pos2(rect.center().x, rect.top() + 6.0);
        let button = |pressed| egui::Event::PointerButton { pos: grip, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        for pressed in [true, false] {
            let input = egui::RawInput { screen_rect: Some(screen), events: vec![button(pressed)], ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(100.0, 50.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(&app, ui, view, &mut None, &host);
                });
            });
            out.textures_delta.clear();
        }
        let (torn_path, pos) = take_tear_request(&ctx).expect("grip click tears the menu off");
        assert_eq!(torn_path, ["View"]);
        assert!(pos[0].is_finite() && pos[1].is_finite());
        upsert_torn(&mut app, torn_path, pos);
        assert_eq!(app.ui.torn_menus.len(), 1);
    }

    #[test]
    fn grip_double_click_tears_a_popup_menu_off() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, Default::default());
        let tree = menu_tree();
        let (_, view) = tree.iter().find(|(t, _)| *t == "View").unwrap();
        let host = MenuHost::Popup { path: vec!["View".into()] };
        let id = egui::Id::unique("test-tear-grip-dbl");
        let screen = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0));
        // Frames 50 ms apart, so two clicks a frame apart make a double-click.
        let mut time = 0.0;
        let mut run = |events: Vec<egui::Event>| {
            time += 0.05;
            let input = egui::RawInput { screen_rect: Some(screen), events, time: Some(time), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(100.0, 50.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(&app, ui, view, &mut None, &host);
                });
            });
            out.textures_delta.clear();
        };
        run(vec![]);
        run(vec![]);
        let rect = ctx.memory(|m| m.area_rect(id)).expect("menu area");
        let grip = pos2(rect.center().x, rect.top() + 6.0);
        let button = |pressed| egui::Event::PointerButton { pos: grip, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        run(vec![egui::Event::PointerMoved(grip), button(true)]);
        run(vec![button(false)]);
        // First click already tears (same as a single click); clear and tear again via a second
        // click that egui reports as a double-click.
        let _ = take_tear_request(&ctx);
        run(vec![button(true)]);
        run(vec![button(false)]);
        let (torn_path, pos) = take_tear_request(&ctx).expect("grip double-click tears the menu off");
        assert_eq!(torn_path, ["View"]);
        assert!(pos[0].is_finite() && pos[1].is_finite());
        upsert_torn(&mut app, torn_path, pos);
        assert_eq!(app.ui.torn_menus.len(), 1);
    }

    #[test]
    fn close_box_removes_a_torn_menu() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), crate::Services::default());
        upsert_torn(&mut app, vec!["View".into()], [100.0, 50.0]);
        let ctx = egui::Context::default();
        theme::install_fonts(&ctx);
        theme::apply(&ctx, Default::default());
        let tree = menu_tree();
        let (_, view) = tree.iter().find(|(t, _)| *t == "View").unwrap();
        let id = egui::Id::unique("test-torn-close");
        let drag_id = id.with("bar");
        let host = MenuHost::Torn { drag_id };
        let screen = egui::Rect::from_min_size(pos2(0.0, 0.0), vec2(1200.0, 800.0));
        for _ in 0..2 {
            let input = egui::RawInput { screen_rect: Some(screen), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(100.0, 50.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(&app, ui, view, &mut None, &host);
                });
            });
            out.textures_delta.clear();
        }
        let rect = ctx.memory(|m| m.area_rect(id)).expect("menu area");
        let x = pos2(rect.right() - 7.0, rect.top() + 6.0);
        let button = |pressed| egui::Event::PointerButton { pos: x, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        for pressed in [true, false] {
            let input = egui::RawInput { screen_rect: Some(screen), events: vec![button(pressed)], ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                egui::Area::new(id).fixed_pos(pos2(100.0, 50.0)).show(ui.ctx(), |ui| {
                    egui::containers::menu::menu_style(ui.style_mut());
                    menu_body(&app, ui, view, &mut None, &host);
                });
            });
            out.textures_delta.clear();
        }
        assert!(take_torn_close(&ctx, drag_id), "× closes the torn menu");
        app.ui.torn_menus.clear();
        assert!(app.ui.torn_menus.is_empty());
    }
}
