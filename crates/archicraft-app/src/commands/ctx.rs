//! What a command sees when it is shown or invoked (UI-017): a snapshot of
//! the application state, the target it acts on and where the invocation
//! came from.

use archicraft_model::{ElementId, StoreyId};

use crate::workspace::{Pane, Workspace};

/// A cheap snapshot of the state that decides whether commands are enabled,
/// checked and how they are labelled, built once per frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CommandCtx {
    pub selection_empty: bool,
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
}

impl CommandCtx {
    /// The snapshot of `ws`; the rest of the application state is passed in.
    pub fn new(ws: &Workspace, text_focused: bool, problems_open: bool) -> Self {
        let history = &ws.doc.history;
        Self {
            selection_empty: ws.selection.is_empty(),
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
        }
    }
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
    Test,
}

/// One invocation, as the test recorder captures it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub id: super::CommandId,
    pub target: Target,
    pub origin: Origin,
}
