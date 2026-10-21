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
mod pointer;
mod snap;
mod surface;
mod targets;
mod tracker;
mod typed;

use archicraft_model::glam::{DVec2, DVec3};
use archicraft_model::tolerance::coincident;
use archicraft_model::units::{NumberFormat, UnitSettings, input_alphabet};
use archicraft_model::{Command, Project, StoreyId};

pub use constrain::constrain;
pub use consumer::{CursorState, Expect, InputConsumer, KeyHint, Outcome, Preview, StepSpec, TrackerMode};
pub use keys::{KeyInput, Modifiers};
pub use pointer::{InputMode, PointerEvent};
pub use snap::{Snap, SnapKind, SnapLevel, candidates, snap};
pub use surface::{SurfaceFrame, ViewSurface};
pub use targets::{IndexStats, SnapIndex, SnapSource, SourceClass, TargetOwner, TargetSink};
pub use tracker::TrackerLine;
pub use typed::{Field, FieldSet, Origin, Reference, TypedInput, Value, starts_value};

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
    /// The snaps the user can choose between at this position, in order of
    /// precedence (INP-009); Tab cycles through them. A typed or
    /// constrained point is the only one, a free point has none.
    pub candidates: Vec<Snap>,
    pub guides: Vec<GuideShape>,
}

/// Everything point input needs to resolve a point on one view's working
/// plane, for the current step of a consumer.
#[derive(Clone, Debug)]
pub struct InputContext<'a> {
    pub project: &'a Project,
    pub storey: StoreyId,
    /// The snap targets of the storey (INP-008).
    pub targets: &'a SnapIndex,
    /// The consumer's points entered so far, snap targets kept out of the
    /// index.
    pub pending: &'a [DVec2],
    /// The consumer's reference point on this storey.
    pub previous: Option<DVec2>,
    /// The guides shown, snap targets of the guide levels (INP-009).
    pub guides: &'a [GuideShape],
    /// What the consumer's current step expects.
    pub expects: Expect,
    /// How the consumer's current step reports and takes values (INP-002).
    pub tracker: TrackerMode,
    /// The tracker adds the absolute X, Y and Z (INP-002, the tracker
    /// preference of SET-001).
    pub tracker_xyz: bool,
    /// The snap radius in model units: the radius in pixels times the
    /// surface's model length per pixel.
    pub radius: f64,
    /// How far the pointer moves before a press becomes a drag, in model
    /// units: the click distance in pixels times the surface's model length
    /// per pixel (INP-020).
    pub drag_threshold: f64,
    pub units: &'a UnitSettings,
    pub number_format: &'a NumberFormat,
    pub angle_increment_deg: f64,
    pub snap_step: f64,
    pub frame: SurfaceFrame,
    /// The user origin on the working plane (INP-014): the project origin
    /// until the user origin can be placed (archicraft-k2uv).
    pub user_origin: DVec2,
    /// The temporarily marked snap point absolute coordinates can be
    /// measured from (INP-004); none until points can be marked.
    pub marked: Option<DVec2>,
}

impl InputContext<'_> {
    /// The point of the step that relative input starts from.
    fn origin(&self) -> Option<DVec2> {
        self.expects.origin(self.previous)
    }

    /// The point typed coordinates are measured from (INP-004).
    fn base(&self, reference: Reference) -> Option<DVec2> {
        match reference {
            Reference::Relative => self.origin(),
            Reference::Absolute(Origin::Project) => Some(self.frame.to_plane(DVec3::ZERO)),
            Reference::Absolute(Origin::User) => Some(self.user_origin),
            Reference::Absolute(Origin::Marked) => self.marked,
        }
    }

    /// The direction typed angles are measured from: the reference
    /// direction in a relative angle step, otherwise the X axis.
    fn base_angle(&self, reference: Reference) -> f64 {
        match (reference, self.expects) {
            (Reference::Relative, Expect::Angle { centre }) => self
                .previous
                .filter(|p| !coincident(*p, centre))
                .map_or(0.0, |p| (p - centre).to_angle()),
            _ => 0.0,
        }
    }

    /// The typed fields this step starts with (INP-004).
    fn follow(&self, typed: &mut TypedInput) {
        typed.follow_step(
            self.origin().is_none(),
            matches!(self.expects, Expect::Angle { .. }),
            self.tracker,
        );
    }
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

/// The state of point input between events: the typed values, the last
/// cursor position and the snap candidate chosen with Tab (INP-001,
/// INP-003, INP-009).
#[derive(Clone, Debug, Default)]
pub struct PointInput {
    pub typed: TypedInput,
    cursor: Option<(DVec2, Modifiers)>,
    last: Option<Resolved>,
    /// Where cycling through the snap candidates started and the index of
    /// the chosen candidate, like the selection's pick cycling (SEL-001).
    cycle: Option<(DVec2, usize)>,
    /// Where the press of a press-drag-release gesture took its point.
    pressed: Option<DVec2>,
}

impl PointInput {
    /// The point resolved for the last cursor position.
    pub fn last(&self) -> Option<&Resolved> {
        self.last.as_ref()
    }

    /// Whether keys belong to point input rather than to single-key
    /// shortcuts: while the consumer's step has a point to start from or a
    /// value is typed (INP-003, UI-008). Before that, only a key that
    /// starts a value ([`PointInput::starts_typing`]) goes to point input.
    pub fn captures_keys(&self, consumer: &dyn InputConsumer) -> bool {
        !self.typed.is_empty()
            || consumer
                .step()
                .is_some_and(|s| s.expects.origin(consumer.reference_point()).is_some())
    }

    /// Whether `text` starts typing a value in the consumer's current step
    /// (INP-003): a digit, a sign or the decimal separator, also for the
    /// first point. The Select tool has no step and never types.
    pub fn starts_typing(&self, consumer: &dyn InputConsumer, text: &str, nf: &NumberFormat) -> bool {
        consumer.step().is_some() && text.chars().next().is_some_and(|c| starts_value(c, nf))
    }

