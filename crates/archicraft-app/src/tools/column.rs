//! The column tool (COL-002): one click places a column at the tool's
//! default rotation, or a first click fixes its position and a second click
//! or a typed angle sets its rotation.
//!
//! The rotation step is an angle step of the point input around the
//! column's position ([`Expect::Angle`]), as the user origin's direction
//! (INP-014): typed values go to the angle first and, with the position as
//! the reference point, a typed angle is measured from the X axis, so it is
//! the absolute rotation; the angle constraint (Shift) and the angle
//! tracker work as for every step (INP-002…INP-006). The preview draws the column's section in plan from
//! the same section the body is swept from
//! ([`archicraft_geometry::kinds::column::plan_footprint`]), so what the
//! user sees is what the click creates. Placement at grid intersections is
//! GRD-005.

use archicraft_geometry::kinds::column::plan_footprint;
use archicraft_model::glam::DVec2;
use archicraft_model::{Command, Element, ElementKind, KindTag, Project, StoreyId};

use super::ToolState;
use crate::input::{Expect, KeyHint, StepSpec};
use crate::tr;

/// Geometry methods of the column tool (COL-002).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColumnMode {
    /// One click places the column at the tool's default rotation.
    #[default]
    Single,
    /// The first click places the column, the second click or a typed
    /// angle sets its rotation.
    Rotated,
}

impl ColumnMode {
    pub const ALL: [ColumnMode; 2] = [ColumnMode::Single, ColumnMode::Rotated];

    /// The stable code in the id of the mode's command,
    /// `tool.column.mode.<code>` (UI-017).
    pub fn code(self) -> &'static str {
        match self {
            ColumnMode::Single => "single",
            ColumnMode::Rotated => "rotated",
        }
    }

    /// How many points complete a column: the position, and for
    /// [`ColumnMode::Rotated`] a point in the direction of the rotation.
    pub(super) fn needed(self) -> usize {
        match self {
            ColumnMode::Single => 1,
            ColumnMode::Rotated => 2,
        }
    }
}

/// The rotation of the column tool's defaults (D.2), in radians.
fn default_rotation(project: &Project) -> f64 {
    project
        .tool_defaults
        .get(KindTag::Column)
        .and_then(|slot| match &slot.kind {
            ElementKind::Column(column) => Some(column.rotation),
            _ => None,
        })
        .unwrap_or(0.0)
}

/// The column the tool places at `position` on `storey`: turned towards
/// `towards` when given, otherwise by the defaults' rotation (COL-002).
fn column_at(project: &Project, storey: StoreyId, position: DVec2, towards: Option<DVec2>) -> Element {
    let rotation = towards.map_or_else(|| default_rotation(project), |p| (p - position).to_angle());
    project.new_column(storey, position, rotation, 0)
}

impl ToolState {
    /// The command placing the column of the completed points (COL-002):
    /// the position and, in the rotated mode, the direction point.
    pub(super) fn column_command(&self, project: &Project, storey: StoreyId) -> Option<Command> {
        let (&position, towards) = self.points.split_first()?;
        let column = column_at(project, storey, position, towards.first().copied());
        Some(Command::AddElements(vec![column]))
    }

    /// The column tool's preview (COL-002): the section of the column a
    /// click at `cursor` places, or, once the position is fixed, the
    /// section there turned towards the cursor with the line of the
    /// direction.
    pub(super) fn column_outlines(&self, project: &Project, cursor: DVec2) -> Vec<(Vec<DVec2>, bool)> {
        let storey = self.storey.unwrap_or_else(|| project.storeys.ground());
        let (column, line) = match self.points.first() {
            None => (column_at(project, storey, cursor, None), None),
            Some(&position) if position == cursor => (column_at(project, storey, position, None), None),
            Some(&position) => (
                column_at(project, storey, position, Some(cursor)),
                Some((vec![position, cursor], false)),
            ),
        };
        plan_footprint(project, &column)
            .unwrap_or_default()
            .into_iter()
            .map(|ring| (ring, true))
            .chain(line)
            .collect()
    }

