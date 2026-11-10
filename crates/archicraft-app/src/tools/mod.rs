//! Element tools (WAL-001, SLB-001, INP-002…INP-009) and the measure tool
//! (MEA-001).
//!
//! A tool collects points in plan; snapping, the constraint modifier and
//! typed distances (`crate::input::PointInput`) are applied before a point
//! reaches the tool. When a tool has enough points, it produces a command.
//! Tools are point-input consumers (`InputConsumer`, INP-001). With contour
//! detection on, the wall and slab tools create their elements from a picked
//! contour instead (INP-018); the space tool detects rooms bounded by walls
//! (SPC-001). The text tool's click opens an inline editor at the anchor
//! (ANN-007).

mod contour;
mod measure;
mod slab;
mod space;
mod text;
mod user_origin;
mod wall;

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use archicraft_model::{BoundarySource, Command, Element, ElementKind, KindTag, Project, StoreyId};

use crate::input::{Expect, InputConsumer, KeyHint, Outcome, Preview, Resolved, StepSpec};
use crate::tr;

pub use contour::{Accepts, Contour, ContourDetection};
pub use measure::{MeasureMode, MeasureTool, Measurement};
pub use slab::SlabMode;
pub use space::{SpaceDetection, SpaceMode};
pub use text::TextEditor;
pub use user_origin::UserOriginTool;
pub use wall::WallMode;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolKind {
    #[default]
    Select,
    Wall,
    Slab,
    /// Spaces, detected inside walls or drawn (SPC-001).
    Space,
    /// Texts, typed in an inline editor at the clicked anchor (ANN-007).
    Text,
    /// Measures distances and angles, creating nothing (MEA-001).
    Measure,
}

impl ToolKind {
    /// The element kind whose defaults the tool places, none for the
    /// selection and the measure tool (ELM-001).
    pub fn element_kind(self) -> Option<KindTag> {
        match self {
            ToolKind::Select | ToolKind::Measure => None,
            ToolKind::Wall => Some(KindTag::Wall),
            ToolKind::Slab => Some(KindTag::Slab),
            ToolKind::Space => Some(KindTag::Space),
            ToolKind::Text => Some(KindTag::Text),
        }
    }

    /// The tool that places elements of `kind`, if there is one.
    pub fn for_kind(kind: KindTag) -> Option<ToolKind> {
        ToolKind::ALL.into_iter().find(|tool| tool.element_kind() == Some(kind))
    }

    pub const ALL: [ToolKind; 6] = [
        ToolKind::Select,
        ToolKind::Wall,
        ToolKind::Slab,
        ToolKind::Space,
        ToolKind::Text,
        ToolKind::Measure,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Select => "Select",
            ToolKind::Wall => "Wall",
            ToolKind::Slab => "Slab",
            ToolKind::Space => "Space",
            ToolKind::Text => "Text",
            ToolKind::Measure => "Measure",
        }
    }
}

/// The tool's state: which tool, its geometry method, and the points entered
/// so far. The measure tool keeps its own points, which stay shown after a
/// measurement is finished (MEA-001).
#[derive(Clone, Debug)]
pub struct ToolState {
    pub kind: ToolKind,
    pub wall_mode: WallMode,
    pub slab_mode: SlabMode,
    pub space_mode: SpaceMode,
    pub points: Vec<DVec2>,
    /// The storey the input started on.
    pub storey: Option<StoreyId>,
    pub measure: MeasureTool,
    /// Contour detection of the wall, slab and space tools (INP-018).
    pub detection: ContourDetection,
    /// The rooms the space tool detects (SPC-001).
    pub space: SpaceDetection,
    /// The open inline text editor (ANN-007): opened by the text tool's
    /// click or a double-click on a text, so also with the selection tool.
    /// Changing tools commits it, nothing else in the tool state touches it.
    pub editor: Option<TextEditor>,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            kind: ToolKind::Select,
            wall_mode: WallMode::Chain,
            slab_mode: SlabMode::Polygon,
            space_mode: SpaceMode::Detect,
            points: Vec::new(),
            storey: None,
            measure: MeasureTool::default(),
            detection: ContourDetection::default(),
            space: SpaceDetection::default(),
            editor: None,
        }
    }
}

fn rectangle(a: DVec2, c: DVec2) -> Vec<DVec2> {
    let corners = vec![a, DVec2::new(c.x, a.y), c, DVec2::new(a.x, c.y)];
    counter_clockwise(corners)
}

/// A rectangle on the base edge `a`–`b`, extended towards `c`.
fn rotated_rectangle(a: DVec2, b: DVec2, c: DVec2) -> Vec<DVec2> {
    let dir = (b - a).normalize_or_zero();
    let normal = dir.perp();
    let width = (c - a).dot(normal);
    counter_clockwise(vec![a, b, b + normal * width, a + normal * width])
}

/// The walls along `points`, closed back to the first point if `closed`;
/// consecutive walls share their ends (WAL-001).
fn walls_along(project: &Project, storey: StoreyId, points: &[DVec2], closed: bool) -> Vec<Element> {
    let n = points.len();
    let ends = if closed { n } else { n.saturating_sub(1) };
    (0..ends)
        .map(|i| project.new_wall(storey, points[i], points[(i + 1) % n], i as u32))
        .collect()
}

/// The outline of every wall along `line`, closed back to the first point
/// if `closed`, from the tool defaults' thickness and reference-line
/// location (WAL-007).
fn wall_faces(project: &Project, line: &[DVec2], closed: bool) -> Vec<(Vec<DVec2>, bool)> {
    let (exterior, interior) = project
        .tool_defaults
        .get(KindTag::Wall)
        .and_then(|slot| match &slot.kind {
            ElementKind::Wall(w) => Some(w),
            _ => None,
        })
        .map_or((0.0, 0.0), |w| w.reference_line.face_offsets(w.thickness));
    let n = line.len();
    let ends = if closed { n } else { n.saturating_sub(1) };
    (0..ends)
        .filter_map(|i| {
            let (a, b) = (line[i], line[(i + 1) % n]);
            let normal = (b - a).try_normalize()?.perp();
            Some((
                vec![
                    a + normal * exterior,
                    b + normal * exterior,
                    b + normal * interior,
                    a + normal * interior,
                ],
                true,
            ))
        })
        .collect()
}

/// What the wall or slab tool traces from a picked contour (INP-018): the
/// preview draws it and the click creates its elements, so the two agree.
#[derive(Clone, Debug, PartialEq)]
enum Trace {
    /// A slab on the outer ring with the holes (SLB-001, SLB-003).
    Slab { outer: Vec<DVec2>, holes: Vec<Vec<DVec2>> },
    /// Joined walls along the line, closed back to its first point if
    /// `closed` (WAL-001).
    Walls { line: Vec<DVec2>, closed: bool },
    /// A space on the room around `seed`, the outer ring with the holes
    /// (SPC-001), detected and kept up to date with its walls.
    Space {
        outer: Vec<DVec2>,
        holes: Vec<Vec<DVec2>>,
        seed: DVec2,
    },
}

impl Trace {
    /// The elements on `storey`. The walls' reference lines run on the
    /// line, counter-clockwise around a closed contour.
    fn elements(self, project: &Project, storey: StoreyId) -> Vec<Element> {
        match self {
            Trace::Slab { outer, holes } => vec![project.new_slab(storey, outer, holes, 0)],
            Trace::Walls { line, closed } => walls_along(project, storey, &line, closed),
            Trace::Space { outer, holes, seed } => vec![space::new_space(
                project,
                storey,
                outer,
                holes,
                BoundarySource::Detected {
                    seed,
                    auto_update: true,
                },
            )],
        }
    }

