//! Helpers shared by the command-level integration tests.

use archicraft_model::glam::DVec2;
use archicraft_model::{Element, Project, StoreyId};

pub fn ground(p: &Project) -> StoreyId {
    p.storeys.ground()
}

/// A wall on the ground storey with the wall tool's defaults.
pub fn wall(p: &Project, start: (f64, f64), end: (f64, f64)) -> Element {
    p.new_wall(ground(p), DVec2::new(start.0, start.1), DVec2::new(end.0, end.1), 0)
}
