//! The project browser's icons (VIEW-003, UI-001): a plan sheet at each
//! storey, a camera at each perspective view and a tall box at each
//! parallel view, tinted like the row's text, and the whole row still
//! opens its view.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use archicraft_app::ArchiCraftApp;
use archicraft_app::commands::{Origin, Target, view3d};
use archicraft_app::persistence::prefs::ThemeChoice;
use archicraft_app::ui::icons;
use archicraft_app::workspace::Pane;
use archicraft_model::project::Projection;
use archicraft_model::{StoreyId, ViewId};
use common::harness;
use egui::epaint::{Color32, Rect, Shape};
use egui::{Event, Modifiers, PointerButton, Pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

type App = Harness<'static, ArchiCraftApp>;

/// What the last frame painted inside a browser row: its images (the
/// texture's name, which egui takes from the icon's uri, where it was
/// painted and its tint) and the colours of its text.
#[derive(Debug, Default)]
struct Painted {
    images: Vec<(String, Rect, Color32)>,
    text: Vec<(Rect, Color32)>,
}

fn collect(h: &App, shape: &Shape, row: Rect, out: &mut Painted) {
    match shape {
        Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(h, s, row, out)),
        Shape::Mesh(mesh) => {
            let bounds = mesh.calc_bounds();
            if row.contains_rect(bounds) && !mesh.vertices.is_empty() {
                let name = h.ctx.tex_manager().read().meta(mesh.texture_id).map(|m| m.name.clone());
                out.images
                    .push((name.unwrap_or_default(), bounds, mesh.vertices[0].color));
            }
        }
        // egui paints an image as a rect filled with its texture.
        Shape::Rect(rect) if rect.brush.is_some() && row.contains_rect(rect.rect) => {
            let texture = rect.brush.as_ref().unwrap().fill_texture_id;
            let name = h.ctx.tex_manager().read().meta(texture).map(|m| m.name.clone());
            out.images.push((name.unwrap_or_default(), rect.rect, rect.fill));
        }
        Shape::Text(text) => {
            let bounds = text.visual_bounding_rect();
            if row.contains_rect(bounds) {
                out.text
                    .push((bounds, text.override_text_color.unwrap_or(text.fallback_color)));
            }
        }
        _ => {}
    }
}

fn painted_in(h: &App, row: Rect) -> Painted {
    let mut out = Painted::default();
    for clipped in &h.output().shapes {
        collect(h, &clipped.shape, row, &mut out);
    }
    out
}

/// The rect of the browser row whose label contains `text`: between the
/// browser's "Storeys" and "Project" headers, so neither a tab nor the
/// status line.
fn row(h: &App, text: &str) -> Rect {
    let browser = h.get_by_label("Storeys").rect();
    let project = h
        .query_all_by_label("Project")
        .map(|n| n.rect())
        .find(|r| r.min.x >= browser.min.x - 1.0 && r.min.y > browser.max.y)
        .expect("the browser has a Project header");
    let rows: Vec<Rect> = h
        .query_all_by_label_contains(text)
        .map(|n| n.rect())
        .filter(|r| r.min.x >= browser.min.x - 1.0 && r.min.y > browser.max.y && r.max.y < project.min.y)
        .collect();
    assert_eq!(rows.len(), 1, "browser rows with “{text}”: {rows:?}");
    rows[0]
}

/// The one icon of the row with `text`, after checking that it sits before
/// the text, at the text's height and in the text's colour.
fn icon_of(h: &App, text: &str) -> String {
    let rect = row(h, text);
    let painted = painted_in(h, rect);
    assert_eq!(painted.images.len(), 1, "“{text}” paints {painted:?}");
    let (name, icon, tint) = painted.images[0].clone();
    let (label, colour) = *painted.text.first().unwrap_or_else(|| panic!("“{text}” has no text"));
    assert!(icon.max.x <= label.min.x, "the icon of “{text}” is not before its text");
    assert_eq!(tint, colour, "the icon of “{text}” is not tinted like its text");
    assert!(
        (icon.center().y - rect.center().y).abs() <= 1.0,
        "the icon of “{text}” is not centred in its row: {icon:?} in {rect:?}"
    );
    assert!(
        icon.height() >= label.height() - 2.0 && icon.height() <= rect.height(),
        "the icon of “{text}” is not at row height: {icon:?}, text {label:?}, row {rect:?}"
    );
    name
}

fn uri(icon: egui::ImageSource<'static>) -> String {
    icon.uri().unwrap().to_owned()
}

