//! Commands of the Project menu (STR-001, LAY-001, MAT-001, PRJ-009).

use super::{Category, CommandId, CommandSpec, Handler};

// The project commands the browser's buttons invoke.
pub const LAYERS: CommandId = CommandId::new("project.layers");
pub const MATERIALS: CommandId = CommandId::new("project.materials");
pub const SETTINGS: CommandId = CommandId::new("project.settings");

pub(super) fn specs() -> Vec<CommandSpec> {
    let project =
        |id, order, handler: Handler| CommandSpec::new(id, Category::Project, handler).menu("project", 0, order);
    vec![
        project("project.storeys", 0, |app, _, _| app.dialogs.storeys = true),
        project(LAYERS.as_str(), 1, |app, _, _| app.dialogs.layers = true),
        project(MATERIALS.as_str(), 2, |app, _, _| app.dialogs.materials = true),
        project(SETTINGS.as_str(), 3, |app, _, _| {
            app.dialogs.open_project_settings(&app.ws)
        }),
    ]
}