    /// The preview: a slab's or a space's rings, or the walls' reference
    /// line with the outline of every wall, as the wall chain previews them.
    fn outlines(&self, project: &Project) -> Vec<(Vec<DVec2>, bool)> {
        match self {
            Trace::Slab { outer, holes } | Trace::Space { outer, holes, .. } => std::iter::once(outer)
                .chain(holes)
                .map(|ring| (ring.clone(), true))
                .collect(),
            Trace::Walls { line, closed } => std::iter::once((line.clone(), *closed))
                .chain(wall_faces(project, line, *closed))
                .collect(),
        }
    }
}

fn counter_clockwise(mut ring: Vec<DVec2>) -> Vec<DVec2> {
    let area: f64 = (0..ring.len())
        .map(|i| ring[i].perp_dot(ring[(i + 1) % ring.len()]))
        .sum();
    if area < 0.0 {
        ring.reverse();
    }
    ring
}

impl ToolState {
    pub fn select(&mut self, kind: ToolKind) {
        self.kind = kind;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.storey = None;
        self.measure.clear();
    }

    pub fn is_drawing(&self) -> bool {
        !self.points.is_empty() || self.measure.in_progress()
    }

    /// The storey the input in progress started on, also of a measurement.
    pub fn input_storey(&self) -> Option<StoreyId> {
        match self.kind {
            ToolKind::Measure => self.measure.storey,
            _ => self.storey,
        }
    }

    /// How many points complete the current operation, if fixed.
    fn needed(&self) -> Option<usize> {
        match self.kind {
            ToolKind::Select | ToolKind::Text | ToolKind::Measure => None,
            ToolKind::Wall => match self.wall_mode {
                WallMode::Single | WallMode::Rectangle => Some(2),
                WallMode::RotatedRectangle => Some(3),
                WallMode::Chain => None,
            },
            ToolKind::Slab | ToolKind::Space => match self.outline_mode()? {
                SlabMode::Rectangle => Some(2),
                SlabMode::RotatedRectangle => Some(3),
                SlabMode::Polygon => None,
            },
        }
    }

    /// The slab tool's method an outline is drawn with (SLB-001): the slab
    /// tool's, or the space tool's polygon or rectangle (SPC-001). `None`
    /// for the other tools and for detecting spaces.
    fn outline_mode(&self) -> Option<SlabMode> {
        match self.kind {
            ToolKind::Slab => Some(self.slab_mode),
            ToolKind::Space => self.space_mode.outline(),
            _ => None,
        }
    }

    /// The element on a drawn outline: a slab, or a space with a drawn
    /// boundary (SLB-001, SPC-001).
    fn outline_element(&self, project: &Project, storey: StoreyId, outer: Vec<DVec2>) -> Element {
        match self.kind {
            ToolKind::Space => space::new_space(project, storey, outer, Vec::new(), BoundarySource::Drawn),
            _ => project.new_slab(storey, outer, Vec::new(), 0),
        }
    }

    /// Whether a click picks a contour (INP-018) and no points are pending:
    /// detection is on with the wall, slab or space tool, or the space tool
    /// detects rooms, its default method (SPC-001).
    pub fn detecting(&self) -> bool {
        let on = match self.kind {
            ToolKind::Wall | ToolKind::Slab => self.detection.is_on(),
            ToolKind::Space => self.space_mode == SpaceMode::Detect || self.detection.is_on(),
            ToolKind::Select | ToolKind::Text | ToolKind::Measure => false,
        };
        on && self.points.is_empty()
    }

    /// The contours the tool creates its elements from (INP-018): walls run
    /// along any, a slab needs a closed outline. The preview and the click
    /// both pick with it, so they agree.
    fn accepts(&self) -> Accepts {
        match self.kind {
            ToolKind::Slab => Accepts::Closed,
            _ => Accepts::All,
        }
    }

    /// What the tool traces from a contour it [accepts] (INP-018,
    /// SLB-001): the slab tool a slab on a region, its islands as holes, or
    /// on a closed chain; the wall tool joined walls along a chain, closed
    /// around a closed one, or around a region's boundary without its
    /// islands.
    ///
    /// [accepts]: ToolState::accepts
    fn trace(&self, contour: Contour) -> Option<Trace> {
        match (self.kind, contour) {
            (ToolKind::Slab, Contour::Region(region)) => Some(Trace::Slab {
                outer: region.outer,
                holes: region.islands,
            }),
            (ToolKind::Slab, Contour::Chain(chain)) if chain.closed => Some(Trace::Slab {
                outer: chain.points,
                holes: Vec::new(),
            }),
            (ToolKind::Wall, Contour::Region(region)) => Some(Trace::Walls {
                line: region.outer,
                closed: true,
            }),
            (ToolKind::Wall, Contour::Chain(chain)) => Some(Trace::Walls {
                line: chain.points,
                closed: chain.closed,
            }),
            _ => None,
        }
    }

    /// The space on the room around `p` on `storey` that the space tool
    /// creates (SPC-001, INP-018): the room's outer ring with the walls
    /// standing free in it as holes. `None` where `p` lies outside every
    /// room or in a wall, or without regions of `storey`. The preview and
    /// the click both take it, so they agree.
    fn room(&self, storey: StoreyId, p: DVec2) -> Option<Trace> {
        let region = self.space.pick(storey, p)?.ok()?;
        Some(Trace::Space {
            outer: region.outer,
            holes: region.islands,
            seed: p,
        })
    }

    /// The text tool's step (ANN-007, NFR-USE-003): a point for the anchor,
    /// typed or clicked, then the content in the open editor.
    fn text_step(&self) -> StepSpec {
        if self.editor.is_some() {
            let hints = vec![
                KeyHint::new("Enter", tr!("prompt-text-key-newline")),
                KeyHint::new("Ctrl/Cmd+Enter", tr!("prompt-text-key-commit")),
                KeyHint::new("Esc", tr!("prompt-text-key-cancel")),
            ];
            return StepSpec {
                hints,
                ..StepSpec::new(tr!("prompt-text-type"), Expect::Point)
            };
        }
        let hints = vec![
            KeyHint::new("0–9", "types the coordinates"),
            KeyHint::new("P", "switches polar and Cartesian"),
        ];
        StepSpec {
            hints,
            ..StepSpec::new(tr!("prompt-text-anchor"), Expect::Point)
        }
    }

    fn is_chain(&self) -> bool {
        self.kind == ToolKind::Wall && self.wall_mode == WallMode::Chain
    }

    /// Ends an open wall chain of two or more points: the command creating
    /// all its walls as one undo step (UND-003). A chain with fewer points
    /// ends without walls.
    pub fn end_chain(&mut self, project: &Project) -> Option<Command> {
        if !self.is_chain() {
            return None;
        }
        let command = self.chain_walls(project, false).map(Command::AddElements);
        self.reset();
        command
    }

    /// The walls along the pending chain points, closed back to the first
    /// point if `closed`; consecutive walls share their ends (WAL-001).
    fn chain_walls(&self, project: &Project, closed: bool) -> Option<Vec<Element>> {
        let storey = self.storey?;
        (self.points.len() >= 2).then(|| walls_along(project, storey, &self.points, closed))
    }