/// Clicks the primary button at `pos`.
fn click_at(h: &mut App, pos: Pos2) {
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        });
    }
    h.run_steps(2);
}

fn storey_names(h: &mut App) -> Vec<(StoreyId, String)> {
    let storeys = &h.state_mut().workspace().doc.project.storeys;
    storeys.highest_first().map(|s| (s.id, s.name.clone())).collect()
}

fn view_named(h: &mut App, name: &str) -> ViewId {
    let views = &h.state_mut().workspace().doc.project.views;
    views
        .iter()
        .find(|v| v.name == name)
        .expect("the template has this view")
        .id
}

fn active_pane(h: &mut App) -> Option<Pane> {
    h.state_mut().workspace().active_pane
}

/// VIEW-003, UI-001: every storey row of the browser draws the plan sheet,
/// "3D perspective" the camera and "3D parallel" the parallel box, each
/// before its text, at row height and in the row's text colour, in the
/// light and the dark theme, selected or not. A click on the icon opens
/// the plan or the 3D view: the whole row is one target.
#[test]
fn browser_rows_show_the_plan_sheet_camera_and_parallel_box() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let mut h = harness(&format!("browser-icons-{theme:?}"));
        h.state_mut().workspace().prefs.theme = theme;
        h.run_steps(2);
        let storeys = storey_names(&mut h);
        assert!(storeys.len() >= 2, "{storeys:?}");
        for (_, name) in &storeys {
            assert_eq!(icon_of(&h, name), uri(icons::PLAN_SHEET), "{name}");
        }
        assert_eq!(icon_of(&h, "3D perspective"), uri(icons::CAMERA));
        assert_eq!(icon_of(&h, "3D parallel"), uri(icons::PARALLEL_BOX));

        for (id, name) in &storeys {
            let icon = painted_in(&h, row(&h, name)).images[0].1;
            click_at(&mut h, icon.center());
            assert_eq!(active_pane(&mut h), Some(Pane::Plan(*id)), "{theme:?}: {name}");
            // Selected now: still the plan sheet in the text's colour.
            assert_eq!(icon_of(&h, name), uri(icons::PLAN_SHEET), "{name}");
        }
        for (name, icon) in [("3D perspective", icons::CAMERA), ("3D parallel", icons::PARALLEL_BOX)] {
            let view = view_named(&mut h, name);
            let at = painted_in(&h, row(&h, name)).images[0].1;
            click_at(&mut h, at.center());
            assert_eq!(active_pane(&mut h), Some(Pane::View3d(view)), "{theme:?}: {name}");
            assert_eq!(icon_of(&h, name), uri(icon), "{name}");
        }
    }
}

/// VIEW-003, VIEW-020: a view's icon follows its projection. Edited in the
/// project, the closed "3D perspective" shows the parallel box; opened, its
/// view-control bar's view3d.toggle-projection switches what it shows, and
/// the icon with it.
#[test]
fn switching_a_views_projection_switches_its_icon() {
    let mut h = harness("browser-icons-projection");
    let view = view_named(&mut h, "3D perspective");
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::CAMERA));

    let ws = h.state_mut().workspace();
    ws.doc
        .project
        .views
        .iter_mut()
        .find(|v| v.id == view)
        .unwrap()
        .projection = Projection::Parallel;
    h.run_steps(2);
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::PARALLEL_BOX));

    let at = painted_in(&h, row(&h, "3D perspective")).images[0].1;
    click_at(&mut h, at.center());
    assert_eq!(active_pane(&mut h), Some(Pane::View3d(view)));
    assert_eq!(icon_of(&h, "3D perspective"), uri(icons::PARALLEL_BOX));
    for expected in [icons::CAMERA, icons::PARALLEL_BOX] {
        let ctx = h.ctx.clone();
        let app = h.state_mut();
        let target = Target::active(app.workspace());
        app.invoke(&ctx, view3d::TOGGLE_PROJECTION, target, Origin::Test);
        h.run_steps(2);
        assert_eq!(icon_of(&h, "3D perspective"), uri(expected));
    }
}

/// VIEW-003: the icon is chosen in one place, by projection, so saved
/// cameras show the same icons as the 3D views.
#[test]
fn the_view_icon_is_chosen_by_projection() {
    assert_eq!(uri(icons::view_icon(Projection::Perspective)), uri(icons::CAMERA));
    assert_eq!(uri(icons::view_icon(Projection::Parallel)), uri(icons::PARALLEL_BOX));
}
