//! What a command sees when it is shown or invoked (UI-017): a snapshot of
//! the application state, the target it acts on and where the invocation
//! came from.

use archicraft_model::project::Projection;
use archicraft_model::{ElementId, StoreyId};

use super::Keymap;
use crate::tools::ToolKind;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

/// A cheap snapshot of the state that decides whether commands are enabled,
/// checked and how they are labelled, built once per frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandCtx {
    pub selection_empty: bool,
    /// The groups and ungrouped elements the selection consists of, each
    /// group counted once (ELM-015); Group needs two.
    pub selection_items: usize,
    /// A selected element belongs to a group, which Ungroup can remove.
    pub selection_grouped: bool,
    /// Groups are suspended for the session (ELM-015).
    pub groups_suspended: bool,
    pub active_pane: Option<Pane>,
    /// A tool takes point input, so keys belong to it (UI-008).
    pub drawing: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    /// The label of the change Undo takes back, in the current language.
    pub undo_label: Option<String>,
    /// The label of the change Redo repeats, in the current language.
    pub redo_label: Option<String>,
    /// A text field has focus (UI-008).
    pub text_focused: bool,
    /// The problems panel is open.
    pub problems_open: bool,
    pub reference_lines_shown: bool,
    pub user_origin_placed: bool,
    /// The active tool; the toolbox and the Design menu check it.
    pub tool: ToolKind,
    /// The state of the active 3D view, if a 3D view is active.
    pub view3d: Option<View3dCtx>,
    /// The keys that run each command, for the shortcuts shown (UI-009,
    /// UI-011).
    pub keymap: Keymap,
}

impl CommandCtx {
    /// The snapshot of `ws`; the rest of the application state is passed in.
    pub fn new(ws: &Workspace, text_focused: bool, problems_open: bool) -> Self {
        let history = &ws.doc.history;
        let view3d = match ws.active_pane {
            Some(Pane::View3d(view)) => ws.views_3d.get(&view).map(|state| View3dCtx {
                mode: state.mode,
                projection: state.camera.projection,
                orbit: state.orbit,
            }),
            _ => None,
        };
        let (groups, loose) = ws.selection_items();
        Self {
            selection_empty: ws.selection.is_empty(),
            selection_items: groups.len() + loose.len(),
            selection_grouped: !groups.is_empty(),
            groups_suspended: ws.groups_suspended,
            active_pane: ws.active_pane,
            drawing: ws.input.captures_keys(ws.consumer()),
            can_undo: history.can_undo(),
            can_redo: history.can_redo(),
            undo_label: history.undo_label().map(|l| ws.render(&l)),
            redo_label: history.redo_label().map(|l| ws.render(&l)),
            text_focused,
            problems_open,
            reference_lines_shown: ws.prefs.show_reference_lines,
            user_origin_placed: ws.frames.user_origin.is_placed(),
            tool: ws.tool.kind,
            view3d,
            keymap: ws.keymap.clone(),
        }
    }
}

/// The state of the active 3D view that its commands show (VIEW-014,
/// VIEW-020).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3dCtx {
    pub mode: DisplayMode,
    pub projection: Projection,
    pub orbit: bool,
}

/// Whether a command can run now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enabled {
    Yes,
    /// No, for the reason with this catalogue key.
    No(&'static str),
}

impl Enabled {
    pub fn is_yes(self) -> bool {
        self == Enabled::Yes
    }

    /// [`Enabled::Yes`] if `condition` holds, otherwise `No(reason)`.
    pub fn when(condition: bool, reason: &'static str) -> Self {
        if condition { Enabled::Yes } else { Enabled::No(reason) }
    }
}

/// What a command acts on. Menus and shortcuts act on the active storey and
/// pane; a context menu passes the item it was opened on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Target {
    pub storey: StoreyId,
    pub pane: Option<Pane>,
    pub element: Option<ElementId>,
}

impl Target {
    /// The active storey and pane, without an element.
    pub fn active(ws: &Workspace) -> Self {
        Self {
            storey: ws.active_storey,
            pane: ws.active_pane,
            element: None,
        }
    }
}

/// Where an invocation came from, for logging and the test recorder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Origin {
    Menu,
    Shortcut,
    Palette,
    ContextMenu,
    Toolbar,
    /// A button of a panel or dialog, such as the browser.
    Panel,
    Test,
}

/// One invocation, as the test recorder captures it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub id: super::CommandId,
    pub target: Target,
    pub origin: Origin,
}