    /// Adds a point. Returns the command to run once an operation completes.
    ///
    /// Points within the model tolerance are one point (NFR-ACC-003): a
    /// click that coincides with the previous point is ignored, and one that
    /// coincides with the first point closes a wall chain at that point or a
    /// slab outline. Wall chain points stay pending until the chain ends, so
    /// Backspace and Esc remove them without touching the project (UND-003);
    /// a click on another storey ends the open chain and starts a new one.
    fn add_point(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        if matches!(self.kind, ToolKind::Select | ToolKind::Text | ToolKind::Measure)
            || (self.kind == ToolKind::Space && self.outline_mode().is_none())
        {
            return Outcome::Continue;
        }
        if self.storey.is_some_and(|s| s != storey) {
            let ended = self.end_chain(project);
            self.reset();
            if let Some(command) = ended {
                self.storey = Some(storey);
                self.points.push(point);
                return Outcome::Execute(command);
            }
        }
        self.storey = Some(storey);
        if self.points.last().is_some_and(|&p| coincident(p, point)) {
            return Outcome::Continue;
        }
        match self.kind {
            ToolKind::Wall if self.wall_mode == WallMode::Chain => {
                if self.points.len() >= 2 && coincident(self.points[0], point) {
                    let walls = self.chain_walls(project, true);
                    self.reset();
                    return walls.map_or(Outcome::Continue, |w| Outcome::Execute(Command::AddElements(w)));
                }
                self.points.push(point);
                Outcome::Continue
            }
            ToolKind::Slab | ToolKind::Space if self.outline_mode() == Some(SlabMode::Polygon) => {
                if self.points.len() >= 3 && coincident(self.points[0], point) {
                    return self.close(project);
                }
                self.points.push(point);
                Outcome::Continue
            }
            _ => {
                self.points.push(point);
                if Some(self.points.len()) == self.needed() {
                    let outcome = self.complete(project, storey);
                    self.reset();
                    outcome
                } else {
                    Outcome::Continue
                }
            }
        }
    }

    fn complete(&self, project: &Project, storey: StoreyId) -> Outcome {
        let p = &self.points;
        let walls_around = |ring: Vec<DVec2>| walls_along(project, storey, &ring, true);
        let elements = match (self.kind, self.wall_mode, self.outline_mode()) {
            (ToolKind::Wall, WallMode::Single, _) => vec![project.new_wall(storey, p[0], p[1], 0)],
            (ToolKind::Wall, WallMode::Rectangle, _) => walls_around(rectangle(p[0], p[1])),
            (ToolKind::Wall, WallMode::RotatedRectangle, _) => walls_around(rotated_rectangle(p[0], p[1], p[2])),
            (_, _, Some(SlabMode::Rectangle)) => vec![self.outline_element(project, storey, rectangle(p[0], p[1]))],
            (_, _, Some(SlabMode::RotatedRectangle)) => {
                vec![self.outline_element(project, storey, rotated_rectangle(p[0], p[1], p[2]))]
            }
            _ => return Outcome::Continue,
        };
        Outcome::Execute(Command::AddElements(elements))
    }

    /// Ends a chain or closes a polygon (Enter or double-click).
    fn close(&mut self, project: &Project) -> Outcome {
        if self.is_chain() {
            return self.end_chain(project).map_or(Outcome::Continue, Outcome::Execute);
        }
        let outcome = match self.storey {
            Some(storey) if self.outline_mode() == Some(SlabMode::Polygon) && self.points.len() >= 3 => {
                Outcome::Execute(Command::AddElements(vec![self.outline_element(
                    project,
                    storey,
                    counter_clockwise(self.points.clone()),
                )]))
            }
            _ => Outcome::Continue,
        };
        self.reset();
        outcome
    }

    /// Removes the last entered point (UND-003).
    fn pop_point(&mut self) -> bool {
        let popped = self.points.pop().is_some();
        if self.points.is_empty() {
            self.storey = None;
        }
        popped
    }

    /// Outlines previewing the operation for a cursor position: walls as
    /// their reference lines, rectangles and polygons as closed rings. A
    /// wall chain also shows the outline of every wall it will create, from
    /// the tool defaults' thickness and reference-line location (WAL-007).
    pub fn outlines(&self, project: &Project, cursor: DVec2) -> Vec<(Vec<DVec2>, bool)> {
        let p = &self.points;
        if p.is_empty() {
            return Vec::new();
        }
        match (self.kind, self.wall_mode, self.outline_mode(), p.len()) {
            (ToolKind::Wall, WallMode::Single, _, _) => vec![(vec![p[p.len() - 1], cursor], false)],
            (ToolKind::Wall, WallMode::Chain, _, _) => {
                let mut line = p.clone();
                line.push(cursor);
                let faces = wall_faces(project, &line, false);
                std::iter::once((line, false)).chain(faces).collect()
            }
            (ToolKind::Wall, WallMode::Rectangle, _, _) | (_, _, Some(SlabMode::Rectangle), _) => {
                vec![(rectangle(p[0], cursor), true)]
            }
            (ToolKind::Wall, WallMode::RotatedRectangle, _, 1) | (_, _, Some(SlabMode::RotatedRectangle), 1) => {
                vec![(vec![p[0], cursor], false)]
            }
            (ToolKind::Wall, WallMode::RotatedRectangle, _, _) | (_, _, Some(SlabMode::RotatedRectangle), _) => {
                vec![(rotated_rectangle(p[0], p[1], cursor), true)]
            }
            (_, _, Some(SlabMode::Polygon), _) => {
                let mut ring = p.clone();
                ring.push(cursor);
                vec![(ring, true)]
            }
            _ => Vec::new(),
        }
    }
}

