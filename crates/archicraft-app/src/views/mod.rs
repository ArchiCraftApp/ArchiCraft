//! Plan and 3D views.

pub mod annotation_draw;
pub mod camera;
pub mod plan;
pub mod render3d;
pub mod transfer;
pub mod view3d;

use crate::commands::{CommandCtx, Origin, Registry, Target, edit};
use crate::ui::command_widgets::menu_item;
use crate::ui::dialogs::Dialogs;
use crate::workspace::{Request, Workspace};

/// Group and Ungroup in the context menu of a view (UI-005, ELM-015), as
/// in the Edit menu: label, shortcut, enabled state and tooltip come from
/// the registry, and a click invokes the command on the selection.
fn group_menu_items(ui: &mut egui::Ui, ws: &mut Workspace) {
    // The views see no dialogs; Group and Ungroup do not depend on them.
    let cx = CommandCtx::new(ws, false, &Dialogs::default());
    for id in [edit::GROUP, edit::UNGROUP] {
        if menu_item(ui, &cx, Registry::get().command(id)) {
            let target = Target::active(ws);
            ws.requests.push(Request::Invoke(id, target, Origin::ContextMenu));
        }
    }
}
