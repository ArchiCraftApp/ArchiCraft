//! The headless point-input service (INP-001…INP-009).
//!
//! Every point the user defines passes through this module: typed values,
//! the angle constraint and snapping. It knows nothing of the windowing
//! toolkit, so plan views, 3D views and every tool share it. [`PointInput`]
//! resolves the cursor into a point and feeds keys and clicks to an
//! [`InputConsumer`], identically at every step of every consumer.

mod constrain;
mod consumer;
mod keys;
mod snap;
mod surface;
mod typed;

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use archicraft_model::units::{NumberFormat, UnitSettings, format_angle, format_length, parse_angle, parse_length};
use archicraft_model::{Command, Project, StoreyId};

pub use constrain::constrain;
pub use consumer::{CursorState, Expect, InputConsumer, KeyHint, Outcome, Preview, StepSpec, TrackerMode};
pub use keys::{KeyInput, Modifiers};
pub use snap::{Snap, SnapKind, SnapTargets, snap};
pub use surface::{SurfaceFrame, ViewSurface};
pub use typed::{Field, TypedInput, is_numeric_input};

/// A guide line shown while resolving a point; inference guides (INP-011)
/// fill these.
#[derive(Clone, Debug, PartialEq)]
pub enum GuideShape {
    Line([DVec2; 2]),
}

/// A cursor position resolved by point input: the point a commit takes, how
/// it was found and what is shown around it.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub point: DVec2,
    pub snap: Snap,
    /// The snaps the user can choose between at this position. Until
    /// candidate cycling arrives this is the chosen snap alone, or nothing
    /// for a free point.
    pub candidates: Vec<Snap>,
    pub guides: Vec<GuideShape>,
}

/// Everything point input needs to resolve a point on one view's working
/// plane, for the current step of a consumer.
#[derive(Clone, Debug)]
pub struct InputContext<'a> {
    pub project: &'a Project,
    pub storey: StoreyId,
    pub targets: &'a SnapTargets,
    /// The consumer's reference point on this storey.
    pub previous: Option<DVec2>,
    /// What the consumer's current step expects.
    pub expects: Expect,
    /// The snap radius in model units: the radius in pixels times the
    /// surface's model length per pixel.
    pub radius: f64,
    pub units: &'a UnitSettings,
    pub number_format: &'a NumberFormat,
    pub angle_increment_deg: f64,
    pub snap_step: f64,
    pub frame: SurfaceFrame,
}

/// What the view does after a key or a click.
#[derive(Debug, PartialEq)]
pub enum Effect {
    None,
    Execute(Command),
    Error(String),
    /// Esc found neither typed text nor a step to cancel: the view goes on
    /// with the rest of the Esc ladder (INP-003).
    NothingToCancel,
}

impl From<Outcome> for Effect {
    fn from(outcome: Outcome) -> Self {
        match outcome {
            Outcome::Execute(command) => Effect::Execute(command),
            Outcome::Continue | Outcome::Done => Effect::None,
        }
    }
}

/// The state of point input between events: the typed values and the last
/// cursor position (INP-001, INP-003).
#[derive(Clone, Debug, Default)]
pub struct PointInput {
    pub typed: TypedInput,
    cursor: Option<(DVec2, Modifiers)>,
    last: Option<Resolved>,
}

impl PointInput {
    /// The point resolved for the last cursor position.
    pub fn last(&self) -> Option<&Resolved> {
        self.last.as_ref()
    }

    /// Whether keys belong to point input rather than to single-key
    /// shortcuts: while the consumer's step takes typed values or a value is
    /// typed (INP-003, UI-008).
    pub fn captures_keys(&self, consumer: &dyn InputConsumer) -> bool {
        !self.typed.is_empty()
            || consumer
                .step()
                .is_some_and(|s| s.expects.origin(consumer.reference_point()).is_some())
    }