/// The tools as point-input consumers (INP-001): each step names the
/// expected input and its keys (NFR-USE-003).
impl InputConsumer for ToolState {
    fn step(&self) -> Option<StepSpec> {
        let n = self.points.len();
        if self.detecting() {
            if self.kind == ToolKind::Space {
                return Some(self.space.step(self.detection.is_on()));
            }
            let prompt = match self.kind {
                ToolKind::Slab => tr!("prompt-contour-slab"),
                _ => tr!("prompt-contour-wall"),
            };
            return Some(self.detection.step(prompt, self.accepts()));
        }
        let owned = |(prompt, repeat): (&str, bool)| (prompt.to_owned(), repeat);
        let (prompt, repeat): (String, bool) = match self.kind {
            ToolKind::Select => return None,
            ToolKind::Measure => return self.measure.step(),
            ToolKind::Text => return Some(self.text_step()),
            ToolKind::Space => match (self.space_mode, n) {
                (SpaceMode::Rectangle, 0) => (tr!("prompt-space-rectangle-first"), false),
                (SpaceMode::Rectangle, _) => (tr!("prompt-space-rectangle-next"), false),
                (_, 0) => (tr!("prompt-space-polygon-first"), false),
                (_, _) => (tr!("prompt-space-polygon-next"), true),
            },
            ToolKind::Wall => owned(match (self.wall_mode, n) {
                (WallMode::Rectangle, 0) => ("Wall rectangle: click the first corner", false),
                (WallMode::Rectangle, _) => ("Wall rectangle: click the opposite corner or type a distance", false),
                (WallMode::RotatedRectangle, 0) => ("Rotated wall rectangle: click the start of the base edge", false),
                (WallMode::RotatedRectangle, 1) => (
                    "Rotated wall rectangle: click the end of the base edge or type a distance",
                    false,
                ),
                (WallMode::RotatedRectangle, _) => ("Rotated wall rectangle: click to set the width", false),
                (_, 0) => ("Wall: click the start point of the reference line", false),
                (WallMode::Chain, _) => ("Wall chain: click the next point or type a distance", true),
                (_, _) => ("Wall: click the end point or type a distance", false),
            }),
            ToolKind::Slab => owned(match (self.slab_mode, n) {
                (SlabMode::Polygon, 0) => ("Slab: click the first corner of the outline", false),
                (SlabMode::Polygon, _) => ("Slab: click the next corner or type a distance", true),
                (SlabMode::Rectangle, 0) => ("Slab rectangle: click the first corner", false),
                (SlabMode::Rectangle, _) => ("Slab rectangle: click the opposite corner or type a distance", false),
                (SlabMode::RotatedRectangle, 0) => ("Rotated slab rectangle: click the start of the base edge", false),
                (SlabMode::RotatedRectangle, 1) => (
                    "Rotated slab rectangle: click the end of the base edge or type a distance",
                    false,
                ),
                (SlabMode::RotatedRectangle, _) => ("Rotated slab rectangle: click to set the width", false),
            }),
        };
        let mut hints = Vec::new();
        let expects = match self.points.last() {
            None => {
                if self.kind == ToolKind::Wall {
                    hints.push(KeyHint::new("R", "cycles the reference line"));
                }
                // INP-003, INP-004: the first point can be typed as X and Y.
                hints.push(KeyHint::new("0–9", "types the coordinates"));
                hints.push(KeyHint::new("P", "switches polar and Cartesian"));
                // INP-018: the provisional key held for contour detection.
                hints.push(KeyHint::new("Space", tr!("prompt-contour-key-hold")));
                Expect::Point
            }
            Some(&from) => {
                hints.push(KeyHint::new("Tab", "moves to the next field"));
                hints.push(KeyHint::new("P", "switches polar and Cartesian"));
                hints.push(KeyHint::new("Shift+P", "switches relative and absolute"));
                hints.push(KeyHint::new("Shift", "constrains angles"));
                match (self.kind, self.wall_mode, self.outline_mode()) {
                    (ToolKind::Wall, WallMode::Chain, _) => {
                        hints.push(KeyHint::new("Enter", "or double-click ends the chain"));
                    }
                    (_, _, Some(SlabMode::Polygon)) => {
                        hints.push(KeyHint::new(
                            "Enter",
                            "or double-click closes the outline, as does a click on the first corner",
                        ));
                    }
                    _ => {}
                }
                hints.push(KeyHint::new("Backspace", "removes the last point"));
                hints.push(KeyHint::new("Esc", "cancels the step"));
                Expect::Distance { from }
            }
        };
        Some(StepSpec {
            hints,
            repeat,
            ..StepSpec::new(prompt, expects)
        })
    }

    fn reference_point(&self) -> Option<DVec2> {
        match self.kind {
            ToolKind::Measure => self.measure.reference_point(),
            _ => self.points.last().copied(),
        }
    }

    fn preview(&self, project: &Project, resolved: &Resolved) -> Preview {
        match self.kind {
            ToolKind::Measure => self.measure.preview(project, resolved),
            ToolKind::Space if self.detecting() => Preview {
                outlines: self
                    .space
                    .storey()
                    .and_then(|storey| self.room(storey, resolved.point))
                    .map_or_else(Vec::new, |trace| trace.outlines(project)),
                ..Preview::default()
            },
            _ if self.detecting() => Preview {
                outlines: self
                    .detection
                    .hovered(resolved.point, self.accepts())
                    .and_then(|contour| self.trace(contour))
                    .map_or_else(Vec::new, |trace| trace.outlines(project)),
                ..Preview::default()
            },
            _ => Preview {
                outlines: self.outlines(project, resolved.point),
                ..Preview::default()
            },
        }
    }

    fn commit(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        match self.kind {
            ToolKind::Measure => self.measure.commit(project, storey, point),
            // ANN-007: the anchor; the editor's commit creates the text.
            ToolKind::Text => {
                if self.editor.is_none() {
                    self.editor = Some(TextEditor::new(project, storey, point));
                }
                Outcome::Continue
            }
            ToolKind::Space if self.detecting() => self.room(storey, point).map_or(Outcome::Continue, |trace| {
                Outcome::Execute(Command::AddElements(trace.elements(project, storey)))
            }),
            _ if self.detecting() => self
                .detection
                .pick(storey, point, self.accepts())
                .and_then(|contour| self.trace(contour))
                .map_or(Outcome::Continue, |trace| {
                    Outcome::Execute(Command::AddElements(trace.elements(project, storey)))
                }),
            _ => self.add_point(project, storey, point),
        }
    }

    /// Every operation, a wall chain too, returns to its previous point
    /// (INP-003); Esc clears a measurement (MEA-001) and turns contour
    /// detection off (INP-018).
    fn cancel_step(&mut self) -> bool {
        match self.kind {
            ToolKind::Measure => self.measure.cancel_step(),
            _ if self.detecting() => self.detection.cancel(),
            _ => self.pop_point(),
        }
    }

    fn backspace(&mut self) -> bool {
        match self.kind {
            ToolKind::Measure => self.measure.backspace(),
            _ => self.pop_point(),
        }
    }

