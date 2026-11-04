//! A test-only element kind without a body, behind the cargo feature
//! `test-kinds` (see the documentation of [`crate::kinds`]).
//!
//! The note stands in for texts, markers and dimensions: it has no 3D body
//! and no material, belongs to the plan view of its home storey (ANN-002)
//! and is not written as an IFCX node (ANN-010). Like the other test kind,
//! nothing outside this module and its line in the registry names it.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor, KindTag, plan_bounds_of};
use crate::drafting_view::{DraftingView, zero_height_extent};
use crate::element::{Element, IfcClass, Position, PredefinedType};
use crate::ids::{ElementId, StoreyId};
use crate::message::Message;
use crate::project::Project;
use crate::storey::StoreyList;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// A line of text at a plan point of a drafting view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// The view the note belongs to, the plan view of its home storey.
    pub view: DraftingView,
    /// Where the note sits in plan.
    pub at: DVec2,
    pub text: String,
}

impl Note {
    /// A note at `at` in the plan view of `storey`.
    pub fn new(storey: StoreyId, at: DVec2, text: impl Into<String>) -> Self {
        Self {
            view: DraftingView::plan(storey),
            at,
            text: text.into(),
        }
    }

    /// An element of this kind on `storey` with the kind's default IFC
    /// class, the project's first layer and the next automatic name; `nth`
    /// numbers elements created together (ELM-019).
    pub fn into_element(self, project: &Project, storey: StoreyId, nth: u32) -> Element {
        Element {
            id: ElementId::new(),
            name: project.next_element_name(KindTag::Note, nth),
            storey,
            layer: project.layers[0].id,
            ifc_class: Self::DESCRIPTOR.default_class,
            predefined_type: None,
            position: Position::Undefined,
            locked: false,
            snap_excluded: false,
            group: None,
            kind: self.into(),
        }
    }
}

impl Kind for Note {
    /// Table 5-1: an `IfcAnnotation`; names `NT-001`, `NT-002`, …; no
    /// drawing tool, no body, and stored outside the IFCX node tree like
    /// texts and dimensions (ANN-010).
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-test-annotation",
        noun_key: "model-kind-test-annotation-noun",
        node_prefix: "Note",
        code: "note",
        name_prefix: "NT",
        default_class: IfcClass::Annotation,
        ifc_classes: IfcClass::ANNOTATION_CLASSES,
        predefined_types: PredefinedType::for_class,
        has_tool: false,
        has_body: false,
        ifcx_node: false,
        template_defaults: None,
    };

    /// The point must be within the model's range, and the view must be the
    /// plan view of the home storey (ANN-002).
    fn validate(&self, home: StoreyId, storeys: &StoreyList) -> Result<Self, Message> {
        validate::check_point(self.at)?;
        self.view.check_binding(home, storeys)?;
        Ok(self.clone())
    }

    /// The point moves, rotates and mirrors; the text keeps its reading.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        self.at = transform.apply(self.at);
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        zero_height_extent(home, storeys)
    }

    /// The note stays where it is in plan and moves to the plan view of
    /// `target`.
    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId, _target_elevation: f64) -> Option<()> {
        storeys.get(home)?;
        self.view.rebind(target);
        Some(())
    }

    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        plan_bounds_of([self.at])
    }
}
