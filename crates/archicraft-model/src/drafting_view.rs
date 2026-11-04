//! The drafting views that 2D elements belong to (ANN-002).
//!
//! Texts, 2D drafting elements, section markers and dimensions have no 3D
//! body ([`KindDescriptor::has_body`](crate::kinds::KindDescriptor::has_body)).
//! Each belongs to a layer, like every element, and to one drafting view:
//! today the plan view of its home storey, so that storey commands move it
//! with its storey; section and elevation views follow with their markers.
//! Several kinds share the view, so it lives here rather than in one kind's
//! module, together with the helpers their hooks call: the binding check,
//! the rebinding when the element moves to another storey and the vertical
//! extent of an element without a body.

use serde::{Deserialize, Serialize};

use crate::ids::StoreyId;
use crate::message::Message;
use crate::storey::StoreyList;

/// The drafting view a 2D element belongs to (ANN-002), stored tagged by its
/// type: `{ "type": "plan", "storey": "<uuid>" }`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum DraftingView {
    /// The plan view of a storey.
    Plan { storey: StoreyId },
}

impl DraftingView {
    /// The plan view of `storey`.
    pub fn plan(storey: StoreyId) -> Self {
        DraftingView::Plan { storey }
    }

    /// The storey whose plan the view is, if it is a plan view.
    pub fn plan_storey(self) -> Option<StoreyId> {
        match self {
            DraftingView::Plan { storey } => Some(storey),
        }
    }

    /// Checks the binding of an element on `home` to this view (ANN-002): a
    /// plan view must show an existing storey, and that storey must be the
    /// element's home storey. The error explains which rule is broken and
    /// names the storeys.
    pub fn check_binding(self, home: StoreyId, storeys: &StoreyList) -> Result<(), Message> {
        match self {
            DraftingView::Plan { storey } => {
                let Some(view) = storeys.get(storey) else {
                    return Err(Message::new("model-error-view-storey-missing"));
                };
                if storey != home {
                    let home = storeys.get(home).map_or_else(String::new, |s| s.name.clone());
                    return Err(Message::new("model-error-view-not-home-storey")
                        .text("view", view.name.clone())
                        .text("home", home));
                }
                Ok(())
            }
        }
    }

    /// Binds the view to `target`, the element's new home storey, when the
    /// element moves there (STR-004, ELM-005): a plan view becomes the plan
    /// view of `target`.
    pub fn rebind(&mut self, target: StoreyId) {
        match self {
            DraftingView::Plan { storey } => *storey = target,
        }
    }
}

/// The vertical extent of an element without a body on `home`: zero height
/// at the storey's elevation, where its view draws it (ANN-002). `None` if
/// the storey is missing, as for every kind.
pub fn zero_height_extent(home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
    let elevation = storeys.get(home)?.elevation;
    Some((elevation, elevation))
}