    /// The tracker text next to the cursor (INP-002): distance and angle of
    /// `point` from the origin of the step, or the typed values with the
    /// active field marked.
    pub fn tracker(&self, point: DVec2, ctx: &InputContext) -> Option<String> {
        let d = point - ctx.expects.origin(ctx.previous)?;
        let mark = |field: Field| if self.typed.field == field { "▸" } else { " " };
        let distance = if self.typed.distance.is_empty() {
            format_length(d.length(), ctx.units, ctx.number_format, true)
        } else {
            format!("{}_", self.typed.distance)
        };
        let angle = if self.typed.angle.is_empty() {
            format_angle(d.to_angle(), ctx.units, ctx.number_format, true)
        } else {
            format!("{}_", self.typed.angle)
        };
        Some(format!(
            "{}d {distance}\n{}a {angle}",
            mark(Field::Distance),
            mark(Field::Angle)
        ))
    }

    /// Resolves a cursor position on the working plane: typed values first,
    /// then the angle constraint while Shift is held, then snapping
    /// (INP-003, INP-006, INP-009).
    pub fn resolve(&mut self, cursor: DVec2, mods: Modifiers, ctx: &InputContext) -> Resolved {
        self.cursor = Some((cursor, mods));
        let origin = ctx.expects.origin(ctx.previous);
        let typed = origin
            .filter(|_| !self.typed.is_empty())
            .and_then(|o| self.apply_typed(o, cursor, ctx))
            .map(|point| Snap {
                point,
                kind: SnapKind::Typed,
            });
        let constrained = || {
            origin.filter(|_| mods.shift).map(|o| Snap {
                point: constrain(o, cursor, ctx.angle_increment_deg),
                kind: SnapKind::Constrained,
            })
        };
        let s = typed
            .or_else(constrained)
            .unwrap_or_else(|| snap(ctx.targets, cursor, ctx.radius, ctx.snap_step, ctx.previous));
        let candidates = if s.kind == SnapKind::Free { Vec::new() } else { vec![s] };
        let resolved = Resolved {
            point: s.point,
            snap: s,
            candidates,
            guides: Vec::new(),
        };
        self.last = Some(resolved.clone());
        resolved
    }

    /// The typed distance and angle applied from `origin` (INP-003,
    /// INP-004). An empty field takes its value from the cursor; an angle
    /// step measures the typed angle from the reference direction.
    fn apply_typed(&self, origin: DVec2, cursor: DVec2, ctx: &InputContext) -> Option<DVec2> {
        let offset = cursor - origin;
        let angle = if self.typed.angle.is_empty() {
            offset.to_angle()
        } else {
            let base = match ctx.expects {
                Expect::Angle { centre } => ctx
                    .previous
                    .filter(|p| !coincident(*p, centre))
                    .map_or(0.0, |p| (p - centre).to_angle()),
                _ => 0.0,
            };
            base + parse_angle(&self.typed.angle, ctx.units.angle, ctx.number_format).ok()?
        };
        let distance = if self.typed.distance.is_empty() {
            offset.length()
        } else {
            parse_length(&self.typed.distance, ctx.units.length, ctx.number_format).ok()?
        };
        Some(origin + DVec2::from_angle(angle) * distance)
    }

