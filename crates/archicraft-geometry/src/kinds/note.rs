//! The generator of the body-less test kind, behind the cargo feature
//! `test-kinds` (see [`crate::kinds`]). It is registered like every kind's
//! generator, but never asked for bodies: [`crate::evaluate`] gives a kind
//! whose descriptor has no body an empty geometry before a generator runs
//! (ANN-002), as it will for texts, markers and dimensions.

use archicraft_model::kinds::Note;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::ElementGeometry;

impl GeometryKind for Note {
    /// No body, no axis, no parts (ANN-002).
    fn evaluate(&self, _ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        Ok(ElementGeometry::default())
    }
}