    /// The tracker text next to the cursor (INP-002): one line per field
    /// of the active field set with the value of `point`, or the typed text
    /// with the active field marked; a stretch measures from its fixed end.
    pub fn tracker(&self, point: DVec2, ctx: &InputContext) -> Option<String> {
        let mut typed = self.typed.clone();
        ctx.follow(&mut typed);
        let base = match (&typed.set, ctx.tracker.stretch()) {
            (FieldSet::Stretch { .. }, Some((base, _, _))) => base,
            _ => ctx.base(typed.reference)?,
        };
        let lines = tracker::lines(&typed, base, ctx.base_angle(typed.reference), point, ctx);
        Some(tracker::text(&lines))
    }

    /// Whether Tab cycles through the snap candidates rather than moving to
    /// the next typed field (INP-009): while two or more candidates are in
    /// range and nothing is typed. Typed text in either field makes the
    /// typed point the only candidate; checking the text directly also
    /// covers a Tab that follows a typed key before the next resolve.
    pub fn cycles(&self) -> bool {
        self.typed.is_empty() && self.last.as_ref().is_some_and(|r| r.candidates.len() >= 2)
    }

    /// Resolves a cursor position on the working plane: typed values first,
    /// then the angle constraint while Shift is held, then snapping to the
    /// candidate chosen with Tab, the first one unless the user cycled
    /// (INP-003, INP-006, INP-009). Leaving the snap radius around the
    /// position where cycling started ends the cycle. In a stretch every
    /// point that is not typed is projected onto the stretch line (INP-002).
    pub fn resolve(&mut self, cursor: DVec2, mods: Modifiers, ctx: &InputContext) -> Resolved {
        ctx.follow(&mut self.typed);
        self.cursor = Some((cursor, mods));
        let origin = ctx.origin();
        let typed = (!self.typed.is_empty())
            .then(|| self.apply_typed(cursor, ctx))
            .flatten()
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
        let candidates = match typed.or_else(constrained) {
            Some(s) => vec![s],
            None => candidates(
                ctx.targets,
                ctx.pending,
                cursor,
                ctx.radius,
                ctx.snap_step,
                ctx.previous,
                ctx.guides,
            ),
        };
        let stretch = ctx
            .tracker
            .stretch()
            .filter(|_| matches!(self.typed.set, FieldSet::Stretch { .. }));
        let onto_stretch = |s: Snap| match stretch {
            Some((base, dir, _)) if s.kind != SnapKind::Typed => Snap {
                point: base + dir * (s.point - base).dot(dir),
                ..s
            },
            _ => s,
        };
        let candidates: Vec<Snap> = candidates.into_iter().map(onto_stretch).collect();
        let chosen = match self.cycle {
            Some((start, i)) if start.distance(cursor) <= ctx.radius => i,
            _ => {
                self.cycle = None;
                0
            }
        };
        let s = candidates
            .get(chosen % candidates.len().max(1))
            .copied()
            .unwrap_or_else(|| {
                onto_stretch(Snap {
                    point: cursor,
                    kind: SnapKind::Free,
                })
            });
        let resolved = Resolved {
            point: s.point,
            snap: s,
            candidates,
            guides: Vec::new(),
        };
        self.last = Some(resolved.clone());
        resolved
    }

    /// The typed values applied from their base point (INP-002, INP-003,
    /// INP-004): an empty field takes its value from the cursor (a typed ΔX
    /// with the cursor's ΔY, a typed distance with the cursor's angle). A
    /// stretch takes the typed total length or length change along its
    /// direction, the active field first. `None` when a value does not
    /// parse or the base point is unknown.
    fn apply_typed(&self, cursor: DVec2, ctx: &InputContext) -> Option<DVec2> {
        let typed = &self.typed;
        let value = |field| typed.value(field, ctx.units, ctx.number_format).transpose().ok();
        let base = ctx.base(typed.reference);
        match typed.set {
            FieldSet::Polar { .. } => {
                let (base, offset) = base.map(|b| (b, cursor - b))?;
                let angle = value(Field::Angle)?.map_or(offset.to_angle(), |a| ctx.base_angle(typed.reference) + a);
                let distance = value(Field::Distance)?.unwrap_or(offset.length());
                Some(base + DVec2::from_angle(angle) * distance)
            }
            FieldSet::Cartesian { .. } => {
                let (base, offset) = base.map(|b| (b, cursor - b))?;
                // A plan view's working plane is the storey plane: planar
                // consumers ignore ΔZ until the 3D working plane exists.
                value(Field::Z)?;
                let x = value(Field::X)?.unwrap_or(offset.x);
                let y = value(Field::Y)?.unwrap_or(offset.y);
                Some(base + DVec2::new(x, y))
            }
            FieldSet::Stretch { .. } => {
                let (base, dir, length) = ctx.tracker.stretch()?;
                let total = value(Field::Total)?;
                let changed = value(Field::Change)?.map(|change| length + change);
                let total = if typed.field == Field::Change {
                    changed.or(total)
                } else {
                    total.or(changed)
                };
                total.map(|total| base + dir * total)
            }
        }
    }

