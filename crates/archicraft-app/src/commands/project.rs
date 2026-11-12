//! Commands of the Project menu, the storeys and the materials (UI-017,
//! STR-001, STR-003, STR-004, LAY-001, MAT-001, PRJ-009).

use archicraft_model::{Command, Material, MaterialId, Storey, StoreyId};

use super::{Category, CommandId, CommandSpec, Enabled, Handler};
use crate::ArchiCraftApp;
use crate::tr;
use crate::ui::icons;
use crate::workspace::Workspace;

pub const STOREYS: CommandId = CommandId::new("project.storeys");
pub const LAYERS: CommandId = CommandId::new("project.layers");
pub const MATERIALS: CommandId = CommandId::new("project.materials");
pub const SETTINGS: CommandId = CommandId::new("project.settings");
// The storey commands act on the target's storey: the active one from the
// menus and the palette, the clicked one from a context menu or the Storeys
// dialog (STR-003).
pub const INSERT_STOREY_ABOVE: CommandId = CommandId::new("storey.insert-above");
pub const INSERT_STOREY_BELOW: CommandId = CommandId::new("storey.insert-below");
pub const EDIT_STOREY: CommandId = CommandId::new("storey.edit");
/// Asks how to handle the storey's elements first (STR-004).
pub const DELETE_STOREY: CommandId = CommandId::new("storey.delete");
/// Duplicates the material selected in the Materials dialog (MAT-001).
pub const DUPLICATE_MATERIAL: CommandId = CommandId::new("material.duplicate");

pub(super) fn specs() -> Vec<CommandSpec> {
    let project = |id: CommandId, group, order, handler: Handler| {
        CommandSpec::new(id.as_str(), Category::Project, handler).menu("project", group, order)
    };
    vec![
        project(STOREYS, 0, 0, |app, _, _| app.dialogs.storeys = true),
        // The toolbar shows these three as icons (UI-001, UI-011).
        project(LAYERS, 0, 1, |app, _, _| app.dialogs.layers = true).icon(icons::LAYERS),
        project(MATERIALS, 0, 2, |app, _, _| app.dialogs.materials = true).icon(icons::MATERIALS),
        project(SETTINGS, 0, 3, |app, _, _| app.dialogs.open_project_settings(&app.ws)).icon(icons::SETTINGS),
        project(INSERT_STOREY_ABOVE, 1, 0, |app, _, target| {
            insert_storey(app, target.storey, true)
        }),
        project(INSERT_STOREY_BELOW, 1, 1, |app, _, target| {
            insert_storey(app, target.storey, false)
        }),
        project(EDIT_STOREY, 1, 2, |app, _, target| {
            app.dialogs.storeys = true;
            app.dialogs.selected_storey = Some(target.storey);
        }),
        project(DELETE_STOREY, 1, 3, |app, _, target| {
            app.dialogs.ask_delete_storey(target.storey)
        }),
        CommandSpec::new(DUPLICATE_MATERIAL.as_str(), Category::Project, duplicate_material)
            .palette_only()
            .enabled_when(|cx| Enabled::when(cx.material_selected, "cmd-reason-no-material-selected")),
    ]
}

/// A new storey for inserting next to `reference`, named by its future
/// number in the user's language.
pub fn new_storey(ws: &Workspace, reference: StoreyId, above: bool) -> Storey {
    let project = &ws.doc.project;
    let number = project.storeys.number(reference).unwrap_or(0) + if above { 1 } else { 0 };
    let height = project.storeys.get(reference).map_or(3.0, |s| s.height);
    let name = if number >= 0 {
        tr!("browser-new-storey", number = format!("{number:02}"))
    } else {
        tr!("browser-new-basement", number = format!("{:02}", -number))
    };
    Storey::new(name, 0.0, height)
}

fn insert_storey(app: &mut ArchiCraftApp, reference: StoreyId, above: bool) {
    if app.ws.doc.project.storeys.get(reference).is_none() {
        return;
    }
    let storey = new_storey(&app.ws, reference, above);
    app.ws.execute(Command::InsertStorey {
        reference,
        above,
        storey,
    });
}

/// Adds a copy of the material selected in the Materials dialog and selects
/// the copy.
fn duplicate_material(app: &mut ArchiCraftApp, _: &egui::Context, _: super::Target) {
    let Some(material) = app
        .dialogs
        .selected_material
        .and_then(|id| app.ws.doc.project.material(id))
        .cloned()
    else {
        return;
    };
    let copy = Material {
        id: MaterialId::new(),
        name: format!("{} copy", material.name),
        ..material
    };
    app.dialogs.selected_material = Some(copy.id);
    app.ws.execute(Command::AddMaterial(copy));
}
