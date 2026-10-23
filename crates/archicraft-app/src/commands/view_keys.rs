//! The keys the plan view handles itself (HW-003, NFR-A11Y-001, UI-009):
//! cycling the wall reference line (WAL-007), the overlapping candidates
//! (SEL-001) and the keys of point input (INP-003). They are commands, so
//! they are listed with the other shortcuts and can be rebound, but they are
//! [local](super::CommandSpec::local): the plan view matches their keys
//! against the [`Keymap`](super::Keymap) and the state of its tool. Invoked
//! another way (the palette, tests), they wait in
//! [`Workspace::view_commands`] for the active plan view, except the wall
//! reference line, which needs no view.
//!
//! [`Workspace::view_commands`]: crate::workspace::Workspace::view_commands

use egui::Key;

use super::{Binding, Category, CommandId, CommandSpec, Handler, Scope};

/// Cycles the reference-line location of the wall tool (WAL-007).
pub const CYCLE_REFERENCE_LINE: CommandId = CommandId::new("tool.wall.cycle-reference-line");
/// Highlights the next of the elements under the cursor; the next click
/// selects it (SEL-001).
pub const CYCLE_CANDIDATES: CommandId = CommandId::new("canvas.cycle-candidates");
/// Moves to the next typed field, or to the next snap candidate while
/// several are under the cursor (INP-003, INP-009).
pub const NEXT_FIELD: CommandId = CommandId::new("input.next-field");
/// Removes the last typed character, or the last point (INP-003).
pub const REMOVE_LAST: CommandId = CommandId::new("input.remove-last");
/// Takes the typed point, or ends the chain (INP-003).
pub const FINISH: CommandId = CommandId::new("input.finish");
/// Cancels one step: the typed text, the last point, the selection, the
/// tool (INP-003).
pub const CANCEL_STEP: CommandId = CommandId::new("input.cancel-step");

/// The local commands of the plan view.
pub const ALL: [CommandId; 6] = [
    CYCLE_REFERENCE_LINE,
    CYCLE_CANDIDATES,
    NEXT_FIELD,
    REMOVE_LAST,
    FINISH,
    CANCEL_STEP,
];

pub(super) fn specs() -> Vec<CommandSpec> {
    let local = |id: CommandId, category, key, scope, handler: Handler| {
        CommandSpec::new(id.as_str(), category, handler)
            .palette_only()
            .local()
            .bind(Binding::key(key))
            .scope(scope)
    };
    vec![
        // Global: R cycles the reference line whenever the wall tool is
        // active, before the first point too.
        local(
            CYCLE_REFERENCE_LINE,
            Category::Design,
            Key::R,
            Scope::Global,
            |app, _, _| app.ws.cycle_wall_reference_line(),
        ),
        local(
            CYCLE_CANDIDATES,
            Category::Edit,
            Key::Tab,
            Scope::Canvas,
            |app, _, _| app.ws.view_commands.push(CYCLE_CANDIDATES),
        ),
        local(
            NEXT_FIELD,
            Category::Design,
            Key::Tab,
            Scope::PointInput,
            |app, _, _| app.ws.view_commands.push(NEXT_FIELD),
        ),
        local(
            REMOVE_LAST,
            Category::Design,
            Key::Backspace,
            Scope::PointInput,
            |app, _, _| app.ws.view_commands.push(REMOVE_LAST),
        ),
        // Enter and Esc work before the first point too (Esc leaves the tool).
        local(FINISH, Category::Design, Key::Enter, Scope::Global, |app, _, _| {
            app.ws.view_commands.push(FINISH)
        })
        .fixed(),
        local(
            CANCEL_STEP,
            Category::Design,
            Key::Escape,
            Scope::Global,
            |app, _, _| app.ws.view_commands.push(CANCEL_STEP),
        )
        .fixed(),
    ]
}
