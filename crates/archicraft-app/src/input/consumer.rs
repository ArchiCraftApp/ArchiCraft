//! The contract between point input and the commands that consume points
//! (INP-001, NFR-USE-003).
//!
//! Element tools, transforms, reshaping, insertion, measuring and
//! dimensioning all implement [`InputConsumer`]: they describe each step,
//! preview the result for a resolved point and turn committed points into
//! commands. Point input does the rest, identically for every consumer.

use archicraft_model::glam::DVec2;
use archicraft_model::{Command, Project, StoreyId};

use super::Resolved;

/// A key and what it does in the current step, shown after the prompt
/// (NFR-USE-003).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyHint {
    pub key: &'static str,
    pub action: String,
}

impl KeyHint {
    pub fn new(key: &'static str, action: impl Into<String>) -> Self {
        Self {
            key,
            action: action.into(),
        }
    }
}

/// What a step expects, which decides how typed values and the angle
/// constraint turn into a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Expect {
    /// A free point; typed values are polar from the reference point, if any.
    Point,
    /// A direction from `from`; a typed angle is absolute.
    Direction { from: DVec2 },
    /// An angle around `centre`; a typed angle is measured from the direction
    /// towards the reference point (a rotation angle).
    Angle { centre: DVec2 },
    /// A point at a distance from `from`; typed values are polar from `from`.
    Distance { from: DVec2 },
}

impl Expect {
    /// The point typed values and the angle constraint are relative to.
    pub fn origin(self, reference: Option<DVec2>) -> Option<DVec2> {
        match self {
            Expect::Point => reference,
            Expect::Direction { from } | Expect::Distance { from } => Some(from),
            Expect::Angle { centre } => Some(centre),
        }
    }
}

/// How the tracker next to the cursor reports values (INP-002).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrackerMode {
    /// Distance and angle from the origin of the step.
    #[default]
    Relative,
}

/// One step of a consumer: the prompt naming the expected input and the keys
/// that apply (INP-001, NFR-USE-003).
#[derive(Clone, Debug, PartialEq)]
pub struct StepSpec {
    pub prompt: String,
    pub hints: Vec<KeyHint>,
    pub expects: Expect,
    pub tracker: TrackerMode,
    /// The step repeats after a commit (a chain, a polygon, multiple copies)
    /// until the user finishes it.
    pub repeat: bool,
}

impl StepSpec {
    pub fn new(prompt: impl Into<String>, expects: Expect) -> Self {
        Self {
            prompt: prompt.into(),
            hints: Vec::new(),
            expects,
            tracker: TrackerMode::Relative,
            repeat: false,
        }
    }

    /// The prompt line: the prompt followed by the key hints (NFR-USE-003).
    pub fn status_line(&self) -> String {
        let mut line = self.prompt.clone();
        for hint in &self.hints {
            line.push_str(&format!(" · {} {}", hint.key, hint.action));
        }
        line
    }
}

/// How the cursor looks while a consumer previews.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CursorState {
    #[default]
    Normal,
}

/// The live preview of a consumer for a resolved point (INP-001).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// Outlines in plane coordinates; `true` closes the ring.
    pub outlines: Vec<(Vec<DVec2>, bool)>,
    pub cursor: CursorState,
}

/// What a consumer wants after a commit or a finish.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Waiting for the next input.
    Continue,
    /// Run this command; the consumer goes on with its next step.
    Execute(Command),
    /// The consumer ended without a command.
    Done,
}

/// A command that consumes points from point input (INP-001).
pub trait InputConsumer {
    /// The current step, or `None` when the consumer expects no point.
    fn step(&self) -> Option<StepSpec>;
    /// The point the current step is relative to: typed values, the angle
    /// constraint and perpendicular snaps start there.
    fn reference_point(&self) -> Option<DVec2>;
    /// The live preview for a resolved point.
    fn preview(&self, resolved: &Resolved) -> Preview;
    /// Takes a resolved point on `storey`.
    fn commit(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> Outcome;
    /// Cancels the current step (INP-003). Returns `false` when there was
    /// nothing to cancel.
    fn cancel_step(&mut self) -> bool;
    /// Removes the last point (UND-003). Returns `false` when there was none.
    fn backspace(&mut self) -> bool;
    /// Ends a repeating step (Enter or double-click).
    fn finish(&mut self, project: &Project) -> Outcome;
}
