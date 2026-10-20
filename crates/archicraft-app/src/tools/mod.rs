//! Element tools (WAL-001, SLB-001, INP-002…INP-009).
//!
//! A tool collects points in plan; snapping, the constraint modifier and
//! typed distances (`crate::input::PointInput`) are applied before a point
//! reaches the tool. When a tool has enough points, it produces a command.
//! Tools are point-input consumers (`InputConsumer`, INP-001).

mod slab;
mod wall;

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use archicraft_model::{Command, Element, Project, StoreyId};

use crate::input::{Expect, InputConsumer, KeyHint, Outcome, Preview, Resolved, StepSpec};

pub use slab::SlabMode;
pub use wall::WallMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolKind {
    Select,
    Wall,
    Slab,
}

impl ToolKind {
    pub const ALL: [ToolKind; 3] = [ToolKind::Select, ToolKind::Wall, ToolKind::Slab];

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::Select => "Select",
            ToolKind::Wall => "Wall",
            ToolKind::Slab => "Slab",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ToolKind::Select => "Select elements by clicking or by dragging a rectangle",
            ToolKind::Wall => "Draw walls by their reference line",
            ToolKind::Slab => "Draw slabs by their outline",
        }
    }
}

/// The tool's state: which tool, its geometry method, and the points entered
/// so far.
#[derive(Clone, Debug)]
pub struct ToolState {
    pub kind: ToolKind,
    pub wall_mode: WallMode,
    pub slab_mode: SlabMode,
    pub points: Vec<DVec2>,
    /// The storey the input started on.
    pub storey: Option<StoreyId>,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            kind: ToolKind::Select,
            wall_mode: WallMode::Chain,
            slab_mode: SlabMode::Polygon,
            points: Vec::new(),
            storey: None,
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
    }

    pub fn is_drawing(&self) -> bool {
        !self.points.is_empty()
    }

    /// How many points complete the current operation, if fixed.
    fn needed(&self) -> Option<usize> {
        match self.kind {
            ToolKind::Select => None,
            ToolKind::Wall => match self.wall_mode {
                WallMode::Single | WallMode::Rectangle => Some(2),
                WallMode::RotatedRectangle => Some(3),
                WallMode::Chain => None,
            },
            ToolKind::Slab => match self.slab_mode {
                SlabMode::Rectangle => Some(2),
                SlabMode::RotatedRectangle => Some(3),
                SlabMode::Polygon => None,
            },
        }
    }

    /// Adds a point. Returns the command to run once an operation completes.
    ///
    /// Points within the model tolerance are one point (NFR-ACC-003): a
    /// click that coincides with the previous point is ignored, and one that
    /// coincides with the first point closes a wall chain at that point or a
    /// slab outline.
    fn add_point(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        if self.kind == ToolKind::Select {
            return Outcome::Continue;
        }
        if self.storey.is_some_and(|s| s != storey) {
            self.reset();
        }
        self.storey = Some(storey);
        if self.points.last().is_some_and(|&p| coincident(p, point)) {
            return Outcome::Continue;
        }
        match self.kind {
            ToolKind::Wall if self.wall_mode == WallMode::Chain => {
                let closes = self.points.len() >= 2 && coincident(self.points[0], point);
                let Some(&last) = self.points.last() else {
                    self.points.push(point);
                    return Outcome::Continue;
                };
                let end = if closes { self.points[0] } else { point };
                let wall = project.new_wall(storey, last, end, 0);
                if closes {
                    self.reset();
                } else {
                    self.points.push(point);
                }
                Outcome::Execute(Command::AddElements(vec![wall]))
            }
            ToolKind::Slab if self.slab_mode == SlabMode::Polygon => {
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
        let walls_around = |ring: Vec<DVec2>| -> Vec<Element> {
            (0..ring.len())
                .map(|i| project.new_wall(storey, ring[i], ring[(i + 1) % ring.len()], i as u32))
                .collect()
        };
        let elements = match (self.kind, self.wall_mode, self.slab_mode) {
            (ToolKind::Wall, WallMode::Single, _) => vec![project.new_wall(storey, p[0], p[1], 0)],
            (ToolKind::Wall, WallMode::Rectangle, _) => walls_around(rectangle(p[0], p[1])),
            (ToolKind::Wall, WallMode::RotatedRectangle, _) => walls_around(rotated_rectangle(p[0], p[1], p[2])),
            (ToolKind::Slab, _, SlabMode::Rectangle) => {
                vec![project.new_slab(storey, rectangle(p[0], p[1]), Vec::new(), 0)]
            }
            (ToolKind::Slab, _, SlabMode::RotatedRectangle) => {
                vec![project.new_slab(storey, rotated_rectangle(p[0], p[1], p[2]), Vec::new(), 0)]
            }
            _ => return Outcome::Continue,
        };
        Outcome::Execute(Command::AddElements(elements))
    }

    /// Ends a chain or closes a polygon (Enter or double-click).
    fn close(&mut self, project: &Project) -> Outcome {
        let outcome = match (self.kind, self.storey) {
            (ToolKind::Slab, Some(storey)) if self.slab_mode == SlabMode::Polygon && self.points.len() >= 3 => {
                Outcome::Execute(Command::AddElements(vec![project.new_slab(
                    storey,
                    counter_clockwise(self.points.clone()),
                    Vec::new(),
                    0,
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
    /// their reference lines, rectangles and polygons as closed rings.
    pub fn outlines(&self, cursor: DVec2) -> Vec<(Vec<DVec2>, bool)> {
        let p = &self.points;
        if p.is_empty() {
            return Vec::new();
        }
        match (self.kind, self.wall_mode, self.slab_mode, p.len()) {
            (ToolKind::Wall, WallMode::Single | WallMode::Chain, _, _) => {
                vec![(vec![p[p.len() - 1], cursor], false)]
            }
            (ToolKind::Wall, WallMode::Rectangle, _, _) | (ToolKind::Slab, _, SlabMode::Rectangle, _) => {
                vec![(rectangle(p[0], cursor), true)]
            }
            (ToolKind::Wall, WallMode::RotatedRectangle, _, 1) | (ToolKind::Slab, _, SlabMode::RotatedRectangle, 1) => {
                vec![(vec![p[0], cursor], false)]
            }
            (ToolKind::Wall, WallMode::RotatedRectangle, _, _) | (ToolKind::Slab, _, SlabMode::RotatedRectangle, _) => {
                vec![(rotated_rectangle(p[0], p[1], cursor), true)]
            }
            (ToolKind::Slab, _, SlabMode::Polygon, _) => {
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
        let (prompt, repeat) = match self.kind {
            ToolKind::Select => return None,
            ToolKind::Wall => match (self.wall_mode, n) {
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
            },
            ToolKind::Slab => match (self.slab_mode, n) {
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
            },
        };
        let mut hints = Vec::new();
        let expects = match self.points.last() {
            None => {
                if self.kind == ToolKind::Wall {
                    hints.push(KeyHint::new("R", "cycles the reference line"));
                }
                Expect::Point
            }
            Some(&from) => {
                hints.push(KeyHint::new("Tab", "switches between distance and angle"));
                hints.push(KeyHint::new("Shift", "constrains angles"));
                match (self.kind, self.wall_mode, self.slab_mode) {
                    (ToolKind::Wall, WallMode::Chain, _) => {
                        hints.push(KeyHint::new("Enter", "or double-click ends the chain"));
                    }
                    (ToolKind::Slab, _, SlabMode::Polygon) => {
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
        self.points.last().copied()
    }

    fn preview(&self, resolved: &Resolved) -> Preview {
        Preview {
            outlines: self.outlines(resolved.point),
            ..Preview::default()
        }
    }

    fn commit(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        self.add_point(project, storey, point)
    }

    /// A chain ends, keeping its walls; any other operation returns to its
    /// previous point (INP-003).
    fn cancel_step(&mut self) -> bool {
        if self.kind == ToolKind::Wall && self.wall_mode == WallMode::Chain && self.points.len() >= 2 {
            self.reset();
            return true;
        }
        self.pop_point()
    }

    fn backspace(&mut self) -> bool {
        self.pop_point()
    }

    fn finish(&mut self, project: &Project) -> Outcome {
        self.close(project)
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{ElementKind, History};

    use super::*;

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
        assert_eq!(slab.outline.len(), 3);
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
            .map(|i| slab.outline[i].perp_dot(slab.outline[(i + 1) % 4]))
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
        assert_eq!(s.outline.len(), 3);
    }
}
