//! The generator of texts (ANN-007). It is registered like every kind's
//! generator, but never asked for bodies: [`crate::evaluate`] gives a kind
//! whose descriptor has no body an empty geometry before a generator runs
//! (ANN-002). The views lay texts out from the model, because glyph metrics
//! need the application's fonts and this crate stays headless.

use archicraft_model::Text;

use super::{EvalContext, GeometryKind};
use crate::GeometryError;
use crate::evaluate::ElementGeometry;

impl GeometryKind for Text {
    /// No body, no axis, no parts (ANN-002).
    fn evaluate(&self, _ctx: &EvalContext<'_>) -> Result<ElementGeometry, GeometryError> {
        Ok(ElementGeometry::default())
    }
}
