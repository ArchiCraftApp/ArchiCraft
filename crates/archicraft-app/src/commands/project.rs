//! Commands of the Project menu (STR-001, LAY-001, MAT-001, PRJ-009).

use super::{Category, CommandSpec};

pub(super) fn specs() -> Vec<CommandSpec> {
    let project = |id, order| CommandSpec::new(id, Category::Project).menu("project", 0, order);
    vec![
        project("project.storeys", 0),
        project("project.layers", 1),
        project("project.materials", 2),
        project("project.settings", 3),
    ]
}
