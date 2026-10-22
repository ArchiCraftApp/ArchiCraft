//! Commands of the Project menu (STR-001, LAY-001, MAT-001, PRJ-009).

use super::{Category, CommandSpec, Handler};

pub(super) fn specs() -> Vec<CommandSpec> {
    let project =
        |id, order, handler: Handler| CommandSpec::new(id, Category::Project, handler).menu("project", 0, order);
    vec![
        project("project.storeys", 0, |app, _, _| app.dialogs.storeys = true),
        project("project.layers", 1, |app, _, _| app.dialogs.layers = true),
        project("project.materials", 2, |app, _, _| app.dialogs.materials = true),
        project("project.settings", 3, |app, _, _| {
            app.dialogs.open_project_settings(&app.ws)
        }),
    ]
}
