//! Element tools and point input (WAL-001, SLB-001, INP-002…INP-009).
//!
//! A tool collects points in plan; snapping, the constraint modifier and
//! typed distances are applied before a point reaches the tool. When a tool
//! has enough points, it produces a command.

use archicraft_geometry::plan::{PlanDrawing, polygon_rings};
use archicraft_model::glam::DVec2;
use archicraft_model::units::{NumberFormat, UnitSettings, format_angle, format_length, parse_angle, parse_length};
use archicraft_model::{Command, Element, Project, StoreyId};

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

/// Geometry methods of the wall tool (WAL-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallMode {
    Single,
    Chain,
    Rectangle,
    RotatedRectangle,
}

impl WallMode {
    pub const ALL: [WallMode; 4] = [
        WallMode::Single,
        WallMode::Chain,
        WallMode::Rectangle,
        WallMode::RotatedRectangle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WallMode::Single => "Single",
            WallMode::Chain => "Chain",
            WallMode::Rectangle => "Rectangle",
            WallMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}

/// Geometry methods of the slab tool (SLB-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlabMode {
    Polygon,
    Rectangle,
    RotatedRectangle,
}

impl SlabMode {
    pub const ALL: [SlabMode; 3] = [SlabMode::Polygon, SlabMode::Rectangle, SlabMode::RotatedRectangle];

    pub fn label(self) -> &'static str {
        match self {
            SlabMode::Polygon => "Polygon",
            SlabMode::Rectangle => "Rectangle",
            SlabMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}

/// Which typed value the keyboard currently edits (INP-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Field {
    #[default]
    Distance,
    Angle,
}

/// Values typed during point input, relative to the previous point in polar
/// coordinates (INP-004).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypedInput {
    pub field: Field,
    pub distance: String,
    pub angle: String,
}

impl TypedInput {
    pub fn is_empty(&self) -> bool {
        self.distance.is_empty() && self.angle.is_empty()
    }

    pub fn active_text(&mut self) -> &mut String {
        match self.field {
            Field::Distance => &mut self.distance,
            Field::Angle => &mut self.angle,
        }
    }

    pub fn clear(&mut self) {
        *self = TypedInput::default();
    }
}

/// Kinds of snap, in order of precedence (INP-009).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SnapKind {
    Node,
    Intersection,
    Midpoint,
    Perpendicular,
    Edge,
    Grid,
    Constrained,
    Typed,
    Free,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Node => "node",
            SnapKind::Intersection => "intersection",
            SnapKind::Midpoint => "midpoint",
            SnapKind::Perpendicular => "perpendicular",
            SnapKind::Edge => "on edge",
            SnapKind::Grid => "grid",
            SnapKind::Constrained => "constrained",
            SnapKind::Typed => "typed",
            SnapKind::Free => "",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub point: DVec2,
    pub kind: SnapKind,
}

/// Snap targets of one storey's plan (INP-008).
#[derive(Clone, Debug, Default)]
pub struct SnapTargets {
    pub nodes: Vec<DVec2>,
    pub edges: Vec<[DVec2; 2]>,
}

impl SnapTargets {
    pub fn from_plan(plan: &PlanDrawing, project: &Project, storey: StoreyId) -> Self {
        let mut targets = SnapTargets::default();
        for element in &plan.elements {
            for polygon in &element.cut {
                for ring in polygon_rings(polygon) {
                    for i in 0..ring.len() {
                        targets.nodes.push(ring[i]);
                        targets.edges.push([ring[i], ring[(i + 1) % ring.len()]]);
                    }
                }
            }
            for segment in element.projected.iter().chain(&element.overhead) {
                targets.nodes.extend(segment);
                targets.edges.push(*segment);
            }
        }
        for element in project.elements_on(storey).filter(|e| project.is_visible(e)) {
            if let Some(wall) = element.as_wall() {
                targets.nodes.extend([wall.start, wall.end]);
                targets.edges.push([wall.start, wall.end]);
            }
        }
        targets
    }
}

fn closest_on_segment(p: DVec2, [a, b]: [DVec2; 2]) -> DVec2 {
    let ab = b - a;
    let len2 = ab.length_squared();
    let t = if len2 > 0.0 {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    a + ab * t
}

fn segment_intersection([a, b]: [DVec2; 2], [c, d]: [DVec2; 2]) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let denom = r.perp_dot(s);
    if denom.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / denom;
    let u = (c - a).perp_dot(r) / denom;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then(|| a + r * t)
}

