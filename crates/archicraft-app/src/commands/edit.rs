//! Commands of the Edit menu (UND-001, SEL-004).

use egui::Key;

use super::{Binding, Category, CommandSpec, Scope};

pub(super) fn specs() -> Vec<CommandSpec> {
    let edit = |id| CommandSpec::new(id, Category::Edit);
    vec![
        edit("edit.undo").menu("edit", 0, 0).bind(Binding::primary(Key::Z)),
        edit("edit.redo")
            .menu("edit", 0, 1)
            .bind(Binding::primary(Key::Z).shift())
            .bind(Binding::primary(Key::Y)),
        // Backspace deletes too, but only while no tool takes point input,
        // where it removes the last point.
        edit("edit.delete")
            .menu("edit", 1, 0)
            .bind(Binding::key(Key::Delete))
            .bind(Binding::key(Key::Backspace))
            .scope(Scope::Canvas),
        edit("edit.select-all")
            .menu("edit", 1, 1)
            .bind(Binding::primary(Key::A)),
        edit("edit.settings")
            .menu("edit", 2, 0)
            .bind(Binding::primary(Key::T))
            .scope(Scope::TextSafe),
    ]
}