    /// The column tool's step (COL-002, NFR-USE-003): the position, typed
    /// or clicked, then in the rotated mode the direction of the rotation,
    /// clicked or typed as an absolute angle.
    pub(super) fn column_step(&self) -> StepSpec {
        match self.points.first() {
            None => StepSpec {
                hints: vec![
                    KeyHint::new("0–9", tr!("prompt-column-key-coordinates")),
                    KeyHint::new("P", tr!("prompt-column-key-polar")),
                ],
                ..StepSpec::new(tr!("prompt-column-position"), Expect::Point)
            },
            Some(&centre) => StepSpec {
                hints: vec![
                    KeyHint::new("0–9", tr!("prompt-column-key-angle")),
                    KeyHint::new("Shift", tr!("prompt-column-key-constrain")),
                    KeyHint::new("Esc", tr!("prompt-column-key-cancel")),
                ],
                ..StepSpec::new(tr!("prompt-column-rotation"), Expect::Angle { centre })
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;

    use super::*;
    use crate::input::tests::{Env, index_of, text};
    use crate::input::{Effect, InputConsumer, KeyInput, Outcome, PointInput};
    use crate::tools::ToolKind;

    fn column_tool(mode: ColumnMode) -> ToolState {
        let mut tool = ToolState::default();
        tool.select(ToolKind::Column);
        tool.column_mode = mode;
        tool
    }

    /// The positions and rotations of the columns of `env`'s project.
    fn columns(env: &Env) -> Vec<(DVec2, f64)> {
        env.project
            .elements
            .values()
            .filter_map(|e| e.as_column().map(|c| (c.position, c.rotation)))
            .collect()
    }

    /// COL-002: in the single mode one commit gives one AddElements whose
    /// column stands at the committed point with the defaults' rotation.
    #[test]
    fn a_single_click_places_a_column_with_the_default_rotation() {
        let mut project = default_metric_project();
        let Some(ElementKind::Column(defaults)) = project.tool_defaults.get_mut(KindTag::Column).map(|s| &mut s.kind)
        else {
            panic!("the template has a column slot")
        };
        defaults.rotation = 0.25;
        let storey = project.storeys.ground();
        let mut tool = column_tool(ColumnMode::Single);
        let point = DVec2::new(3.0, 2.0);
        let Outcome::Execute(Command::AddElements(elements)) = tool.commit(&project, storey, point) else {
            panic!("one click places the column")
        };
        let [element] = &elements[..] else {
            panic!("one column: {elements:?}")
        };
        let column = element.as_column().expect("a column");
        assert_eq!((column.position, column.rotation), (point, 0.25));
        assert_eq!(element.storey, storey);
        assert!(!tool.is_drawing(), "the tool is ready for the next column");
    }

    /// COL-002, INP-001: a click snapped through point input places the
    /// column at the snapped point.
    #[test]
    fn the_column_stands_at_the_snapped_point() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = column_tool(ColumnMode::Single);
        let corner = DVec2::new(1.0, 1.0);
        env.targets = index_of(&[corner], &[]);
        assert_eq!(env.click(&mut input, &mut tool, (1.03, 0.98)), Effect::None);
        let placed = columns(&env);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0], (corner, 0.0), "snapped onto the point");
    }