    /// Handles a key (INP-003, INP-004): characters go to the active field,
    /// Tab cycles through the snap candidates in range or else moves between
    /// fields (INP-009), P and Shift+P switch the field set and the
    /// reference, Enter commits the typed point or finishes a repeating
    /// step, Backspace removes a character and then a point, and Esc clears
    /// the typed text and then cancels one step.
    pub fn key(&mut self, key: KeyInput, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        ctx.follow(&mut self.typed);
        let origin = ctx.origin();
        let step = consumer.step().is_some();
        match key {
            KeyInput::Text(text) if step => {
                if !self.typed.is_empty()
                    || origin.is_some()
                    || text.chars().next().is_some_and(|c| starts_value(c, ctx.number_format))
                {
                    // NFR-L10N-004: the characters the parsers accept under
                    // the active number format, not a fixed list.
                    let alphabet = input_alphabet(ctx.number_format);
                    let text: String = text.chars().filter(|c| alphabet.contains(*c)).collect();
                    self.typed.push_str(&text);
                }
                Effect::None
            }
            KeyInput::Tab if self.cycles() => {
                if let Some((cursor, _)) = self.cursor {
                    self.cycle = Some(match self.cycle {
                        Some((start, i)) => (start, i + 1),
                        None => (cursor, 1),
                    });
                }
                Effect::None
            }
            KeyInput::Tab if step => {
                self.typed.next_field();
                Effect::None
            }
            KeyInput::SwitchFields if step => {
                let reference = self.typed.reference;
                let cursor = self.cursor.zip(ctx.base(reference)).map(|((c, _), base)| c - base);
                self.typed
                    .switch_set(ctx.base_angle(reference), cursor, ctx.units, ctx.number_format);
                Effect::None
            }
            // The first point has nothing to be relative to.
            KeyInput::SwitchReference if step && origin.is_some() => {
                self.typed.switch_reference();
                Effect::None
            }
            KeyInput::Enter if step => {
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
                if !self.typed.pop() {
                    consumer.backspace();
                }
                Effect::None
            }
            KeyInput::Escape => {
                let typed = !self.typed.is_empty();
                self.typed.clear();
                if typed || consumer.cancel_step() {
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

    /// Handles a primary-button event in `mode` (INP-020). Click-click takes
    /// a point per click. Press-drag-release takes a point on the press and
    /// the next one on the release when the pointer moved beyond the drag
    /// threshold; a press and release without that movement takes one point,
    /// like a click. A double-click finishes a repeating step in both modes.
    pub fn pointer(
        &mut self,
        event: PointerEvent,
        mode: InputMode,
        cursor: DVec2,
        mods: Modifiers,
        consumer: &mut dyn InputConsumer,
        ctx: &InputContext,
    ) -> Effect {
        match (mode, event) {
            (_, PointerEvent::DoubleClicked) => {
                self.pressed = None;
                self.double_click(consumer, ctx)
            }
            (_, PointerEvent::Clicked) => self.click(cursor, mods, consumer, ctx),
            (InputMode::DragRelease, PointerEvent::Pressed) => {
                self.pressed = consumer.step().is_some().then_some(cursor);
                self.click(cursor, mods, consumer, ctx)
            }
            (InputMode::DragRelease, PointerEvent::Released) => match self.pressed.take() {
                Some(press) if press.distance(cursor) > ctx.drag_threshold => self.click(cursor, mods, consumer, ctx),
                _ => Effect::None,
            },
            (InputMode::ClickClick, PointerEvent::Pressed | PointerEvent::Released) => Effect::None,
        }
    }

    /// Finishes a repeating step (a chain, a polygon, multiple copies).
    pub fn double_click(&mut self, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        self.typed.clear();
        consumer.finish(ctx.project).into()
    }

    fn commit(&mut self, point: DVec2, consumer: &mut dyn InputConsumer, ctx: &InputContext) -> Effect {
        self.typed.clear();
        self.cycle = None;
        consumer.commit(ctx.project, ctx.storey, point).into()
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;
    use archicraft_model::{Element, ElementId, History};

    use super::*;
    use crate::input::targets::tests::index_of;
    use crate::tools::{SlabMode, ToolKind, ToolState, WallMode};

    /// The model length of one screen pixel in [`Env`].
    const PIXEL: f64 = 0.01;

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

    /// A stretch-shaped consumer (INP-002): moves the end of a 4 m long
    /// element from (1, 1) along X, in the tracker mode `mode` opens with.
    struct StretchStub {
        change: bool,
        end: Option<DVec2>,
    }

    impl StretchStub {
        const BASE: DVec2 = DVec2::new(1.0, 1.0);
        const LENGTH: f64 = 4.0;

        fn new(change: bool) -> Self {
            Self { change, end: None }
        }
    }

    impl InputConsumer for StretchStub {
        fn step(&self) -> Option<StepSpec> {
            let (base, dir, length) = (Self::BASE, DVec2::X, Self::LENGTH);
            self.end.is_none().then(|| StepSpec {
                tracker: if self.change {
                    TrackerMode::LengthChange { base, dir, length }
                } else {
                    TrackerMode::TotalLength { base, dir, length }
                },
                ..StepSpec::new(
                    "Stretch: click the new end or type the length",
                    Expect::Distance { from: base },
                )
            })
        }

        fn reference_point(&self) -> Option<DVec2> {
            Some(Self::BASE)
        }

        fn preview(&self, _project: &Project, resolved: &Resolved) -> Preview {
            Preview {
                outlines: vec![(vec![Self::BASE, resolved.point], false)],
                cursor: CursorState::Normal,
            }
        }

        fn commit(&mut self, _project: &Project, _storey: StoreyId, point: DVec2) -> Outcome {
            self.end = Some(point);
            Outcome::Done
        }

        fn cancel_step(&mut self) -> bool {
            false
        }

        fn backspace(&mut self) -> bool {
            false
        }

        fn finish(&mut self, _project: &Project) -> Outcome {
            Outcome::Continue
        }
    }

    struct Env {
        project: Project,
        storey: StoreyId,
        targets: SnapIndex,
        pending: Vec<DVec2>,
        units: UnitSettings,
        nf: NumberFormat,
        elevation: f64,
        tracker_xyz: bool,
    }

    impl Env {
        fn new() -> Self {
            let project = default_metric_project();
            let storey = project.storeys.ground();
            Self {
                project,
                storey,
                targets: SnapIndex::default(),
                pending: Vec::new(),
                units: UnitSettings::default(),
                nf: NumberFormat::default(),
                elevation: 0.0,
                tracker_xyz: false,
            }
        }

        /// The context of `consumer`'s current step, as a view builds it.
        fn ctx(&self, consumer: &dyn InputConsumer) -> InputContext<'_> {
            InputContext {
                project: &self.project,
                storey: self.storey,
                targets: &self.targets,
                pending: &self.pending,
                previous: consumer.reference_point(),
                guides: &[],
                expects: consumer.step().map_or(Expect::Point, |s| s.expects),
                tracker: consumer.step().map_or(TrackerMode::Relative, |s| s.tracker),
                tracker_xyz: self.tracker_xyz,
                radius: 0.1,
                drag_threshold: 6.0 * PIXEL,
                units: &self.units,
                number_format: &self.nf,
                angle_increment_deg: 45.0,
                snap_step: 0.0,
                frame: SurfaceFrame::plan(self.elevation),
                user_origin: DVec2::ZERO,
                marked: None,
            }
        }

        fn click(&mut self, input: &mut PointInput, consumer: &mut dyn InputConsumer, at: (f64, f64)) -> Effect {
            let ctx = self.ctx(consumer);
            let effect = input.click(DVec2::new(at.0, at.1), Modifiers::default(), consumer, &ctx);
            self.run(effect)
        }

        fn pointer(
            &mut self,
            input: &mut PointInput,
            consumer: &mut dyn InputConsumer,
            event: PointerEvent,
            mode: InputMode,
            at: (f64, f64),
        ) -> Effect {
            let ctx = self.ctx(consumer);
            let effect = input.pointer(
                event,
                mode,
                DVec2::new(at.0, at.1),
                Modifiers::default(),
                consumer,
                &ctx,
            );
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

    /// INP-020: in press-drag-release mode a press takes the first point and
    /// the release 20 px away the next; a single wall is created by one drag.
    #[test]
    fn a_press_drag_release_takes_two_points() {
        use PointerEvent::{Pressed, Released};
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Chain);
        let drag = InputMode::DragRelease;
        assert_eq!(
            env.pointer(&mut input, &mut tool, Pressed, drag, (1.0, 1.0)),
            Effect::None
        );
        assert_eq!(tool.points, [DVec2::ONE], "the press takes the first point");
        env.pointer(&mut input, &mut tool, Released, drag, (1.0 + 20.0 * PIXEL, 1.0));
        assert_eq!(
            tool.points,
            [DVec2::ONE, DVec2::new(1.2, 1.0)],
            "the release takes the next"
        );
        assert!(env.walls().is_empty(), "the chain waits for its end");

        let mut tool = wall_tool(WallMode::Single);
        env.pointer(&mut input, &mut tool, Pressed, drag, (0.0, 0.0));
        env.pointer(&mut input, &mut tool, Released, drag, (0.0, 20.0 * PIXEL));
        assert_eq!(env.walls(), [(DVec2::ZERO, DVec2::new(0.0, 0.2))]);
        assert!(tool.points.is_empty());
    }

    /// INP-020: a press and release without movement beyond the drag
    /// threshold takes one point, exactly as a click does; so does a click
    /// in press-drag-release mode.
    #[test]
    fn a_press_and_release_without_movement_is_a_click() {
        use PointerEvent::{Clicked, Pressed, Released};
        let drag = InputMode::DragRelease;
        let gestures: [&[(PointerEvent, (f64, f64))]; 3] = [
            &[(Pressed, (1.0, 1.0)), (Released, (1.0, 1.0))],
            &[(Pressed, (1.0, 1.0)), (Released, (1.0 + 5.0 * PIXEL, 1.0))],
            &[(Clicked, (1.0, 1.0))],
        ];
        for gesture in gestures {
            for mode in [WallMode::Single, WallMode::Chain, WallMode::Rectangle] {
                let mut clicked = (Env::new(), PointInput::default(), wall_tool(mode));
                let mut dragged = (Env::new(), PointInput::default(), wall_tool(mode));
                for p in [(0.0, 0.0), (1.0, 1.0)] {
                    clicked.0.click(&mut clicked.1, &mut clicked.2, p);
                }
                dragged
                    .0
                    .pointer(&mut dragged.1, &mut dragged.2, Clicked, drag, (0.0, 0.0));
                for &(event, at) in gesture {
                    dragged.0.pointer(&mut dragged.1, &mut dragged.2, event, drag, at);
                }
                assert_eq!(dragged.2.points, clicked.2.points, "{mode:?} {gesture:?}");
                assert_eq!(dragged.0.walls(), clicked.0.walls(), "{mode:?} {gesture:?}");
            }
        }
    }

    /// INP-020: click-click stays the default and behaves as before: each
    /// click takes a point, a double-click ends the chain, presses and
    /// releases take nothing.
    #[test]
    fn click_click_is_the_default_and_unchanged() {
        use PointerEvent::{Clicked, DoubleClicked, Pressed, Released};
        assert_eq!(InputMode::default(), InputMode::ClickClick);
        let mode = InputMode::ClickClick;
        let points = [(0.0, 0.0), (4.0, 0.0), (4.0, 3.0)];
        let mut before = (Env::new(), PointInput::default(), wall_tool(WallMode::Chain));
        for p in points {
            before.0.click(&mut before.1, &mut before.2, p);
        }
        let ctx = before.0.ctx(&before.2);
        let effect = before.1.double_click(&mut before.2, &ctx);
        before.0.run(effect);

        let mut now = (Env::new(), PointInput::default(), wall_tool(WallMode::Chain));
        for p in points {
            now.0.pointer(&mut now.1, &mut now.2, Pressed, mode, p);
            assert_eq!(
                now.0.pointer(&mut now.1, &mut now.2, Released, mode, (9.0, 9.0)),
                Effect::None
            );
            now.0.pointer(&mut now.1, &mut now.2, Clicked, mode, p);
        }
        assert_eq!(now.2.points.len(), 3, "presses and releases take no point");
        now.0.pointer(&mut now.1, &mut now.2, DoubleClicked, mode, (4.0, 3.0));
        assert_eq!(now.0.walls(), before.0.walls());
        assert_eq!(now.0.walls().len(), 2);
        assert!(now.2.points.is_empty());
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
        env.targets = index_of(&[DVec2::new(3.0, 3.0)], &[[DVec2::new(0.0, 5.0), DVec2::new(6.0, 5.0)]]);
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
        assert_eq!(
            input.typed.text(Field::Angle),
            "30",
            "an angle step types the angle first"
        );
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
        assert_eq!(input.typed.text(Field::Distance), "1");
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
        input.key(text("2500mm"), &mut tool, &ctx);
        let p = input.resolve(DVec2::new(1.0, 0.0), Modifiers::default(), &ctx).point;
        assert!(p.distance(DVec2::new(2.5, 0.0)) < 1e-12);
        input.key(KeyInput::Tab, &mut tool, &ctx);
        input.key(text("90"), &mut tool, &ctx);
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

    /// INP-002, INP-003, UI-008: once a step has a point to start from,
    /// keys belong to point input and the tracker shows the distance and
    /// angle from the previous point, or the typed value in the active field.
    /// Before the first point it shows the absolute X and Y.
    #[test]
    fn keys_and_tracker_follow_the_step() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        assert!(
            !input.captures_keys(&tool),
            "only a value starts typing the first point"
        );
        let tracker = input.tracker(DVec2::new(2.0, -1.5), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, "▸X 2.000 m\n Y -1.500 m\n Z 0.000 m");
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

    /// Resolves `cursor` for `consumer`'s current step.
    fn resolve_at(env: &Env, input: &mut PointInput, consumer: &dyn InputConsumer, cursor: DVec2) -> Resolved {
        input.resolve(cursor, Modifiers::default(), &env.ctx(consumer))
    }

    /// INP-009: with two candidates in range Tab selects the second, which a
    /// click then takes, and the next Tab wraps to the first; moving within
    /// the radius keeps the choice, leaving it starts over.
    #[test]
    fn tab_cycles_through_the_candidates_in_range() {
        let mut env = Env::new();
        let (near, far) = (DVec2::new(1.0, 0.0), DVec2::new(1.05, 0.0));
        env.targets = index_of(&[near, far], &[]);
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        let cursor = DVec2::new(1.02, 0.0);
        let resolved = resolve_at(&env, &mut input, &tool, cursor);
        assert_eq!(
            resolved.candidates.iter().map(|s| s.point).collect::<Vec<_>>(),
            [near, far]
        );
        assert_eq!(resolved.point, near);
        assert!(input.cycles());

        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Tab), Effect::None);
        assert_eq!(resolve_at(&env, &mut input, &tool, cursor).point, far);
        assert_eq!(
            resolve_at(&env, &mut input, &tool, cursor + DVec2::new(0.0, 0.05)).point,
            far
        );
        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(resolve_at(&env, &mut input, &tool, cursor).point, near, "wraps");

        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(resolve_at(&env, &mut input, &tool, cursor).point, far);
        let away = resolve_at(&env, &mut input, &tool, DVec2::new(5.0, 5.0));
        assert_eq!((away.snap.kind, away.candidates.len()), (SnapKind::Free, 0));
        assert_eq!(
            resolve_at(&env, &mut input, &tool, cursor).point,
            near,
            "leaving the radius ends the cycle"
        );

        env.key(&mut input, &mut tool, KeyInput::Tab);
        resolve_at(&env, &mut input, &tool, cursor);
        env.click(&mut input, &mut tool, (cursor.x, cursor.y));
        assert_eq!(tool.points, [far], "a click takes the chosen candidate");
        assert_eq!(input.typed, TypedInput::default(), "Tab typed nothing");
    }

    /// INP-003, INP-009: with typed text present, or with a single candidate
    /// in range, Tab moves to the next typed field instead.
    #[test]
    fn tab_moves_to_the_next_field_without_two_candidates() {
        let mut env = Env::new();
        env.targets = index_of(
            &[DVec2::new(1.0, 0.0), DVec2::new(1.05, 0.0), DVec2::new(4.0, 0.0)],
            &[],
        );
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 3.0));

        let single = resolve_at(&env, &mut input, &tool, DVec2::new(4.02, 0.0));
        assert_eq!(single.candidates.len(), 1);
        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(input.typed.field, Field::Angle);
        assert_eq!(
            resolve_at(&env, &mut input, &tool, DVec2::new(4.02, 0.0)).point,
            DVec2::new(4.0, 0.0)
        );
        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(input.typed.field, Field::Distance);

        let cursor = DVec2::new(1.02, 0.0);
        assert_eq!(resolve_at(&env, &mut input, &tool, cursor).candidates.len(), 2);
        env.key(&mut input, &mut tool, text("2"));
        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(input.typed.field, Field::Angle, "typed text: Tab moves on");
        env.key(&mut input, &mut tool, KeyInput::Tab);
        assert_eq!(input.typed.field, Field::Distance, "the other field holds text");
        let typed = resolve_at(&env, &mut input, &tool, cursor);
        assert_eq!((typed.snap.kind, typed.candidates.len()), (SnapKind::Typed, 1));
    }

    /// NFR-L10N-004, INP-003: typed keys pass through the input alphabet of
    /// the active number format: under a decimal comma "2,5" is typed and
    /// draws a 2.5 m wall, while under a point without grouping a comma is
    /// dropped; letters outside the unit suffixes are dropped in both.
    #[test]
    fn typed_keys_follow_the_number_format() {
        let mut env = Env::new();
        env.nf = NumberFormat {
            decimal_separator: ',',
            grouping: None,
        };
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, text("2,5xq"));
        assert_eq!(input.typed.text(Field::Distance), "2,5");
        input.cursor = Some((DVec2::X, Modifiers::default()));
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Enter), Effect::None);
        assert_eq!(env.walls(), [(DVec2::ZERO, DVec2::new(2.5, 0.0))]);

