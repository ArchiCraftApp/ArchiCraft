//! Commands of the Edit menu (UND-001, SEL-004, ELM-015, ELM-003).

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Enabled, Handler, Scope};
use crate::tr;
use crate::ui::icons;
use crate::views::transfer::TransferMode;

// The edit commands the toolbar invokes (UI-011).
pub const UNDO: CommandId = CommandId::new("edit.undo");
pub const REDO: CommandId = CommandId::new("edit.redo");
pub const GROUP: CommandId = CommandId::new("edit.group");
pub const UNGROUP: CommandId = CommandId::new("edit.ungroup");
// The parameter transfer commands that latch pick-up and inject (ELM-003,
// EDT-020).
pub const PICK_UP_PARAMETERS: CommandId = CommandId::new("edit.pick-up-parameters");
pub const INJECT_PARAMETERS: CommandId = CommandId::new("edit.inject-parameters");
// The command the status bar's indicator turns off (ELM-015).
pub const SUSPEND_GROUPS: CommandId = CommandId::new("edit.suspend-groups");

pub(super) fn specs() -> Vec<CommandSpec> {
    let edit = |id, handler: Handler| CommandSpec::new(id, Category::Edit, handler);
    vec![
        edit(UNDO.as_str(), |app, _, _| app.ws.undo())
            .menu("edit", 0, 0)
            .bind(Binding::primary(Key::Z))
            .enabled_when(|cx| Enabled::when(cx.can_undo, "cmd-reason-nothing-to-undo"))
            .icon(icons::UNDO)
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
            .icon(icons::REDO)
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
        // ELM-015: groups of the selection.
        edit(GROUP.as_str(), |app, _, _| app.ws.group_selection())
            .menu("edit", 2, 0)
            .bind(Binding::primary(Key::G))
            .enabled_when(|cx| Enabled::when(cx.selection_items >= 2, "cmd-reason-nothing-to-group"))
            .icon(icons::GROUP),
        edit(UNGROUP.as_str(), |app, _, _| app.ws.ungroup_selection())
            .menu("edit", 2, 1)
            .bind(Binding::primary(Key::G).shift())
            .enabled_when(|cx| Enabled::when(cx.selection_grouped, "cmd-reason-nothing-grouped"))
            .icon(icons::UNGROUP),
        edit(SUSPEND_GROUPS.as_str(), |app, _, _| app.ws.toggle_groups_suspended())
            .menu("edit", 2, 2)
            .checked_when(|cx| cx.groups_suspended),
        edit("edit.settings", |app, _, _| app.dialogs.settings = true)
            .menu("edit", 3, 0)
            .bind(Binding::primary(Key::T))
            .scope(Scope::TextSafe),
        // ELM-003, EDT-020: the explicit commands of the transfer gestures
        // latch their mode until Esc or a second invocation; the gesture is
        // their accelerator (D-7).
        edit(PICK_UP_PARAMETERS.as_str(), |app, _, _| {
            app.ws.transfer.latch(TransferMode::PickUp)
        })
        .menu("edit", 3, 1)
        .checked_when(|cx| cx.transfer == Some(TransferMode::PickUp))
        .gesture(TransferMode::PickUp)
        .icon(icons::PICK_UP),
        edit(INJECT_PARAMETERS.as_str(), |app, _, _| {
            app.ws.transfer.latch(TransferMode::Inject)
        })
        .menu("edit", 3, 2)
        .checked_when(|cx| cx.transfer == Some(TransferMode::Inject))
        .gesture(TransferMode::Inject)
        .icon(icons::INJECT),
    ]
}