    /// Handles a key (INP-003): characters go to the active field, Tab moves
    /// between fields, Enter commits the typed point or finishes a repeating
    /// step, Backspace removes a character and then a point, and Esc clears
    /// the typed text and then cancels one step.
    pub fn key(&mut self, key: KeyInput, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        let origin = ctx.expects.origin(ctx.previous);
        let typing = consumer.step().is_some() && origin.is_some();
        match key {
            KeyInput::Text(text) if typing => {
                if self.typed == TypedInput::default() && matches!(ctx.expects, Expect::Angle { .. }) {
                    self.typed.field = Field::Angle;
                }
                self.typed
                    .active_text()
                    .extend(text.chars().filter(|c| is_numeric_input(*c)));
                Effect::None
            }
            KeyInput::Tab if typing => {
                self.typed.field = match self.typed.field {
                    Field::Distance => Field::Angle,
                    Field::Angle => Field::Distance,
                };
                Effect::None
            }
            KeyInput::Enter if consumer.step().is_some() => {
                if self.typed.is_empty() {
                    return consumer.finish(ctx.project).into();
                }
                let (cursor, mods) = self
                    .cursor
                    .unwrap_or((origin.unwrap_or_default() + DVec2::X, Modifiers::default()));
                let resolved = self.resolve(cursor, mods, ctx);
                if resolved.snap.kind == SnapKind::Typed {
                    self.commit(resolved.point, consumer, ctx)
                } else {
                    Effect::Error("The typed value is not a valid length or angle.".into())
                }
            }
            KeyInput::Backspace => {
                if self.typed.is_empty() {
                    consumer.backspace();
                } else if self.typed.active_text().pop().is_none() {
                    self.typed.field = match self.typed.field {
                        Field::Distance => Field::Angle,
                        Field::Angle => Field::Distance,
                    };
                    self.typed.active_text().pop();
                }
                Effect::None
            }
            KeyInput::Escape => {
                if !self.typed.is_empty() {
                    self.typed.clear();
                    Effect::None
                } else if consumer.cancel_step() {
                    Effect::None
                } else {
                    Effect::NothingToCancel
                }
            }
            _ => Effect::None,
        }
    }

    /// Commits the point resolved for a click.
    pub fn click(
        &mut self,
        cursor: DVec2,
        mods: Modifiers,
        consumer: &mut dyn InputConsumer,
        ctx: &InputContext,
    ) -> Effect {
        if consumer.step().is_none() {
            return Effect::None;
        }
        let resolved = self.resolve(cursor, mods, ctx);
        self.commit(resolved.point, consumer, ctx)
    }

    /// Finishes a repeating step (a chain, a polygon, multiple copies).
    pub fn double_click(&mut self, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        self.typed.clear();
        consumer.finish(ctx.project).into()
    }

    fn commit(&mut self, point: DVec2, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        self.typed.clear();
        consumer.commit(ctx.project, ctx.storey, point).into()
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Element, ElementId, History};

    use super::*;
    use crate::tools::{SlabMode, ToolKind, ToolState, WallMode};