    fn finish(&mut self, project: &Project) -> Outcome {
        match self.kind {
            ToolKind::Measure => self.measure.finish(project),
            _ => self.close(project),
        }
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{ElementKind, History, MODEL_TOLERANCE, samples};

    use super::*;
    use crate::document::Document;

    fn run(project: &mut Project, outcome: Outcome) {
        if let Outcome::Execute(command) = outcome {
            History::default().execute(project, command).unwrap();
        }
    }

    #[test]
    fn wall_chain_closes_into_joined_walls() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        for p in [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0), (0.0, 4.0), (0.0, 0.0)] {
            let outcome = tool.commit(&project, storey, DVec2::new(p.0, p.1));
            run(&mut project, outcome);
        }
        assert_eq!(project.elements.len(), 4);
        assert!(!tool.is_drawing(), "clicking the first point closes the chain");
        let names: Vec<&str> = project.elements.values().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["W-001", "W-002", "W-003", "W-004"]);
    }

    /// UND-003, WAL-001: chain points stay pending until the chain ends.
    /// Backspace removes the last of four points without touching the
    /// project; Enter then creates the two joined walls as one undo step,
    /// and one undo removes both.
    #[test]
    fn a_wall_chain_is_one_undo_step_and_backspace_creates_no_wall() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        for p in [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0), (0.0, 4.0)] {
            let outcome = tool.commit(&project, storey, DVec2::new(p.0, p.1));
            assert_eq!(outcome, Outcome::Continue, "a chain point executes nothing");
        }
        assert!(tool.backspace());
        assert_eq!(tool.points.len(), 3);
        assert!(project.elements.is_empty(), "no wall exists before the chain ends");

        let Outcome::Execute(command) = tool.finish(&project) else {
            panic!("Enter ends the chain")
        };
        let mut history = History::default();
        history.execute(&mut project, command).unwrap();
        assert!(!tool.is_drawing());
        let walls: Vec<(DVec2, DVec2)> = project
            .elements
            .values()
            .map(|e| e.as_wall().map(|w| (w.start, w.end)).unwrap())
            .collect();
        assert_eq!(
            walls,
            [
                (DVec2::ZERO, DVec2::new(5.0, 0.0)),
                (DVec2::new(5.0, 0.0), DVec2::new(5.0, 4.0))
            ],
            "consecutive walls share their end"
        );
        assert_eq!(history.undo_len(), 1, "the chain is one undo step");
        history.undo(&mut project);
        assert!(project.elements.is_empty(), "one undo removes both walls");
    }

    /// INP-003: Esc during a chain cancels one step, the last point, and
    /// creates no wall; with one point left it removes that one too.
    #[test]
    fn esc_removes_the_last_chain_point() {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        for p in [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0)] {
            tool.commit(&project, storey, DVec2::new(p.0, p.1));
        }
        assert!(tool.cancel_step());
        assert_eq!(tool.points, [DVec2::ZERO, DVec2::new(5.0, 0.0)]);
        assert!(tool.cancel_step());
        assert!(tool.cancel_step());
        assert!(!tool.is_drawing());
        assert!(!tool.cancel_step(), "nothing left to cancel");
    }

    /// UND-003: a click on another storey ends the open chain there, keeping
    /// its walls, and starts a new chain at the click.
    #[test]
    fn a_click_on_another_storey_commits_the_open_chain() {
        let project = default_metric_project();
        let ground = project.storeys.ground();
        let other = StoreyId::new();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        for p in [(0.0, 0.0), (5.0, 0.0), (5.0, 4.0)] {
            tool.commit(&project, ground, DVec2::new(p.0, p.1));
        }
        let Outcome::Execute(Command::AddElements(walls)) = tool.commit(&project, other, DVec2::new(9.0, 9.0)) else {
            panic!("the open chain is committed")
        };
        assert_eq!(walls.len(), 2);
        assert!(walls.iter().all(|w| w.storey == ground));
        assert_eq!(tool.points, [DVec2::new(9.0, 9.0)]);
        assert_eq!(tool.storey, Some(other));
    }

    /// UND-003, WAL-007: the chain preview draws the pending reference-line
    /// polyline to the cursor and the outline of every wall it will create,
    /// from the tool defaults' thickness and reference-line location.
    #[test]
    fn wall_chain_previews_the_pending_walls() {
        let mut project = default_metric_project();
        let Some(ElementKind::Wall(wall)) = project.tool_defaults.get_mut(KindTag::Wall).map(|s| &mut s.kind) else {
            panic!("the template has a wall slot");
        };
        wall.thickness = 0.2;
        wall.reference_line = archicraft_model::ReferenceLine::Outside;
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        for p in [(0.0, 0.0), (4.0, 0.0)] {
            tool.commit(&project, storey, DVec2::new(p.0, p.1));
        }
        let cursor = DVec2::new(4.0, 3.0);
        let outlines = tool.outlines(&project, cursor);
        assert_eq!(
            outlines[0],
            (vec![DVec2::ZERO, DVec2::new(4.0, 0.0), cursor], false),
            "the reference line"
        );
        assert_eq!(outlines.len(), 3, "the line and two wall outlines");
        // Exterior face on the reference line, the interior face 0.2 to the
        // left of it.
        let (ring, closed) = &outlines[1];
        assert!(closed);
        let expected = [(0.0, 0.0), (4.0, 0.0), (4.0, 0.2), (0.0, 0.2)];
        for (q, e) in ring.iter().zip(expected) {
            assert!(q.distance(DVec2::new(e.0, e.1)) < 1e-12, "{ring:?}");
        }
    }

    const MICROMETRE: f64 = 1e-6;

    /// NFR-ACC-003: a last click 5 µm from the first point closes the chain
    /// into a ring that ends exactly at the first point.
    #[test]
    fn wall_chain_closes_within_the_tolerance() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        let near_start = DVec2::new(3.0, 4.0) * MICROMETRE;
        for p in [
            DVec2::ZERO,
            DVec2::new(5.0, 0.0),
            DVec2::new(5.0, 4.0),
            DVec2::new(0.0, 4.0),
            near_start,
        ] {
            let outcome = tool.add_point(&project, storey, p);
            run(&mut project, outcome);
        }
        assert_eq!(project.elements.len(), 4);
        assert!(!tool.is_drawing(), "5 µm from the first point closes the chain");
        let last = project.elements.values().last().unwrap().as_wall().unwrap();
        assert_eq!(last.end, DVec2::ZERO, "the ring closes at the first point");
    }

    /// NFR-ACC-003: a click within the tolerance of the previous point is
    /// the same point; one 20 µm away is a new one.
    #[test]
    fn repeated_clicks_within_the_tolerance_are_one_point() {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Slab);
        tool.add_point(&project, storey, DVec2::ZERO);
        tool.add_point(&project, storey, DVec2::new(5.0 * MICROMETRE, 0.0));
        assert_eq!(tool.points, [DVec2::ZERO]);
        tool.add_point(&project, storey, DVec2::new(20.0 * MICROMETRE, 0.0));
        assert_eq!(tool.points.len(), 2);
    }

    /// NFR-ACC-003: a click within the tolerance of the first corner closes
    /// the slab outline.
    #[test]
    fn slab_polygon_closes_within_the_tolerance() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Slab);
        for p in [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)] {
            tool.add_point(&project, storey, DVec2::new(p.0, p.1));
        }
        let outcome = tool.add_point(&project, storey, DVec2::new(0.0, 5.0 * MICROMETRE));
        run(&mut project, outcome);
        assert!(!tool.is_drawing());
        let slab = project.elements.values().find_map(|e| e.as_slab()).unwrap();
        assert_eq!(slab.outline.outer().len(), 3);
    }

    #[test]
    fn rectangle_tools() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        tool.wall_mode = WallMode::Rectangle;
        assert!(matches!(
            tool.commit(&project, storey, DVec2::new(4.0, 3.0)),
            Outcome::Continue
        ));
        let outcome = tool.commit(&project, storey, DVec2::ZERO);
        run(&mut project, outcome);
        assert_eq!(project.elements.len(), 4);
        // Counter-clockwise: exterior faces outside.
        let first = project.elements.values().next().unwrap().as_wall().unwrap().clone();
        assert_eq!((first.start, first.end), (DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0)));

        tool.select(ToolKind::Slab);
        tool.slab_mode = SlabMode::RotatedRectangle;
        for p in [(0.0, 0.0), (3.0, 3.0)] {
            tool.commit(&project, storey, DVec2::new(p.0, p.1));
        }
        let outcome = tool.commit(&project, storey, DVec2::new(-1.0, 1.0));
        run(&mut project, outcome);
        let slab = project.elements.values().find_map(|e| e.as_slab()).unwrap();
        let area: f64 = (0..4)
            .map(|i| slab.outline.outer().node(i).perp_dot(slab.outline.outer().node(i + 1)))
            .sum::<f64>()
            / 2.0;
        assert!((area - 3.0 * 2f64.sqrt() * 2f64.sqrt()).abs() < 1e-9, "{area}");
    }

    #[test]
    fn slab_polygon_finishes_on_enter() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Slab);
        for p in [(0.0, 0.0), (0.0, 4.0), (6.0, 4.0)] {
            tool.commit(&project, storey, DVec2::new(p.0, p.1));
        }
        tool.backspace();
        tool.commit(&project, storey, DVec2::new(6.0, 0.0));
        let outcome = tool.finish(&project);
        run(&mut project, outcome);
        let slab = project.elements.values().next().unwrap();
        let ElementKind::Slab(s) = &slab.kind else { panic!() };
        assert_eq!(s.outline.outer().len(), 3);
    }

    /// The tool on the storey at `index` of two_storey_shell_with_shaft with
    /// contour detection on, picking from the cached graph of its plan with
    /// a 0.15 m radius (INP-018).
    fn detecting_on_the_shaft_sample(kind: ToolKind, index: usize) -> (Document, StoreyId, ToolState) {
        let mut doc = Document::new(samples::two_storey_shell_with_shaft(), 100);
        let storey = doc.project.storeys.as_slice()[index].id;
        let mut tool = ToolState::default();
        tool.select(kind);
        tool.detection.toggle();
        assert!(tool.detecting());
        let graph = doc.contour_graph(storey);
        tool.detection.set_source(storey, graph, 0.15);
        (doc, storey, tool)
    }

    /// The preview outlines of the tool with the cursor at `p`.
    fn preview_at(tool: &ToolState, project: &Project, p: DVec2) -> Vec<(Vec<DVec2>, bool)> {
        let resolved = Resolved {
            point: p,
            snap: crate::input::Snap {
                point: p,
                kind: crate::input::SnapKind::Free,
            },
            candidates: Vec::new(),
            guides: Vec::new(),
            hovered: None,
        };
        tool.preview(project, &resolved).outlines
    }

    /// INP-018, SLB-001: on the ground storey of the shell with a shaft, the
    /// slab tool with detection on previews the region around (2, 3) and a
    /// click there creates a slab with that ring as its outline, 97.08875
    /// m² inside it, as one undo step; the tool stays in detection.
    #[test]
    fn a_click_in_a_region_creates_a_slab_of_its_outline() {
        let (mut doc, storey, mut tool) = detecting_on_the_shaft_sample(ToolKind::Slab, 0);
        let p = DVec2::new(2.0, 3.0);
        let preview = preview_at(&tool, &doc.project, p);
        assert_eq!(preview.len(), 1, "the ring alone: {preview:?}");
        let (ring, closed) = &preview[0];
        assert!(closed);
        let step = tool.step().unwrap();
        assert_eq!(step.expects, Expect::Pick);
        assert!(step.prompt.starts_with("Slab from a contour"), "{}", step.prompt);

        let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
            panic!("the click creates the slab")
        };
        doc.execute(command).unwrap();
        assert_eq!(doc.history.undo_len(), 1);
        let slab = doc.project.elements.values().last().unwrap().as_slab().unwrap();
        let (outer, holes) = slab.outline.to_points();
        assert_eq!(&outer, ring, "the slab has the previewed ring");
        let area = slab.outline.outer().signed_area();
        assert!((area - 97.08875).abs() < 1e-6, "{outer:?}");
        assert!(holes.is_empty());
        assert!(tool.detecting(), "detection stays on for the next pick");
    }

    /// INP-018, SLB-001, SLB-003: on the upper storey the region around
    /// (2, 3) has two islands, the stair hole and the shaft walls; the slab
    /// created from it has them as its two holes and passes validation.
    #[test]
    fn the_islands_of_a_picked_region_become_holes_of_the_slab() {
        let (mut doc, storey, mut tool) = detecting_on_the_shaft_sample(ToolKind::Slab, 1);
        let p = DVec2::new(2.0, 3.0);
        assert_eq!(preview_at(&tool, &doc.project, p).len(), 3, "the ring and two islands");
        let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
            panic!("the click creates the slab")
        };
        doc.execute(command).expect("the slab with holes is valid");
        let slab = doc.project.elements.values().last().unwrap().as_slab().unwrap();
        assert_eq!(slab.outline.holes().len(), 2);
        let area = 11.5 * 8.5 - 0.115 * 5.75 - 3.0 * 1.2 - 2.115 * 1.615;
        // Signed: the outer ring counter-clockwise, the holes clockwise.
        let net: f64 = slab.outline.indexed_rings().map(|(_, ring)| ring.signed_area()).sum();
        assert!((net - area).abs() < 1e-6, "{net}");
    }

    /// INP-018, WAL-001: on the upper storey, the wall tool with detection on
    /// turns a click on the projected stair-hole outline into four walls
    /// around the hole, each starting where the one before ends, as one
    /// undo step that one undo removes.
    #[test]
    fn a_click_on_the_stair_hole_outline_creates_four_joined_walls() {
        let (mut doc, storey, mut tool) = detecting_on_the_shaft_sample(ToolKind::Wall, 1);
        let p = DVec2::new(7.5, 1.02);
        let preview = preview_at(&tool, &doc.project, p);
        let hole = [(6.0, 1.0), (9.0, 1.0), (9.0, 2.2), (6.0, 2.2)].map(|(x, y)| DVec2::new(x, y));
        assert_eq!(preview.len(), 5, "the chain and the outlines of its four walls");
        assert!(preview[0].1, "the chain is closed");
        assert_eq!(preview[0].0.len(), 4);

        let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
            panic!("the click creates the walls")
        };
        doc.execute(command).unwrap();
        let created = doc.project.elements.values().skip(doc.project.elements.len() - 4);
        let walls: Vec<(DVec2, DVec2)> = created
            .map(|e| e.as_wall().map(|w| (w.start, w.end)).unwrap())
            .collect();
        for (i, (start, end)) in walls.iter().enumerate() {
            assert_eq!(*end, walls[(i + 1) % 4].0, "wall {i} ends where the next starts");
            assert_eq!(*start, preview[0].0[i]);
            assert!(
                hole.iter().any(|q| q.distance(*start) < MODEL_TOLERANCE),
                "{start} is a hole corner"
            );
        }
        assert_eq!(doc.history.undo_len(), 1, "one undo step");
        let before = doc.project.elements.len();
        doc.undo();
        assert_eq!(
            doc.project.elements.len(),
            before - 4,
            "one undo removes the four walls"
        );
    }

    /// INP-018: with Alt held, the wall tool previews and creates exactly
    /// one wall along the clicked edge, the room's south face up to the T
    /// wall.
    #[test]
    fn an_alt_click_creates_one_wall_along_the_clicked_edge() {
        let (mut doc, storey, mut tool) = detecting_on_the_shaft_sample(ToolKind::Wall, 0);
        tool.detection.edge_only = true;
        let p = DVec2::new(2.0, 0.25);
        let edge = vec![DVec2::new(0.25, 0.25), DVec2::new(4.9425, 0.25)];
        let preview = preview_at(&tool, &doc.project, p);
        assert_eq!(preview.len(), 2, "the edge and its wall's outline");
        assert!(!preview[0].1);
        for (q, e) in preview[0].0.iter().zip(&edge) {
            assert!(q.distance(*e) < MODEL_TOLERANCE, "{preview:?}");
        }
        assert_eq!(preview[0].0.len(), 2, "the edge only");

        let before = doc.project.elements.len();
        let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
            panic!("the click creates the wall")
        };
        doc.execute(command).unwrap();
        assert_eq!(doc.project.elements.len(), before + 1, "exactly one wall");
        let wall = doc.project.elements.values().last().unwrap().as_wall().unwrap();
        assert_eq!([wall.start, wall.end].to_vec(), preview[0].0);
    }

    /// The trace the elements a click created follow: a slab's rings, or
    /// the line of joined walls.
    fn trace_of(elements: &[Element]) -> Trace {
        if let [element] = elements
            && let Some(slab) = element.as_slab()
        {
            let (outer, holes) = slab.outline.to_points();
            return Trace::Slab { outer, holes };
        }
        let walls: Vec<_> = elements.iter().map(|e| e.as_wall().unwrap()).collect();
        let (first, last) = (walls[0], walls[walls.len() - 1]);
        let closed = walls.len() > 1 && last.end == first.start;
        let mut line: Vec<DVec2> = walls.iter().map(|w| w.start).collect();
        if !closed {
            line.push(last.end);
        }
        Trace::Walls { line, closed }
    }

    /// INP-018: for the wall and slab tools, with and without Alt, the
    /// preview shows exactly what a click at the same point creates. The
    /// slab tool needs a closed outline: at an antenna inside a square it
    /// previews and creates the square, at a free line outside it nothing,
    /// never the open chain or the clicked edge, and in a room with an
    /// island a slab with a hole. The wall tool takes the antenna and the
    /// line, and walls around the room without its island, previewed with
    /// their outlines and no island ring; with Alt it takes edges only, so
    /// nothing in the room. Only the wall tool's prompt offers Alt.
    #[test]
    fn the_preview_shows_what_the_click_creates() {
        use std::sync::Arc;

        use archicraft_geometry::contour::{ContourGraph, ContourOptions, Segment};

        let v = DVec2::new;
        let ring_segments = |ring: &[DVec2]| -> Vec<Segment> {
            (0..ring.len())
                .map(|i| Segment::new(ring[i], ring[(i + 1) % ring.len()]))
                .collect()
        };
        let square = [v(0.0, 0.0), v(4.0, 0.0), v(4.0, 4.0), v(0.0, 4.0)];
        let room = [v(10.0, 0.0), v(14.0, 0.0), v(14.0, 4.0), v(10.0, 4.0)];
        let island = [v(11.0, 1.0), v(12.0, 1.0), v(12.0, 2.0), v(11.0, 2.0)];
        let mut segments = ring_segments(&square);
        segments.extend(ring_segments(&room));
        segments.extend(ring_segments(&island));
        segments.push(Segment::new(v(2.0, 0.0), v(2.0, 2.0)));
        segments.push(Segment::new(v(6.0, 0.0), v(8.0, 0.0)));
        let graph = Arc::new(ContourGraph::build(segments.as_slice(), &ContourOptions::default()));
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let (antenna, line, outside, in_room) = (v(2.05, 1.5), v(7.0, 0.05), v(6.0, 6.0), v(13.0, 3.0));

        let mut traces = Vec::new();
        for kind in [ToolKind::Slab, ToolKind::Wall] {
            for alt in [false, true] {
                for p in [antenna, line, outside, in_room] {
                    let mut tool = ToolState::default();
                    tool.select(kind);
                    tool.detection.toggle();
                    tool.detection.set_source(storey, graph.clone(), 0.15);
                    tool.detection.edge_only = alt;
                    let preview = preview_at(&tool, &project, p);
                    let trace = match tool.commit(&project, storey, p) {
                        Outcome::Execute(Command::AddElements(elements)) => Some(trace_of(&elements)),
                        Outcome::Continue => None,
                        other => panic!("{other:?}"),
                    };
                    let created = trace.as_ref().map_or_else(Vec::new, |t| t.outlines(&project));
                    assert_eq!(preview, created, "{kind:?} alt {alt} at {p}");
                    if kind == ToolKind::Wall {
                        let island_ring = |ring: &Vec<DVec2>| island.iter().all(|q| ring.contains(q));
                        assert!(
                            !preview.iter().any(|(ring, _)| island_ring(ring)),
                            "{kind:?} alt {alt} at {p}: no island ring"
                        );
                    }
                    traces.push(trace);
                }
            }
        }
        // The rings counter-clockwise without the antenna's foot, the island
        // as a hole clockwise, as the graph gives them.
        let square_ring = vec![square[3], square[0], square[1], square[2]];
        let room_ring = vec![room[3], room[0], room[1], room[2]];
        let hole = vec![island[0], island[3], island[2], island[1]];
        let slab = [
            Some(Trace::Slab {
                outer: square_ring,
                holes: Vec::new(),
            }),
            None,
            None,
            Some(Trace::Slab {
                outer: room_ring.clone(),
                holes: vec![hole],
            }),
        ];
        let walls = |line: Vec<DVec2>, closed| Some(Trace::Walls { line, closed });
        let wall = [
            walls(vec![v(2.0, 0.0), v(2.0, 2.0)], false),
            walls(vec![v(6.0, 0.0), v(8.0, 0.0)], false),
            None,
            walls(room_ring, true),
        ];
        // With Alt the wall tool picks an edge only: none in the room.
        let mut wall_alt = wall.clone();
        wall_alt[3] = None;
        let expected: Vec<_> = [&slab, &slab, &wall, &wall_alt]
            .into_iter()
            .flatten()
            .cloned()
            .collect();
        assert_eq!(traces, expected);

        let hints = |kind| {
            let mut tool = ToolState::default();
            tool.select(kind);
            tool.detection.toggle();
            tool.step()
                .unwrap()
                .hints
                .into_iter()
                .map(|h| h.key)
                .collect::<Vec<_>>()
        };
        assert_eq!(hints(ToolKind::Slab), ["Esc"]);
        assert_eq!(hints(ToolKind::Wall), ["Alt", "Esc"]);
    }

    /// INP-018, INP-003: Esc turns detection off and returns to the tool's
    /// first point without touching the project; a click on another storey
    /// than the graph's picks nothing.
    #[test]
    fn esc_returns_from_detection_without_side_effects() {
        let (doc, storey, mut tool) = detecting_on_the_shaft_sample(ToolKind::Slab, 0);
        let other = doc.project.storeys.as_slice()[1].id;
        assert_eq!(
            tool.commit(&doc.project, other, DVec2::new(2.0, 3.0)),
            Outcome::Continue
        );
        let revision = doc.project.revision;
        assert!(tool.cancel_step());
        assert!(!tool.detection.is_on() && !tool.detecting());
        assert_eq!(tool.step().unwrap().expects, Expect::Point);
        assert!(!tool.is_drawing());
        assert_eq!(doc.project.revision, revision);
        assert!(!tool.cancel_step(), "nothing left to cancel");
        // A click now takes the first corner.
        assert_eq!(
            tool.commit(&doc.project, storey, DVec2::new(2.0, 3.0)),
            Outcome::Continue
        );
        assert_eq!(tool.points, [DVec2::new(2.0, 3.0)]);
    }

    /// The space tool on the storey at `index` of
    /// two_storey_shell_with_shaft in its default method, detecting rooms
    /// from the storey's cached regions with the cursor at `cursor`
    /// (SPC-001).
    fn space_tool_on_the_shaft_sample(index: usize, cursor: DVec2) -> (Document, StoreyId, ToolState) {
        let mut doc = Document::new(samples::two_storey_shell_with_shaft(), 100);
        let storey = doc.project.storeys.as_slice()[index].id;
        let mut tool = ToolState::default();
        tool.select(ToolKind::Space);
        assert_eq!(tool.space_mode, SpaceMode::Detect, "detection is the default method");
        assert!(tool.detecting());
        let regions = doc.space_regions(storey);
        tool.space.set_source(regions, cursor);
        (doc, storey, tool)
    }

    /// The space a command creates, the last element of the project.
    fn last_space(doc: &Document) -> (&Element, &archicraft_model::Space) {
        let element = doc.project.elements.values().last().unwrap();
        (element, element.as_space().expect("a space"))
    }

    /// SPC-001, INP-018, NFR-USE-003: in its default method the space tool
    /// previews the boundary space_region gives around a point in the
    /// ground-floor room, holes included, and a click there creates one
    /// space with that boundary, detected from the click and kept up to date
    /// with its walls, named "Room 1" without a number, as one undo step.
    /// The regions are built once per revision.
    #[test]
    fn a_click_inside_a_room_creates_a_detected_space_of_its_boundary() {
        let p = DVec2::new(2.0, 3.0);
        let (mut doc, storey, mut tool) = space_tool_on_the_shaft_sample(0, p);
        let step = tool.step().unwrap();
        assert_eq!(step.expects, Expect::Pick);
        assert_eq!(step.prompt, "Space: click inside a room bounded by walls");
        assert!(step.hints.is_empty(), "Esc returns to the selection: {:?}", step.hints);

        let expected = archicraft_geometry::space_region(&doc.project, storey, p).unwrap();
        let (outer, holes) = expected.to_points();
        let rings: Vec<(Vec<DVec2>, bool)> = std::iter::once(&outer)
            .chain(&holes)
            .map(|ring| (ring.clone(), true))
            .collect();
        assert_eq!(
            preview_at(&tool, &doc.project, p),
            rings,
            "the preview shows the boundary"
        );

        let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
            panic!("the click creates the space")
        };
        let before = doc.project.elements.len();
        doc.execute(command).unwrap();
        assert_eq!(doc.project.elements.len(), before + 1, "one space");
        assert_eq!(doc.history.undo_len(), 1, "one undo step");
        let (element, space) = last_space(&doc);
        assert_eq!(element.storey, storey);
        assert_eq!(element.name, "SP-001");
        assert_eq!(
            space.source,
            archicraft_model::BoundarySource::Detected {
                seed: p,
                auto_update: true
            }
        );
        assert_eq!(space.boundary.to_points(), (outer, holes), "the detected boundary");
        assert_eq!(space.long_name, "Room 1");
        assert!(space.number.is_empty());
        assert!(tool.detecting(), "the tool stays ready for the next room");

        assert_eq!(doc.space_region_builds(), 1);
        doc.space_regions(storey);
        assert_eq!(doc.space_region_builds(), 2, "the space changed the revision");
        doc.space_regions(storey);
        assert_eq!(doc.space_region_builds(), 2, "cached for the revision");
    }

    /// SPC-001, NFR-USE-003: a click outside the building or inside a wall
    /// creates nothing, previews nothing, and the prompt says why.
    #[test]
    fn a_click_outside_the_rooms_creates_nothing_and_the_prompt_says_why() {
        for (p, prompt) in [
            (DVec2::new(-3.0, -3.0), "Region is open"),
            (DVec2::new(0.1, 3.0), "The point lies in a wall"),
        ] {
            let (doc, storey, mut tool) = space_tool_on_the_shaft_sample(0, p);
            let step = tool.step().unwrap();
            assert!(step.prompt.starts_with(prompt), "{p}: {}", step.prompt);
            assert!(preview_at(&tool, &doc.project, p).is_empty(), "{p}");
            let revision = doc.project.revision;
            assert_eq!(tool.commit(&doc.project, storey, p), Outcome::Continue, "{p}");
            assert_eq!(doc.project.revision, revision);
        }
    }

    /// SPC-001, ELM-019: new spaces are named from the space counter, "Room
    /// 1" then "Room 2"; with a long name "Raum {n}" in the space defaults
    /// slot and one space in the project, the next is named "Raum 2". The
    /// English message takes the placeable $n.
    #[test]
    fn new_spaces_are_named_from_the_counter_and_the_defaults() {
        let p = DVec2::new(2.0, 3.0);
        let (mut doc, storey, mut tool) = space_tool_on_the_shaft_sample(0, p);
        let click = |doc: &mut Document, tool: &mut ToolState| {
            let regions = doc.space_regions(storey);
            tool.space.set_source(regions, p);
            let Outcome::Execute(command) = tool.commit(&doc.project, storey, p) else {
                panic!("the click creates a space")
            };
            doc.execute(command).unwrap();
            last_space(doc).1.long_name.clone()
        };
        assert_eq!(click(&mut doc, &mut tool), "Room 1");
        assert_eq!(click(&mut doc, &mut tool), "Room 2");

        let (mut doc, _, mut tool) = space_tool_on_the_shaft_sample(0, p);
        assert_eq!(click(&mut doc, &mut tool), "Room 1");
        let Some(ElementKind::Space(defaults)) = doc.project.tool_defaults.get_mut(KindTag::Space).map(|s| &mut s.kind)
        else {
            panic!("the template has a space slot")
        };
        defaults.long_name = "Raum {n}".to_owned();
        assert_eq!(click(&mut doc, &mut tool), "Raum 2");

        assert!(crate::i18n::Localizer::english().has("tool-space-default-name"));
        assert_eq!(tr!("tool-space-default-name", n = "7"), "Room 7");
    }

    /// SPC-001, SLB-001: the space tool's rectangle method creates a space
    /// on the drawn rectangle with a drawn boundary, as one undo step; Esc
    /// in its first step turns nothing off and leaves the tool.
    #[test]
    fn the_rectangle_method_creates_a_drawn_space() {
        let mut doc = Document::new(default_metric_project(), 100);
        let storey = doc.project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Space);
        tool.space_mode = SpaceMode::Rectangle;
        assert!(!tool.detecting());
        assert_eq!(tool.step().unwrap().expects, Expect::Point);
        assert_eq!(tool.commit(&doc.project, storey, DVec2::ZERO), Outcome::Continue);
        assert_eq!(
            preview_at(&tool, &doc.project, DVec2::new(4.0, 3.0)),
            [(rectangle(DVec2::ZERO, DVec2::new(4.0, 3.0)), true)]
        );
        let Outcome::Execute(command) = tool.commit(&doc.project, storey, DVec2::new(4.0, 3.0)) else {
            panic!("the second corner creates the space")
        };
        doc.execute(command).unwrap();
        assert_eq!(doc.history.undo_len(), 1);
        let (_, space) = last_space(&doc);
        assert_eq!(space.source, archicraft_model::BoundarySource::Drawn);
        assert!((space.net_area() - 12.0).abs() < 1e-9);
        assert_eq!(space.long_name, "Room 1");
        assert!(!tool.cancel_step(), "nothing left to cancel");
    }

    /// The tools and the kinds they place map both ways.
    #[test]
    fn tools_map_to_their_element_kinds_and_back() {
        for tool in ToolKind::ALL {
            if let Some(kind) = tool.element_kind() {
                assert_eq!(ToolKind::for_kind(kind), Some(tool));
            }
        }
        assert_eq!(ToolKind::Select.element_kind(), None);
        assert_eq!(ToolKind::Measure.element_kind(), None);
    }
}