/// Finds the snap point for `cursor` within `radius` (model units), with the
/// fixed precedence of INP-009: nodes, intersections, midpoints,
/// perpendicular feet from the previous point, edges, then the snap grid.
pub fn snap(targets: &SnapTargets, cursor: DVec2, radius: f64, grid_step: f64, previous: Option<DVec2>) -> Snap {
    let mut best: Option<(SnapKind, f64, DVec2)> = None;
    let mut consider = |kind: SnapKind, p: DVec2| {
        let d = p.distance(cursor);
        if d <= radius && best.is_none_or(|(k, bd, _)| (kind, d) < (k, bd)) {
            best = Some((kind, d, p));
        }
    };
    for &n in &targets.nodes {
        consider(SnapKind::Node, n);
    }
    let near: Vec<[DVec2; 2]> = targets
        .edges
        .iter()
        .copied()
        .filter(|e| closest_on_segment(cursor, *e).distance(cursor) <= radius)
        .collect();
    for (i, a) in near.iter().enumerate() {
        for b in &near[i + 1..] {
            if let Some(p) = segment_intersection(*a, *b) {
                consider(SnapKind::Intersection, p);
            }
        }
    }
    for e in &near {
        consider(SnapKind::Midpoint, (e[0] + e[1]) / 2.0);
        if let Some(prev) = previous {
            let dir = (e[1] - e[0]).normalize_or_zero();
            let foot = e[0] + dir * (prev - e[0]).dot(dir);
            if closest_on_segment(foot, *e).distance(foot) < 1e-9 {
                consider(SnapKind::Perpendicular, foot);
            }
        }
        consider(SnapKind::Edge, closest_on_segment(cursor, *e));
    }
    if let Some((kind, _, point)) = best {
        return Snap { point, kind };
    }
    if grid_step > 0.0 {
        let point = (cursor / grid_step).round() * grid_step;
        if point.distance(cursor) <= radius {
            return Snap {
                point,
                kind: SnapKind::Grid,
            };
        }
    }
    Snap {
        point: cursor,
        kind: SnapKind::Free,
    }
}

/// Restricts a point to the nearest multiple of `increment_deg` around
/// `from` (INP-006), keeping its distance along that direction.
pub fn constrain(from: DVec2, to: DVec2, increment_deg: f64) -> DVec2 {
    let d = to - from;
    if d.length() < 1e-12 {
        return to;
    }
    let step = increment_deg.to_radians();
    let angle = (d.y.atan2(d.x) / step).round() * step;
    let dir = DVec2::new(angle.cos(), angle.sin());
    from + dir * d.dot(dir).max(0.0)
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
    pub typed: TypedInput,
}

impl Default for ToolState {
    fn default() -> Self {
        Self {
            kind: ToolKind::Select,
            wall_mode: WallMode::Chain,
            slab_mode: SlabMode::Polygon,
            points: Vec::new(),
            storey: None,
            typed: TypedInput::default(),
        }
    }
}

/// What a tool wants after an input.
#[derive(Debug)]
pub enum ToolOutcome {
    Nothing,
    Execute(Command),
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

const CLOSE_TOLERANCE: f64 = 1e-6;

impl ToolState {
    pub fn select(&mut self, kind: ToolKind) {
        self.kind = kind;
        self.reset();
    }