    /// How a [`TransformStub`] places the result.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Transform {
        Move,
        Copy,
        MultipleCopy,
    }

    /// A move-by-vector consumer (EDT-001): a base point, then the target.
    struct TransformStub {
        ids: Vec<ElementId>,
        outline: Vec<DVec2>,
        mode: Transform,
        base: Option<DVec2>,
        last: Option<DVec2>,
        done: bool,
    }

    impl TransformStub {
        fn new(project: &Project, ids: Vec<ElementId>, mode: Transform) -> Self {
            let outline = ids
                .iter()
                .filter_map(|id| project.element(*id)?.as_wall())
                .flat_map(|w| [w.start, w.end])
                .collect();
            Self {
                ids,
                outline,
                mode,
                base: None,
                last: None,
                done: false,
            }
        }

        fn copies(&self, project: &Project, delta: DVec2) -> Vec<Element> {
            self.ids
                .iter()
                .filter_map(|id| project.element(*id))
                .map(|e| {
                    let mut copy = e.clone();
                    copy.id = ElementId::new();
                    copy.kind.translate(delta);
                    copy
                })
                .collect()
        }
    }

    impl InputConsumer for TransformStub {
        fn step(&self) -> Option<StepSpec> {
            match (self.done, self.base) {
                (true, _) => None,
                (false, None) => Some(StepSpec::new("Move: click the base point", Expect::Point)),
                (false, Some(from)) => Some(StepSpec {
                    repeat: self.mode == Transform::MultipleCopy,
                    ..StepSpec::new("Move: click the target point", Expect::Distance { from })
                }),
            }
        }

        fn reference_point(&self) -> Option<DVec2> {
            self.base
        }

        fn preview(&self, _project: &Project, resolved: &Resolved) -> Preview {
            let Some(base) = self.base else {
                return Preview::default();
            };
            let delta = resolved.point - base;
            Preview {
                outlines: vec![(self.outline.iter().map(|p| *p + delta).collect(), false)],
                cursor: CursorState::Normal,
            }
        }

        fn commit(&mut self, project: &Project, _storey: StoreyId, point: DVec2) -> Outcome {
            let Some(base) = self.base else {
                self.base = Some(point);
                return Outcome::Continue;
            };
            if self.last.is_some_and(|p| coincident(p, point)) {
                return Outcome::Continue;
            }
            let delta = point - base;
            match self.mode {
                Transform::Move => {
                    self.done = true;
                    Outcome::Execute(Command::MoveElements {
                        ids: self.ids.clone(),
                        delta,
                    })
                }
                Transform::Copy => {
                    self.done = true;
                    Outcome::Execute(Command::AddElements(self.copies(project, delta)))
                }
                Transform::MultipleCopy => {
                    self.last = Some(point);
                    Outcome::Execute(Command::AddElements(self.copies(project, delta)))
                }
            }
        }

        fn cancel_step(&mut self) -> bool {
            if self.done {
                false
            } else if self.last.is_some() {
                self.done = true;
                true
            } else {
                self.base.take().is_some()
            }
        }

        fn backspace(&mut self) -> bool {
            self.last.is_none() && self.base.take().is_some()
        }

        fn finish(&mut self, _project: &Project) -> Outcome {
            if self.last.is_some() {
                self.done = true;
                Outcome::Done
            } else {
                Outcome::Continue
            }
        }
    }

    /// A rotate-shaped consumer (EDT-001): the centre, a reference direction,
    /// then the target direction or a typed angle.
    #[derive(Default)]
    struct RotateStub {
        centre: Option<DVec2>,
        reference: Option<DVec2>,
        angle: Option<f64>,
    }

    impl InputConsumer for RotateStub {
        fn step(&self) -> Option<StepSpec> {
            match (self.centre, self.reference, self.angle) {
                (_, _, Some(_)) => None,
                (None, _, _) => Some(StepSpec::new("Rotate: click the centre", Expect::Point)),
                (Some(from), None, _) => Some(StepSpec::new(
                    "Rotate: click the reference direction",
                    Expect::Direction { from },
                )),
                (Some(centre), Some(_), _) => Some(StepSpec::new(
                    "Rotate: click the target direction or type the angle",
                    Expect::Angle { centre },
                )),
            }
        }

        fn reference_point(&self) -> Option<DVec2> {
            self.reference.or(self.centre)
        }

        fn preview(&self, _project: &Project, resolved: &Resolved) -> Preview {
            Preview {
                outlines: self
                    .centre
                    .map(|c| (vec![c, resolved.point], false))
                    .into_iter()
                    .collect(),
                cursor: CursorState::Normal,
            }
        }

        fn commit(&mut self, _project: &Project, _storey: StoreyId, point: DVec2) -> Outcome {
            match (self.centre, self.reference) {
                (None, _) => self.centre = Some(point),
                (Some(_), None) => self.reference = Some(point),
                (Some(c), Some(r)) => {
                    let (a, b) = (r - c, point - c);
                    self.angle = Some(a.perp_dot(b).atan2(a.dot(b)));
                    return Outcome::Done;
                }
            }
            Outcome::Continue
        }

        fn cancel_step(&mut self) -> bool {
            self.reference.take().or_else(|| self.centre.take()).is_some()
        }

        fn backspace(&mut self) -> bool {
            self.cancel_step()
        }

        fn finish(&mut self, _project: &Project) -> Outcome {
            Outcome::Continue
        }
    }

    struct Env {
        project: Project,
        storey: StoreyId,
        targets: SnapTargets,
        units: UnitSettings,
        nf: NumberFormat,
    }

    impl Env {
        fn new() -> Self {
            let project = default_metric_project();
            let storey = project.storeys.ground();
            Self {
                project,
                storey,
                targets: SnapTargets::default(),
                units: UnitSettings::default(),
                nf: NumberFormat::default(),
            }
        }

        /// The context of `consumer`'s current step, as a view builds it.
        fn ctx(&self, consumer: &dyn InputConsumer) -> InputContext<'_> {
            InputContext {
                project: &self.project,
                storey: self.storey,
                targets: &self.targets,
                previous: consumer.reference_point(),
                expects: consumer.step().map_or(Expect::Point, |s| s.expects),
                radius: 0.1,
                units: &self.units,
                number_format: &self.nf,
                angle_increment_deg: 45.0,
                snap_step: 0.0,
                frame: SurfaceFrame::plan(0.0),
            }
        }

        fn click(&mut self, input: &mut PointInput, consumer: &mut dyn InputConsumer, at: (f64, f64)) -> Effect {
            let ctx = self.ctx(consumer);
            let effect = input.click(DVec2::new(at.0, at.1), Modifiers::default(), consumer, &ctx);
            self.run(effect)
        }

        fn key(&mut self, input: &mut PointInput, consumer: &mut dyn InputConsumer, key: KeyInput) -> Effect {
            let ctx = self.ctx(consumer);
            let effect = input.key(key, consumer, &ctx);
            self.run(effect)
        }

        /// Executes a command effect and returns the rest.
        fn run(&mut self, effect: Effect) -> Effect {
            match effect {
                Effect::Execute(command) => {
                    History::default().execute(&mut self.project, command).unwrap();
                    Effect::None
                }
                other => other,
            }
        }

        fn walls(&self) -> Vec<(DVec2, DVec2)> {
            self.project
                .elements
                .values()
                .filter_map(|e| e.as_wall())
                .map(|w| (w.start, w.end))
                .collect()
        }
    }

    fn wall_tool(mode: WallMode) -> ToolState {
        let mut tool = ToolState::default();
        tool.select(ToolKind::Wall);
        tool.wall_mode = mode;
        tool
    }

    fn text(s: &str) -> KeyInput {
        KeyInput::Text(s.into())
    }

    /// INP-001: the wall tool takes its points through point input; a chain
    /// of three points ended with Enter creates two walls.
    #[test]
    fn the_wall_tool_draws_a_chain_through_point_input() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Chain);
        for p in [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)] {
            assert_eq!(env.click(&mut input, &mut tool, p), Effect::None);
        }
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Enter), Effect::None);
        assert_eq!(
            env.walls(),
            [
                (DVec2::ZERO, DVec2::new(4.0, 0.0)),
                (DVec2::new(4.0, 0.0), DVec2::new(4.0, 3.0))
            ]
        );
        assert!(tool.points.is_empty(), "Enter ends the chain");
    }

    /// INP-001, INP-003: a move takes its base point, then the typed distance
    /// 2.5 along the cursor direction, and previews the moved outline.
    #[test]
    fn a_move_takes_a_typed_distance_along_the_cursor() {
        let mut env = Env::new();
        let wall = env.project.new_wall(env.storey, DVec2::ZERO, DVec2::new(0.0, 2.0), 0);
        let id = wall.id;
        env.run(Effect::Execute(Command::AddElements(vec![wall])));
        let mut input = PointInput::default();
        let mut stub = TransformStub::new(&env.project, vec![id], Transform::Move);

        env.click(&mut input, &mut stub, (0.0, 0.0));
        let ctx = env.ctx(&stub);
        let resolved = input.resolve(DVec2::new(1.0, 0.0), Modifiers::default(), &ctx);
        assert_eq!(
            stub.preview(&env.project, &resolved).outlines,
            [(vec![DVec2::new(1.0, 0.0), DVec2::new(1.0, 2.0)], false)]
        );
        assert_eq!(input.key(text("2.5"), &mut stub, &ctx), Effect::None);
        let resolved = input.resolve(DVec2::new(1.0, 0.0), Modifiers::default(), &ctx);
        assert_eq!(resolved.snap.kind, SnapKind::Typed);
        assert_eq!(
            stub.preview(&env.project, &resolved).outlines,
            [(vec![DVec2::new(2.5, 0.0), DVec2::new(2.5, 2.0)], false)]
        );
        assert_eq!(
            input.key(KeyInput::Enter, &mut stub, &ctx),
            Effect::Execute(Command::MoveElements {
                ids: vec![id],
                delta: DVec2::new(2.5, 0.0)
            })
        );
        assert!(stub.step().is_none(), "a move ends after its target point");
    }

    /// EDT-001 through INP-001: a copy places one copy; multiple copies go on
    /// until Esc or a double-click, and the copies placed stay.
    #[test]
    fn copies_stay_when_multiple_copy_ends() {
        for end_with_escape in [false, true] {
            let mut env = Env::new();
            let wall = env.project.new_wall(env.storey, DVec2::ZERO, DVec2::new(0.0, 2.0), 0);
            let id = wall.id;
            env.run(Effect::Execute(Command::AddElements(vec![wall])));
            let mut input = PointInput::default();

            let mut copy = TransformStub::new(&env.project, vec![id], Transform::Copy);
            env.click(&mut input, &mut copy, (0.0, 0.0));
            env.click(&mut input, &mut copy, (1.0, 0.0));
            assert!(copy.step().is_none());
            assert_eq!(env.walls().len(), 2);

            let mut multiple = TransformStub::new(&env.project, vec![id], Transform::MultipleCopy);
            env.click(&mut input, &mut multiple, (0.0, 0.0));
            for x in [2.0, 3.0, 4.0] {
                env.click(&mut input, &mut multiple, (x, 0.0));
                assert!(multiple.step().is_some_and(|s| s.repeat));
            }
            if end_with_escape {
                assert_eq!(env.key(&mut input, &mut multiple, KeyInput::Escape), Effect::None);
            } else {
                // A double-click is a second click at the same place first.
                env.click(&mut input, &mut multiple, (4.0, 0.0));
                let ctx = env.ctx(&multiple);
                assert_eq!(input.double_click(&mut multiple, &ctx), Effect::None);
            }
            assert!(multiple.step().is_none());
            let mut starts: Vec<f64> = env.walls().iter().map(|w| w.0.x).collect();
            starts.sort_by(f64::total_cmp);
            assert_eq!(starts, [0.0, 1.0, 2.0, 3.0, 4.0]);
        }
    }

    /// INP-001: for the same cursor, modifiers, typed text and context, the
    /// wall tool's second step and a move's second step resolve identically.
    #[test]
    fn every_consumer_resolves_identically() {
        let mut env = Env::new();
        env.targets.nodes.push(DVec2::new(3.0, 3.0));
        env.targets.edges.push([DVec2::new(0.0, 5.0), DVec2::new(6.0, 5.0)]);
        let mut input = PointInput::default();
        let mut wall = wall_tool(WallMode::Chain);
        let mut stub = TransformStub::new(&env.project, Vec::new(), Transform::Move);
        env.click(&mut input, &mut wall, (1.0, 1.0));
        env.click(&mut input, &mut stub, (1.0, 1.0));
        let (wall_ctx, stub_ctx) = (env.ctx(&wall), env.ctx(&stub));
        assert_eq!(wall_ctx.previous, stub_ctx.previous);
        assert_eq!(wall_ctx.expects, stub_ctx.expects);

        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let cases = [
            ((3.04, 2.97), Modifiers::default(), ""),
            ((2.0, 5.05), Modifiers::default(), ""),
            ((7.3, 1.9), Modifiers::default(), ""),
            ((4.0, 1.4), shift, ""),
            ((4.0, 1.4), Modifiers::default(), "2"),
            ((4.0, 1.4), shift, "abc"),
        ];
        for (cursor, mods, typed) in cases {
            let cursor = DVec2::new(cursor.0, cursor.1);
            let mut a = PointInput::default();
            let mut b = PointInput::default();
            a.key(text(typed), &mut wall, &wall_ctx);
            b.key(text(typed), &mut stub, &stub_ctx);
            assert_eq!(a.typed, b.typed);
            let ra = a.resolve(cursor, mods, &wall_ctx);
            let rb = b.resolve(cursor, mods, &stub_ctx);
            assert_eq!(ra, rb, "{cursor} {mods:?} {typed:?}");
        }
    }

    /// INP-001, NFR-USE-003: every step of every wall and slab method names
    /// the expected input, and previews the result once a point is set.
    #[test]
    fn every_tool_step_has_a_prompt_and_a_preview() {
        let two = vec![(0.0, 0.0), (4.0, 3.0)];
        let three = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)];
        let four = vec![(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 3.0)];
        let mut tools: Vec<(ToolState, Vec<(f64, f64)>)> = WallMode::ALL
            .iter()
            .map(|mode| {
                let points = match mode {
                    WallMode::Single | WallMode::Rectangle => two.clone(),
                    WallMode::RotatedRectangle => three.clone(),
                    WallMode::Chain => four.clone(),
                };
                (wall_tool(*mode), points)
            })
            .collect();
        for mode in SlabMode::ALL {
            let mut tool = ToolState::default();
            tool.select(ToolKind::Slab);
            tool.slab_mode = mode;
            let points = match mode {
                SlabMode::Rectangle => two.clone(),
                SlabMode::RotatedRectangle => three.clone(),
                SlabMode::Polygon => four.clone(),
            };
            tools.push((tool, points));
        }
        for (mut tool, points) in tools {
            let mut env = Env::new();
            let mut input = PointInput::default();
            for (i, p) in points.iter().enumerate() {
                let step = tool.step().unwrap();
                assert!(!step.prompt.is_empty() && !step.status_line().is_empty());
                let ctx = env.ctx(&tool);
                let resolved = input.resolve(DVec2::new(p.0, p.1), Modifiers::default(), &ctx);
                let preview = tool.preview(&env.project, &resolved);
                assert_eq!(preview.outlines.is_empty(), i == 0, "{:?} step {i}", tool.step());
                env.click(&mut input, &mut tool, *p);
            }
            env.key(&mut input, &mut tool, KeyInput::Enter);
            assert!(!env.project.elements.is_empty(), "{:?}", tool.step());
        }
    }

    /// EDT-001 shapes through INP-001: a rotation takes its centre, a
    /// reference direction and the typed angle 30, measured from the
    /// reference direction.
    #[test]
    fn a_rotation_takes_a_typed_angle() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut stub = RotateStub::default();
        env.click(&mut input, &mut stub, (1.0, 1.0));
        assert_eq!(stub.step().unwrap().expects, Expect::Direction { from: DVec2::ONE });
        env.click(&mut input, &mut stub, (1.0, 3.0));
        assert_eq!(stub.step().unwrap().expects, Expect::Angle { centre: DVec2::ONE });
        let ctx = env.ctx(&stub);
        input.resolve(DVec2::new(-2.0, 0.5), Modifiers::default(), &ctx);
        input.key(text("30"), &mut stub, &ctx);
        assert_eq!(input.typed.angle, "30", "an angle step types the angle first");
        assert_eq!(input.key(KeyInput::Enter, &mut stub, &ctx), Effect::None);
        let angle = stub.angle.unwrap().to_degrees();
        assert!((angle - 30.0).abs() < 1e-12, "{angle}");
        assert!(stub.step().is_none());
    }

    /// INP-003: Esc clears typed text first and then cancels one step;
    /// with nothing left it reports that. Backspace removes one character,
    /// then one point.
    #[test]
    fn escape_and_backspace_remove_one_step_at_a_time() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::RotatedRectangle);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.click(&mut input, &mut tool, (4.0, 0.0));
        env.key(&mut input, &mut tool, text("12"));
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Escape), Effect::None);
        assert!(input.typed.is_empty());
        assert_eq!(tool.points.len(), 2);

        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Escape), Effect::None);
        assert_eq!(tool.points, [DVec2::ZERO]);
        assert_eq!(tool.kind, ToolKind::Wall);

        env.key(&mut input, &mut tool, text("12"));
        env.key(&mut input, &mut tool, KeyInput::Backspace);
        assert_eq!(input.typed.distance, "1");
        env.key(&mut input, &mut tool, KeyInput::Backspace);
        assert!(input.typed.is_empty());
        assert_eq!(tool.points.len(), 1, "typed text goes before points");

        // Typed text in the other field also goes first.
        env.key(&mut input, &mut tool, text("3"));
        env.key(&mut input, &mut tool, KeyInput::Tab);
        env.key(&mut input, &mut tool, KeyInput::Backspace);
        assert!(input.typed.is_empty());
        assert_eq!(tool.points.len(), 1);
        env.key(&mut input, &mut tool, KeyInput::Backspace);
        assert!(tool.points.is_empty());
        assert_eq!(
            env.key(&mut input, &mut tool, KeyInput::Escape),
            Effect::NothingToCancel
        );
    }

    /// INP-003, INP-004, INP-006: a typed distance, then a typed angle, from
    /// the previous point; Shift constrains the direction to the angle
    /// increment.
    #[test]
    fn typed_distance_and_constraint() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        let ctx = env.ctx(&tool);
        input.typed.distance = "2500mm".into();
        let p = input.resolve(DVec2::new(1.0, 0.0), Modifiers::default(), &ctx).point;
        assert!(p.distance(DVec2::new(2.5, 0.0)) < 1e-12);
        input.typed.angle = "90".into();
        let p = input.resolve(DVec2::new(1.0, 0.0), Modifiers::default(), &ctx).point;
        assert!(p.distance(DVec2::new(0.0, 2.5)) < 1e-12);
        input.typed.clear();
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        let resolved = input.resolve(DVec2::new(3.0, 0.4), shift, &ctx);
        assert_eq!(resolved.snap.kind, SnapKind::Constrained);
        assert!(resolved.point.distance(DVec2::new(3.0, 0.0)) < 1e-12);
    }

    /// INP-002, INP-003, UI-008: once a step takes typed values, keys belong
    /// to point input and the tracker shows the distance and angle from the
    /// previous point, or the typed value in the active field.
    #[test]
    fn keys_and_tracker_follow_the_step() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        assert!(!input.captures_keys(&tool), "the first point takes no typed values");
        assert_eq!(input.tracker(DVec2::X, &env.ctx(&tool)), None);
        env.click(&mut input, &mut tool, (1.0, 1.0));
        assert!(input.captures_keys(&tool));
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert!(tracker.starts_with("▸d 5"), "{tracker}");
        assert!(tracker.contains(" a 53"), "{tracker}");
        env.key(&mut input, &mut tool, KeyInput::Tab);
        env.key(&mut input, &mut tool, text("30"));
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert!(tracker.ends_with("▸a 30_"), "{tracker}");
        assert!(input.captures_keys(&ToolState::default()), "typed text keeps the keys");
        input.typed.clear();
        assert!(!input.captures_keys(&ToolState::default()));
    }

    /// INP-003: Enter with a typed value that does not parse is an error.
    #[test]
    fn enter_with_an_invalid_typed_value_is_an_error() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, text("1..2"));
        assert!(matches!(
            env.key(&mut input, &mut tool, KeyInput::Enter),
            Effect::Error(_)
        ));
        assert_eq!(tool.points.len(), 1);
    }
}
