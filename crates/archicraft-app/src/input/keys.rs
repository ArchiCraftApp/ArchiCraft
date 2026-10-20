//! Keyboard input of point input, independent of the windowing toolkit
//! (INP-003): views translate their key events into these types.

/// The modifier keys held during an input. Shift constrains angles
/// (INP-006); Command (Ctrl on Windows and Linux) switches a drawing tool to
/// selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub command: bool,
    pub alt: bool,
}

/// A key press that point input handles (INP-003): characters go to the
/// active typed field, Tab moves between fields, Enter commits, Backspace
/// removes a character or a point and Esc cancels one step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyInput {
    Text(String),
    Tab,
    Enter,
    Backspace,
    Escape,
}
