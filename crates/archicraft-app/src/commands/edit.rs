//! Commands of the Edit menu (UND-001, SEL-004).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Enabled, Handler, Scope};
use crate::tr;

// The edit commands the toolbar invokes (UI-011).
pub const UNDO: CommandId = CommandId::new("edit.undo");
pub const REDO: CommandId = CommandId::new("edit.redo");

pub(super) fn specs() -> Vec<CommandSpec> {
    let edit = |id, handler: Handler| CommandSpec::new(id, Category::Edit, handler);
    vec![
        edit(UNDO.as_str(), |app, _, _| app.ws.undo())
            .menu("edit", 0, 0)
            .bind(Binding::primary(Key::Z))
            .enabled_when(|cx| Enabled::when(cx.can_undo, "cmd-reason-nothing-to-undo"))
            .dynamic_label(|cx| {
                cx.undo_label
                    .as_ref()
                    .map(|l| tr!("cmd-edit-undo-named", action = l.as_str()))
            }),
        edit(REDO.as_str(), |app, _, _| app.ws.redo())
            .menu("edit", 0, 1)
            .bind(Binding::primary(Key::Z).shift())
            .bind(Binding::primary(Key::Y))
            .enabled_when(|cx| Enabled::when(cx.can_redo, "cmd-reason-nothing-to-redo"))
            .dynamic_label(|cx| {
                cx.redo_label
                    .as_ref()
                    .map(|l| tr!("cmd-edit-redo-named", action = l.as_str()))
            }),
        // Backspace deletes too, but only while no tool takes point input,
        // where it removes the last point.
        edit("edit.delete", |app, _, _| app.ws.delete_selection())
            .menu("edit", 1, 0)
            .bind(Binding::key(Key::Delete))
            .bind(Binding::key(Key::Backspace))
            .scope(Scope::Canvas)
            .enabled_when(|cx| Enabled::when(!cx.selection_empty, "cmd-reason-nothing-selected")),
        edit("edit.select-all", |app, _, _| app.ws.select_all())
            .menu("edit", 1, 1)
            .bind(Binding::primary(Key::A)),
        edit("edit.settings", |app, _, _| app.dialogs.settings = true)
            .menu("edit", 2, 0)
            .bind(Binding::primary(Key::T))
            .scope(Scope::TextSafe),
    ]
}