    /// COL-002: in the rotated mode the second click sets the rotation,
    /// 45° for a point on the diagonal.
    #[test]
    fn a_second_click_sets_the_rotation() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = column_tool(ColumnMode::Rotated);
        env.click(&mut input, &mut tool, (2.0, 1.0));
        assert!(columns(&env).is_empty(), "the first click fixes the position");
        assert_eq!(
            tool.step().unwrap().expects,
            Expect::Angle {
                centre: DVec2::new(2.0, 1.0)
            }
        );
        env.click(&mut input, &mut tool, (4.0, 3.0));
        let placed = columns(&env);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0, DVec2::new(2.0, 1.0));
        assert!((placed[0].1 - 45f64.to_radians()).abs() < 1e-12, "{}", placed[0].1);
    }

    /// COL-002, INP-003: at the rotation step a typed 30 and Enter give a
    /// rotation of 30°, an absolute angle.
    #[test]
    fn a_typed_angle_sets_the_rotation() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = column_tool(ColumnMode::Rotated);
        env.click(&mut input, &mut tool, (2.0, 1.0));
        env.key(&mut input, &mut tool, text("30"));
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Enter), Effect::None);
        let placed = columns(&env);
        assert_eq!(placed.len(), 1);
        assert_eq!(placed[0].0, DVec2::new(2.0, 1.0));
        assert!((placed[0].1 - 30f64.to_radians()).abs() < 1e-12, "{}", placed[0].1);
    }

    /// INP-003: Esc after the first click drops only that point; the next
    /// Esc has nothing left to cancel.
    #[test]
    fn esc_after_the_first_click_drops_only_the_position() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = column_tool(ColumnMode::Rotated);
        env.click(&mut input, &mut tool, (2.0, 1.0));
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Escape), Effect::None);
        assert!(tool.points.is_empty() && tool.storey.is_none());
        assert_eq!((tool.kind, tool.column_mode), (ToolKind::Column, ColumnMode::Rotated));
        assert_eq!(tool.step().unwrap().expects, Expect::Point);
        assert_eq!(
            env.key(&mut input, &mut tool, KeyInput::Escape),
            Effect::NothingToCancel
        );
        assert!(columns(&env).is_empty());
    }

    /// The points of `ring` turned so that it starts at its point nearest
    /// to `start`, for comparing rings whatever their first point.
    fn starting_near(ring: &[DVec2], start: DVec2) -> Vec<DVec2> {
        let first = (0..ring.len())
            .min_by(|&a, &b| ring[a].distance(start).total_cmp(&ring[b].distance(start)))
            .unwrap_or(0);
        ring[first..].iter().chain(&ring[..first]).copied().collect()
    }

    /// COL-002: the preview is the 0.30 × 0.30 m square of the defaults,
    /// centred on the cursor (Centre anchor) and turned by the defaults'
    /// rotation; once the position is fixed, the square there turned
    /// towards the cursor, with the direction line.
    #[test]
    fn the_preview_is_the_rotated_section_around_the_cursor() {
        let mut project = default_metric_project();
        let Some(ElementKind::Column(defaults)) = project.tool_defaults.get_mut(KindTag::Column).map(|s| &mut s.kind)
        else {
            panic!("the template has a column slot")
        };
        defaults.rotation = 30f64.to_radians();
        let tool = column_tool(ColumnMode::Rotated);
        let cursor = DVec2::new(5.0, 4.0);
        let square = |centre: DVec2, angle: f64| -> Vec<DVec2> {
            [(-0.15, -0.15), (0.15, -0.15), (0.15, 0.15), (-0.15, 0.15)]
                .map(|(x, y)| centre + DVec2::from_angle(angle).rotate(DVec2::new(x, y)))
                .to_vec()
        };
        let close = |ring: &[DVec2], expected: &[DVec2]| {
            let ring = starting_near(ring, expected[0]);
            ring.len() == expected.len() && ring.iter().zip(expected).all(|(a, b)| a.distance(*b) < 1e-12)
        };
        let outlines = tool.outlines(&project, cursor);
        let [(ring, true)] = &outlines[..] else {
            panic!("one closed ring: {outlines:?}")
        };
        let expected = square(cursor, 30f64.to_radians());
        assert!(close(ring, &expected), "{ring:?} != {expected:?}");

        let mut tool = tool;
        let storey = project.storeys.ground();
        let position = DVec2::new(1.0, 1.0);
        assert_eq!(tool.commit(&project, storey, position), Outcome::Continue);
        let towards = DVec2::new(1.0, 3.0);
        let outlines = tool.outlines(&project, towards);
        let [(ring, true), (line, false)] = &outlines[..] else {
            panic!("the section and the direction line: {outlines:?}")
        };
        let expected = square(position, 90f64.to_radians());
        assert!(close(ring, &expected), "{ring:?} != {expected:?}");
        assert_eq!(line, &vec![position, towards]);
    }
}