    pub fn reset(&mut self) {
        self.points.clear();
        self.storey = None;
        self.typed.clear();
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

    /// The prompt for the next input (NFR-USE-003).
    pub fn prompt(&self) -> String {
        let n = self.points.len();
        let keys = "Type a distance, Tab for the angle · Shift constrains angles · Esc cancels";
        match self.kind {
            ToolKind::Select => {
                "Click an element to select it, Shift-click to add or remove, drag a rectangle to select several · Tab cycles overlapping elements".into()
            }
            ToolKind::Wall => match (self.wall_mode, n) {
                (WallMode::Rectangle, 0) => "Wall rectangle: click the first corner".into(),
                (WallMode::Rectangle, _) => format!("Wall rectangle: click the opposite corner · {keys}"),
                (WallMode::RotatedRectangle, 0) => "Rotated wall rectangle: click the start of the base edge".into(),
                (WallMode::RotatedRectangle, 1) => format!("Rotated wall rectangle: click the end of the base edge · {keys}"),
                (WallMode::RotatedRectangle, _) => "Rotated wall rectangle: click to set the width".into(),
                (_, 0) => "Wall: click the start point of the reference line · R cycles the reference line".into(),
                (WallMode::Chain, _) => format!(
                    "Wall chain: click the next point; Enter or double-click ends the chain, Backspace removes the last point · {keys}"
                ),
                (_, _) => format!("Wall: click the end point · {keys}"),
            },
            ToolKind::Slab => match (self.slab_mode, n) {
                (SlabMode::Polygon, 0) => "Slab: click the first corner of the outline".into(),
                (SlabMode::Polygon, _) => format!(
                    "Slab: click the next corner; click the first corner, press Enter or double-click to close · {keys}"
                ),
                (SlabMode::Rectangle, 0) => "Slab rectangle: click the first corner".into(),
                (SlabMode::Rectangle, _) => format!("Slab rectangle: click the opposite corner · {keys}"),
                (SlabMode::RotatedRectangle, 0) => "Rotated slab rectangle: click the start of the base edge".into(),
                (SlabMode::RotatedRectangle, 1) => format!("Rotated slab rectangle: click the end of the base edge · {keys}"),
                (SlabMode::RotatedRectangle, _) => "Rotated slab rectangle: click to set the width".into(),
            },
        }
    }

    /// Applies typed values to the cursor position (INP-003, INP-004):
    /// a typed distance and/or angle from the previous point.
    pub fn apply_typed(&self, cursor: DVec2, units: &UnitSettings, nf: &NumberFormat) -> Option<DVec2> {
        let previous = *self.points.last()?;
        if self.typed.is_empty() {
            return None;
        }
        let offset = cursor - previous;
        let angle = if self.typed.angle.is_empty() {
            offset.y.atan2(offset.x)
        } else {
            parse_angle(&self.typed.angle, units.angle, nf).ok()?
        };
        let distance = if self.typed.distance.is_empty() {
            offset.length()
        } else {
            parse_length(&self.typed.distance, units.length, nf).ok()?
        };
        Some(previous + DVec2::new(angle.cos(), angle.sin()) * distance)
    }

    /// The tracker text next to the cursor (INP-002).
    pub fn tracker(&self, point: DVec2, units: &UnitSettings, nf: &NumberFormat) -> Option<String> {
        let previous = *self.points.last()?;
        let d = point - previous;
        let mark = |field: Field| if self.typed.field == field { "▸" } else { " " };
        let distance = if self.typed.distance.is_empty() {
            format_length(d.length(), units, nf, true)
        } else {
            format!("{}_", self.typed.distance)
        };
        let angle = if self.typed.angle.is_empty() {
            format_angle(d.y.atan2(d.x), units, nf, true)
        } else {
            format!("{}_", self.typed.angle)
        };
        Some(format!(
            "{}d {distance}\n{}a {angle}",
            mark(Field::Distance),
            mark(Field::Angle)
        ))
    }

    /// Adds a point. Returns the command to run once an operation completes.
    pub fn add_point(&mut self, project: &Project, storey: StoreyId, point: DVec2) -> ToolOutcome {
        if self.kind == ToolKind::Select {
            return ToolOutcome::Nothing;
        }
        if self.storey.is_some_and(|s| s != storey) {
            self.reset();
        }
        self.storey = Some(storey);
        self.typed.clear();
        if self.points.last().is_some_and(|p| p.distance(point) < CLOSE_TOLERANCE) {
            return ToolOutcome::Nothing;
        }
        match self.kind {
            ToolKind::Wall if self.wall_mode == WallMode::Chain => {
                let closes = self.points.len() >= 2 && self.points[0].distance(point) < CLOSE_TOLERANCE;
                let Some(&last) = self.points.last() else {
                    self.points.push(point);
                    return ToolOutcome::Nothing;
                };
                let wall = project.new_wall(storey, last, point, 0);
                if closes {
                    self.reset();
                } else {
                    self.points.push(point);
                }
                ToolOutcome::Execute(Command::AddElements(vec![wall]))
            }
            ToolKind::Slab if self.slab_mode == SlabMode::Polygon => {
                if self.points.len() >= 3 && self.points[0].distance(point) < CLOSE_TOLERANCE {
                    return self.finish(project);
                }
                self.points.push(point);
                ToolOutcome::Nothing
            }
            _ => {
                self.points.push(point);
                if Some(self.points.len()) == self.needed() {
                    let outcome = self.complete(project, storey);
                    self.reset();
                    outcome
                } else {
                    ToolOutcome::Nothing
                }
            }
        }
    }

    fn complete(&self, project: &Project, storey: StoreyId) -> ToolOutcome {
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
            _ => return ToolOutcome::Nothing,
        };
        ToolOutcome::Execute(Command::AddElements(elements))
    }

    /// Ends a chain or closes a polygon (Enter or double-click).
    pub fn finish(&mut self, project: &Project) -> ToolOutcome {
        let outcome = match (self.kind, self.storey) {
            (ToolKind::Slab, Some(storey)) if self.slab_mode == SlabMode::Polygon && self.points.len() >= 3 => {
                ToolOutcome::Execute(Command::AddElements(vec![project.new_slab(
                    storey,
                    counter_clockwise(self.points.clone()),
                    Vec::new(),
                    0,
                )]))
            }
            _ => ToolOutcome::Nothing,
        };
        self.reset();
        outcome
    }