        env.nf = NumberFormat::default();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, text("1,5.5 mz"));
        assert_eq!(input.typed.text(Field::Distance), "15.5 m");
    }

    /// The point `input` resolves for `cursor` in `consumer`'s step.
    fn point_at(env: &Env, input: &mut PointInput, consumer: &dyn InputConsumer, cursor: (f64, f64)) -> DVec2 {
        let ctx = env.ctx(consumer);
        input
            .resolve(DVec2::new(cursor.0, cursor.1), Modifiers::default(), &ctx)
            .point
    }

    /// INP-004: P switches to Cartesian fields; ΔX 3 and ΔY 4 from (1, 1)
    /// give (4, 5), and a ΔX typed alone takes ΔY from the cursor.
    #[test]
    fn cartesian_fields_are_relative_to_the_previous_point() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (1.0, 1.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        assert!(matches!(input.typed.set, FieldSet::Cartesian { .. }));
        assert_eq!(input.typed.reference, Reference::Relative);
        env.key(&mut input, &mut tool, text("3"));
        assert_eq!(point_at(&env, &mut input, &tool, (0.0, 7.0)), DVec2::new(4.0, 7.0));
        env.key(&mut input, &mut tool, KeyInput::Tab);
        env.key(&mut input, &mut tool, text("4"));
        assert_eq!(point_at(&env, &mut input, &tool, (0.0, 7.0)), DVec2::new(4.0, 5.0));
        env.key(&mut input, &mut tool, KeyInput::Enter);
        assert_eq!(env.walls(), [(DVec2::ONE, DVec2::new(4.0, 5.0))]);
    }

    /// INP-004, AS-04: P converts typed polar values into Cartesian fields
    /// without rounding: the fields show rounded text and keep the exact
    /// value, so the point does not move.
    #[test]
    fn switching_to_cartesian_keeps_typed_values_exact() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, text("1"));
        env.key(&mut input, &mut tool, KeyInput::Tab);
        env.key(&mut input, &mut tool, text("30"));
        let polar = point_at(&env, &mut input, &tool, (5.0, 5.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        assert_eq!(input.typed.text(Field::X), "0.866");
        assert_eq!(input.typed.text(Field::Y), "0.500");
        let exact = DVec2::from_angle(30f64.to_radians());
        let value = |f| input.typed.value(f, &env.units, &env.nf).unwrap().unwrap();
        assert_eq!((value(Field::X), value(Field::Y)), (exact.x, exact.y));
        assert_eq!(point_at(&env, &mut input, &tool, (5.0, 5.0)), polar);
        // Editing a converted field reads its text again.
        env.key(&mut input, &mut tool, KeyInput::Backspace);
        env.key(&mut input, &mut tool, text("2"));
        assert_eq!(input.typed.value(Field::X, &env.units, &env.nf), Some(Ok(0.862)));
    }

    /// INP-003, INP-004: before the first point the fields are absolute X
    /// and Y from the project origin, measured in the storey plane; '3',
    /// Tab, '4', Enter commits the first point (3, 4). No snap candidate is
    /// in range, so Tab moves the field.
    #[test]
    fn the_first_point_takes_absolute_coordinates() {
        let mut env = Env::new();
        env.elevation = 2.8;
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Chain);
        for key in [text("3"), KeyInput::Tab, text("4")] {
            assert_eq!(env.key(&mut input, &mut tool, key), Effect::None);
        }
        assert_eq!(input.typed.reference, Reference::Absolute(Origin::Project));
        let point = point_at(&env, &mut input, &tool, (-7.0, 9.0));
        assert_eq!(point, DVec2::new(3.0, 4.0));
        assert_eq!(env.ctx(&tool).frame.to_model(point).z, 2.8);
        assert_eq!(env.key(&mut input, &mut tool, KeyInput::Enter), Effect::None);
        assert_eq!(tool.points, [DVec2::new(3.0, 4.0)]);
        assert!(input.typed.is_empty());
        // The next point is relative again: polar from the first point.
        assert!(matches!(input.typed.set, FieldSet::Cartesian { .. }));
        env.key(&mut input, &mut tool, text("2"));
        assert!(matches!(input.typed.set, FieldSet::Polar { .. }));
        assert_eq!(point_at(&env, &mut input, &tool, (10.0, 4.0)), DVec2::new(5.0, 4.0));
    }

    /// INP-003, UI-008: before the first point only a digit, a sign or the
    /// decimal separator starts typing; other keys stay shortcuts. Once
    /// typing started, unit letters belong to the value.
    #[test]
    fn only_a_value_starts_typing_the_first_point() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Chain);
        for start in ["3", "-", "+", "−", ".", "3ft"] {
            assert!(input.starts_typing(&tool, start, &env.nf), "{start}");
        }
        for other in ["s", "f", "P", " ", ""] {
            assert!(!input.starts_typing(&tool, other, &env.nf), "{other:?}");
            env.key(&mut input, &mut tool, text(other));
            assert!(input.typed.is_empty(), "{other:?}");
        }
        assert!(
            !input.starts_typing(&ToolState::default(), "3", &env.nf),
            "Select never types"
        );
        env.key(&mut input, &mut tool, text("3"));
        env.key(&mut input, &mut tool, text("f"));
        env.key(&mut input, &mut tool, text("t"));
        assert_eq!(input.typed.text(Field::X), "3ft");
        assert!(input.captures_keys(&tool));
    }

    /// INP-004: Shift+P toggles relative and absolute coordinates once the
    /// step has a point to be relative to; the typed text is read in the
    /// new reference.
    #[test]
    fn shift_p_toggles_relative_and_absolute() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.key(&mut input, &mut tool, KeyInput::SwitchReference);
        assert_eq!(
            input.typed.reference,
            Reference::Absolute(Origin::Project),
            "the first point has nothing to be relative to"
        );
        env.click(&mut input, &mut tool, (1.0, 1.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        for key in [text("3"), KeyInput::Tab, text("4")] {
            env.key(&mut input, &mut tool, key);
        }
        assert_eq!(input.typed.reference, Reference::Relative);
        assert_eq!(point_at(&env, &mut input, &tool, (0.0, 0.0)), DVec2::new(4.0, 5.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchReference);
        assert_eq!(input.typed.reference, Reference::Absolute(Origin::Project));
        assert_eq!(point_at(&env, &mut input, &tool, (0.0, 0.0)), DVec2::new(3.0, 4.0));
        let tracker = input.tracker(DVec2::new(3.0, 4.0), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, " X 3_\n▸Y 4_\n Z 0.000 m");
        env.key(&mut input, &mut tool, KeyInput::SwitchReference);
        assert_eq!(point_at(&env, &mut input, &tool, (0.0, 0.0)), DVec2::new(4.0, 5.0));
    }

    /// INP-002, INP-004: the Cartesian set shows ΔX, ΔY and ΔZ (0 in plan
    /// views); a typed ΔZ leaves a plan-view point in the storey plane.
    #[test]
    fn cartesian_tracker_and_planar_dz() {
        let mut env = Env::new();
        env.elevation = 3.0;
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (1.0, 1.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, "▸ΔX 3.000 m\n ΔY 4.000 m\n ΔZ 0.000 m");
        for key in [text("3"), KeyInput::Tab, KeyInput::Tab, text("2")] {
            env.key(&mut input, &mut tool, key);
        }
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, " ΔX 3_\n ΔY 4.000 m\n▸ΔZ 2_");
        let point = point_at(&env, &mut input, &tool, (2.0, 5.0));
        assert_eq!(point, DVec2::new(4.0, 5.0), "ΔZ does not move the point in plan");
        assert_eq!(env.ctx(&tool).frame.to_model(point).z, 3.0);
    }

    /// INP-005: an imperial length typed into the ΔX field of a metric
    /// project resolves to metres at full precision.
    #[test]
    fn imperial_lengths_in_the_dx_field() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        env.key(&mut input, &mut tool, text("3'-4 1/2\""));
        assert_eq!(input.typed.text(Field::X), "3'-4 1/2\"");
        let point = point_at(&env, &mut input, &tool, (0.0, 0.0));
        assert!((point.x - 1.0287).abs() < 1e-12, "{point}");
        assert_eq!(point.y, 0.0);
    }

    /// INP-003, INP-004: Backspace removing the last point brings back the
    /// first point's absolute X and Y, also after a key chose the fields of
    /// the later step.
    #[test]
    fn backspace_to_the_first_point_restores_absolute_input() {
        for choose in [KeyInput::Tab, KeyInput::SwitchFields, KeyInput::SwitchReference] {
            let mut env = Env::new();
            let mut input = PointInput::default();
            let mut tool = wall_tool(WallMode::Chain);
            env.click(&mut input, &mut tool, (1.0, 1.0));
            env.key(&mut input, &mut tool, choose.clone());
            env.key(&mut input, &mut tool, KeyInput::Backspace);
            assert!(tool.points.is_empty());
            for key in [text("3"), KeyInput::Tab, text("4"), KeyInput::Enter] {
                assert_eq!(env.key(&mut input, &mut tool, key), Effect::None, "{choose:?}");
            }
            assert_eq!(tool.points, [DVec2::new(3.0, 4.0)], "{choose:?}");
        }
    }

    /// INP-004: P keeps a lone typed value; the untyped field comes from the
    /// cursor, as in the resolved point, so the point does not move.
    #[test]
    fn switching_fields_completes_a_lone_value_from_the_cursor() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.click(&mut input, &mut tool, (0.0, 0.0));
        env.key(&mut input, &mut tool, text("10"));
        let polar = point_at(&env, &mut input, &tool, (3.0, 4.0));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        assert_eq!(
            (input.typed.text(Field::X), input.typed.text(Field::Y)),
            ("6.000", "8.000")
        );
        assert_eq!(point_at(&env, &mut input, &tool, (3.0, 4.0)), polar);
        // Back to polar: ΔX alone takes ΔY from the cursor.
        env.key(&mut input, &mut tool, KeyInput::Tab);
        for _ in 0..5 {
            env.key(&mut input, &mut tool, KeyInput::Backspace);
        }
        assert_eq!((input.typed.text(Field::X), input.typed.text(Field::Y)), ("6.000", ""));
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        let d = input
            .typed
            .value(Field::Distance, &env.units, &env.nf)
            .unwrap()
            .unwrap();
        assert!((d - 6.0f64.hypot(4.0)).abs() < 1e-12, "{d}");
    }

    /// INP-002, SET-001: with the X/Y/Z preference on, the tracker adds the
    /// absolute X, Y and Z of the point in working units beside the distance
    /// and angle and beside ΔX, ΔY and ΔZ; absolute X/Y/Z input shows them
    /// once.
    #[test]
    fn the_tracker_adds_absolute_xyz_on_demand() {
        let mut env = Env::new();
        env.elevation = 3.0;
        let mut input = PointInput::default();
        let mut tool = wall_tool(WallMode::Single);
        env.tracker_xyz = true;
        let tracker = input.tracker(DVec2::new(2.0, -1.5), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, "▸X 2.000 m\n Y -1.500 m\n Z 3.000 m");
        env.click(&mut input, &mut tool, (1.0, 1.0));
        env.tracker_xyz = false;
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, "▸d 5.000 m\n a 53.13°");
        env.tracker_xyz = true;
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert_eq!(tracker, "▸d 5.000 m\n a 53.13°\n X 4.000 m\n Y 5.000 m\n Z 3.000 m");
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert_eq!(
            tracker,
            "▸ΔX 3.000 m\n ΔY 4.000 m\n ΔZ 0.000 m\n X 4.000 m\n Y 5.000 m\n Z 3.000 m"
        );
        env.units.length = archicraft_model::units::LengthUnit::Centimeter;
        env.units.length_decimals = 1;
        env.key(&mut input, &mut tool, KeyInput::SwitchFields);
        let tracker = input.tracker(DVec2::new(4.0, 5.0), &env.ctx(&tool)).unwrap();
        assert!(tracker.ends_with(" X 400.0 cm\n Y 500.0 cm\n Z 300.0 cm"), "{tracker}");
    }

    /// INP-002: a stretch opens in total-length mode, so the tracker shows the
    /// resulting total length, the cursor projected onto the direction; a
    /// typed total of 5.0 on a 4.0 long element moves its end by 1.0.
    #[test]
    fn a_stretch_takes_a_typed_total_length() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut stub = StretchStub::new(false);
        let cursor = DVec2::new(5.5, 2.0);
        let tracker = input.tracker(cursor, &env.ctx(&stub)).unwrap();
        assert_eq!(tracker, "▸L 4.500 m");
        assert!(input.captures_keys(&stub));
        env.key(&mut input, &mut stub, text("5"));
        assert_eq!(input.typed.field, Field::Total);
        assert_eq!(input.tracker(cursor, &env.ctx(&stub)).unwrap(), "▸L 5_");
        assert_eq!(point_at(&env, &mut input, &stub, (3.0, 2.0)), DVec2::new(6.0, 1.0));
        assert_eq!(env.key(&mut input, &mut stub, KeyInput::Enter), Effect::None);
        let end = StretchStub::BASE + DVec2::X * StretchStub::LENGTH;
        assert_eq!(stub.end, Some(end + DVec2::new(1.0, 0.0)), "the end moved by 1.0");
    }

    /// INP-002: an untyped stretch point lies on the stretch line, the cursor
    /// projected onto the direction, so a click commits the length the
    /// tracker shows and the preview ends there.
    #[test]
    fn an_untyped_stretch_point_lies_on_the_stretch_line() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut stub = StretchStub::new(false);
        let cursor = DVec2::new(5.5, 2.0);
        let resolved = resolve_at(&env, &mut input, &stub, cursor);
        assert_eq!(resolved.point, DVec2::new(5.5, 1.0));
        assert_eq!(input.tracker(resolved.point, &env.ctx(&stub)).unwrap(), "▸L 4.500 m");
        let preview = stub.preview(&env.project, &resolved);
        assert_eq!(preview.outlines[0].0[1], DVec2::new(5.5, 1.0));
        assert_eq!(env.click(&mut input, &mut stub, (5.5, 2.0)), Effect::None);
        assert_eq!(stub.end, Some(DVec2::new(5.5, 1.0)));
    }

    /// INP-002: a stretch direction is used as a unit vector whatever its
    /// length.
    #[test]
    fn a_stretch_direction_is_normalised() {
        let mode = TrackerMode::TotalLength {
            base: DVec2::ZERO,
            dir: DVec2::new(0.0, 2.0),
            length: 1.0,
        };
        assert_eq!(mode.stretch(), Some((DVec2::ZERO, DVec2::Y, 1.0)));
        let degenerate = TrackerMode::LengthChange {
            base: DVec2::ZERO,
            dir: DVec2::ZERO,
            length: 1.0,
        };
        assert_eq!(degenerate.stretch(), None);
    }

    /// INP-002: Tab switches a stretch to the length change on demand; the
    /// tracker then shows the change and a typed change of 0.5 on a 4.0 long
    /// element gives the total length 4.5. A consumer can open in that mode.
    #[test]
    fn a_stretch_takes_a_typed_length_change_on_demand() {
        let mut env = Env::new();
        let mut input = PointInput::default();
        let mut stub = StretchStub::new(false);
        let cursor = DVec2::new(5.5, 2.0);
        env.key(&mut input, &mut stub, KeyInput::Tab);
        assert_eq!(input.typed.field, Field::Change);
        assert_eq!(input.tracker(cursor, &env.ctx(&stub)).unwrap(), "▸ΔL 0.500 m");
        for key in [text("0.5"), KeyInput::SwitchFields, KeyInput::SwitchReference] {
            env.key(&mut input, &mut stub, key);
        }
        assert_eq!(input.tracker(cursor, &env.ctx(&stub)).unwrap(), "▸ΔL 0.5_");
        assert_eq!(env.key(&mut input, &mut stub, KeyInput::Enter), Effect::None);
        let end = stub.end.unwrap();
        assert_eq!(end, DVec2::new(5.5, 1.0));
        assert_eq!(end.distance(StretchStub::BASE), 4.5, "the total length");

        let mut input = PointInput::default();
        let stub = StretchStub::new(true);
        assert_eq!(
            input.tracker(DVec2::new(3.0, 0.0), &env.ctx(&stub)).unwrap(),
            "▸ΔL -2.000 m"
        );
        resolve_at(&env, &mut input, &stub, DVec2::new(3.0, 0.0));
        assert_eq!(input.typed.field, Field::Change);
    }
}
