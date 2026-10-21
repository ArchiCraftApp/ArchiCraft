//! Pointer gestures as point input sees them, and the input mode that turns
//! them into points (INP-020).

use serde::{Deserialize, Serialize};

/// How pointer gestures define points (INP-020): click-click by default,
/// press-drag-release as a user preference.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputMode {
    /// Each click takes a point.
    #[default]
    ClickClick,
    /// A press takes a point and the release after a drag takes the next;
    /// a click still takes one point.
    DragRelease,
}

/// A primary-button event of a view, at the cursor position passed with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEvent {
    /// The button went down at the cursor and the gesture became a drag.
    Pressed,
    /// The button went up at the cursor after a press.
    Released,
    /// A press and release that was no drag.
    Clicked,
    DoubleClicked,
}