    /// Removes the last entered point (UND-003).
    pub fn backspace(&mut self) {
        self.points.pop();
        if self.points.is_empty() {
            self.storey = None;
        }
    }

    /// Outlines previewing the operation for a cursor position: walls as
    /// their reference lines, rectangles and polygons as closed rings.
    pub fn preview(&self, cursor: DVec2) -> Vec<(Vec<DVec2>, bool)> {
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

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{ElementKind, History};

    use super::*;

    fn run(project: &mut Project, outcome: ToolOutcome) {
        if let ToolOutcome::Execute(command) = outcome {
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
            let outcome = tool.add_point(&project, storey, DVec2::new(p.0, p.1));
            run(&mut project, outcome);
        }
        assert_eq!(project.elements.len(), 4);
        assert!(!tool.is_drawing(), "clicking the first point closes the chain");
        let names: Vec<&str> = project.elements.values().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["W-001", "W-002", "W-003", "W-004"]);
    }

    #[test]
    fn rectangle_tools() {
        let mut project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        tool.wall_mode = WallMode::Rectangle;
        assert!(matches!(
            tool.add_point(&project, storey, DVec2::new(4.0, 3.0)),
            ToolOutcome::Nothing
        ));
        let outcome = tool.add_point(&project, storey, DVec2::ZERO);
        run(&mut project, outcome);
        assert_eq!(project.elements.len(), 4);
        // Counter-clockwise: exterior faces outside.
        let first = project.elements.values().next().unwrap().as_wall().unwrap().clone();
        assert_eq!((first.start, first.end), (DVec2::new(4.0, 3.0), DVec2::new(0.0, 3.0)));

        tool.select(ToolKind::Slab);
        tool.slab_mode = SlabMode::RotatedRectangle;
        for p in [(0.0, 0.0), (3.0, 3.0)] {
            tool.add_point(&project, storey, DVec2::new(p.0, p.1));
        }
        let outcome = tool.add_point(&project, storey, DVec2::new(-1.0, 1.0));
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
            tool.add_point(&project, storey, DVec2::new(p.0, p.1));
        }
        tool.backspace();
        tool.add_point(&project, storey, DVec2::new(6.0, 0.0));
        let outcome = tool.finish(&project);
        run(&mut project, outcome);
        let slab = project.elements.values().next().unwrap();
        let ElementKind::Slab(s) = &slab.kind else { panic!() };
        assert_eq!(s.outline.len(), 3);
    }

    #[test]
    fn snapping_precedence() {
        let targets = SnapTargets {
            nodes: vec![DVec2::new(1.0, 1.0)],
            edges: vec![
                [DVec2::new(0.0, 0.0), DVec2::new(4.0, 0.0)],
                [DVec2::new(2.0, -2.0), DVec2::new(2.0, 2.0)],
            ],
        };
        let s = snap(&targets, DVec2::new(1.05, 0.98), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Node);
        let s = snap(&targets, DVec2::new(2.1, 0.1), 0.2, 0.05, None);
        assert_eq!((s.kind, s.point), (SnapKind::Intersection, DVec2::new(2.0, 0.0)));
        let s = snap(&targets, DVec2::new(3.0, 0.05), 0.2, 0.0, None);
        assert_eq!(s.kind, SnapKind::Edge);
        let s = snap(&targets, DVec2::new(10.02, 10.01), 0.2, 0.05, None);
        assert_eq!(s.kind, SnapKind::Grid);
        assert!(s.point.distance(DVec2::new(10.0, 10.0)) < 1e-12);
    }

    #[test]
    fn typed_distance_and_constraint() {
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        tool.points.push(DVec2::ZERO);
        tool.typed.distance = "2500mm".into();
        let units = UnitSettings::default();
        let p = tool
            .apply_typed(DVec2::new(1.0, 0.0), &units, &NumberFormat::default())
            .unwrap();
        assert!(p.distance(DVec2::new(2.5, 0.0)) < 1e-12);
        tool.typed.angle = "90".into();
        let p = tool
            .apply_typed(DVec2::new(1.0, 0.0), &units, &NumberFormat::default())
            .unwrap();
        assert!(p.distance(DVec2::new(0.0, 2.5)) < 1e-12);
        let c = constrain(DVec2::ZERO, DVec2::new(3.0, 0.4), 45.0);
        assert!(c.distance(DVec2::new(3.0, 0.0)) < 1e-12);
    }
}
