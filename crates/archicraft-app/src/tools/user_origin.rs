//! Placing the user origin (INP-014): a point-input consumer that takes the
//! new origin, then a point on its X axis or a typed angle. It creates no
//! command: nothing in the model changes, the workspace takes the frame.

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use archicraft_model::{Project, StoreyId};

use crate::input::{Expect, Frame2d, InputConsumer, KeyHint, Outcome, Preview, Resolved, StepSpec};
use crate::tr;

/// The length of the axes drawn in the preview, in metres.
const AXIS: f64 = 1.0;

/// The consumer placing the user origin (INP-014): the origin point, then
/// the direction of the X axis; Enter on the second step keeps rotation 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UserOriginTool {
    origin: Option<DVec2>,
    /// The storey the origin was clicked on.
    pub storey: Option<StoreyId>,
    placed: Option<Frame2d>,
}

impl UserOriginTool {
    /// The frame once both steps are done.
    pub fn placed(&self) -> Option<Frame2d> {
        self.placed
    }
}

/// The X and Y axes of a frame at `origin` rotated by `rotation`.
fn axes(origin: DVec2, rotation: f64) -> Vec<(Vec<DVec2>, bool)> {
    let x = DVec2::from_angle(rotation) * AXIS;
    vec![(vec![origin + x, origin, origin + x.perp()], false)]
}

impl InputConsumer for UserOriginTool {
    fn step(&self) -> Option<StepSpec> {
        if self.placed.is_some() {
            return None;
        }
        let mut hints = Vec::new();
        let step = match self.origin {
            None => {
                hints.push(KeyHint::new("0–9", tr!("prompt-user-origin-key-coordinates")));
                StepSpec::new(tr!("prompt-user-origin-point"), Expect::Point)
            }
            Some(centre) => {
                hints.push(KeyHint::new("Enter", tr!("prompt-user-origin-key-keep-rotation")));
                hints.push(KeyHint::new("Shift", tr!("prompt-user-origin-key-constrain")));
                StepSpec::new(tr!("prompt-user-origin-direction"), Expect::Angle { centre })
            }
        };
        hints.push(KeyHint::new("Esc", tr!("prompt-user-origin-key-cancel")));
        Some(StepSpec { hints, ..step })
    }

    /// The origin: the typed angle of the second step is measured around it
    /// from the plane's X axis.
    fn reference_point(&self) -> Option<DVec2> {
        self.origin
    }

    fn preview(&self, _project: &Project, resolved: &Resolved) -> Preview {
        let outlines = match self.origin {
            None => axes(resolved.point, 0.0),
            Some(origin) if coincident(origin, resolved.point) => axes(origin, 0.0),
            Some(origin) => axes(origin, (resolved.point - origin).to_angle()),
        };
        Preview {
            outlines,
            ..Preview::default()
        }
    }

    fn commit(&mut self, _project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        match self.origin {
            None => {
                self.origin = Some(point);
                self.storey = Some(storey);
                Outcome::Continue
            }
            // A direction needs a second point apart from the origin.
            Some(origin) if coincident(origin, point) => Outcome::Continue,
            Some(origin) => {
                self.placed = Some(Frame2d {
                    origin,
                    rotation: (point - origin).to_angle(),
                });
                Outcome::Done
            }
        }
    }

    fn cancel_step(&mut self) -> bool {
        self.storey = None;
        self.placed.is_none() && self.origin.take().is_some()
    }

    fn backspace(&mut self) -> bool {
        self.cancel_step()
    }

    /// Enter on the second step keeps the plane's axes.
    fn finish(&mut self, _project: &Project) -> Outcome {
        match self.origin {
            Some(origin) if self.placed.is_none() => {
                self.placed = Some(Frame2d { origin, rotation: 0.0 });
                Outcome::Done
            }
            _ => Outcome::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::template::default_metric_project;

    use super::*;

    /// INP-014: the consumer completes in two steps, the origin and a point
    /// on the X axis; Enter on the second step keeps rotation 0, and a
    /// direction point coinciding with the origin is no direction.
    #[test]
    fn the_user_origin_takes_two_steps() {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let mut tool = UserOriginTool::default();
        assert_eq!(tool.step().unwrap().expects, Expect::Point);
        assert_eq!(tool.commit(&project, storey, DVec2::new(10.0, 10.0)), Outcome::Continue);
        assert_eq!(
            tool.step().unwrap().expects,
            Expect::Angle {
                centre: DVec2::new(10.0, 10.0)
            }
        );
        assert_eq!(tool.commit(&project, storey, DVec2::new(10.0, 10.0)), Outcome::Continue);
        assert_eq!(tool.placed(), None);
        assert_eq!(tool.commit(&project, storey, DVec2::new(10.0, 12.0)), Outcome::Done);
        let placed = tool.placed().unwrap();
        assert_eq!(placed.origin, DVec2::new(10.0, 10.0));
        assert!((placed.rotation - 90f64.to_radians()).abs() < 1e-12);
        assert!(tool.step().is_none());

        let mut tool = UserOriginTool::default();
        assert_eq!(tool.finish(&project), Outcome::Continue, "no origin yet");
        tool.commit(&project, storey, DVec2::new(3.0, 4.0));
        assert_eq!(tool.finish(&project), Outcome::Done);
        assert_eq!(
            tool.placed(),
            Some(Frame2d {
                origin: DVec2::new(3.0, 4.0),
                rotation: 0.0
            })
        );
    }

    /// INP-003: Esc and Backspace remove the origin point, then there is
    /// nothing left to cancel.
    #[test]
    fn escape_removes_the_origin_point() {
        let project = default_metric_project();
        let mut tool = UserOriginTool::default();
        tool.commit(&project, project.storeys.ground(), DVec2::ONE);
        assert!(tool.cancel_step());
        assert_eq!(tool.step().unwrap().expects, Expect::Point);
        assert!(!tool.cancel_step());
        assert!(!tool.backspace());
    }
}
