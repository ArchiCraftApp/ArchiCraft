//! Text blocks (ANN-007): plain text at a plan point of a drafting view.
//!
//! A text has no body and no material, belongs to the plan view of its home
//! storey (ANN-002) and is stored in the ArchiCraft namespace only, never as
//! an IFCX element (ANN-010, SRS 3.2). Its size, alignment and line spacing
//! come from its [`TextStyle`]; the size may be overridden per text. The
//! anchor is the plan point the text's anchor position sits on, so the text
//! grows away from it (ANN-007).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor, KindTag, plan_bounds_of};
use crate::drafting_view::{DraftingView, zero_height_extent};
use crate::element::{Element, IfcClass, Position, PredefinedType};
use crate::ids::{StoreyId, TextStyleId};
use crate::message::Message;
use crate::project::{DefaultsSlot, Project};
use crate::storey::StoreyList;
use crate::template::{TEXT_STYLES, TemplateContext};
use crate::text::{AnchorPos, TextStyle, upright};
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// A plain text block (ANN-007).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Text {
    /// The view the text belongs to, the plan view of its home storey.
    pub view: DraftingView,
    /// The plan point the anchor position sits on.
    pub anchor: DVec2,
    /// The point of the block on `anchor`, which stays in place when the
    /// text grows.
    pub anchor_pos: AnchorPos,
    /// The reading direction in radians counter-clockwise from the plan's +X
    /// axis. The views draw it turned upright ([`upright`]).
    pub angle: f64,
    /// The text; each `\n` starts a new line.
    pub content: String,
    /// One of the project's text styles.
    pub style: TextStyleId,
    /// The size in paper millimetres instead of the style's, if set.
    pub size_mm: Option<f64>,
}

impl Text {
    /// The size the text is drawn at, in paper millimetres: the override if
    /// set, else the size of `style`, the text's style (ANN-007).
    pub fn effective_size_mm(&self, style: &TextStyle) -> f64 {
        self.size_mm.unwrap_or(style.size_mm)
    }
}

/// The text tool's defaults in the template. Appendix D has no values for
/// texts; proposed: the first preset style without a size override, the
/// bottom-left anchor position, angle 0 and the '2D construction' layer
/// (LAY-004). The view is a placeholder that every text made from the slot
/// replaces by the plan view of its storey ([`Kind::place_on`]), so the slot
/// refers to no storey.
fn template_defaults(context: &TemplateContext<'_>) -> DefaultsSlot {
    DefaultsSlot {
        layer: context.layer("2D construction"),
        position: Position::Undefined,
        ifc_class: Text::DESCRIPTOR.default_class,
        predefined_type: None,
        kind: Text {
            view: DraftingView::plan(StoreyId::from_uuid(uuid::Uuid::nil())),
            anchor: DVec2::ZERO,
            anchor_pos: AnchorPos::BottomLeft,
            angle: 0.0,
            content: String::new(),
            style: context.text_style(TEXT_STYLES[0].0),
            size_mm: None,
        }
        .into(),
    }
}

impl Project {
    /// A new text reading `content` with its anchor at `anchor` on `storey`,
    /// in that storey's plan view, with the text tool's defaults (ELM-001,
    /// ANN-007). `nth` numbers texts created together.
    pub fn new_text(&self, storey: StoreyId, anchor: DVec2, content: impl Into<String>, nth: u32) -> Element {
        let mut element = self
            .new_element(KindTag::Text, storey, nth)
            .unwrap_or_else(|| unreachable!("texts have template defaults"));
        if let Some(text) = element.as_text_mut() {
            text.anchor = anchor;
            text.content = content.into();
        }
        element
    }
}

impl Kind for Text {
    /// Table 5-1: always an `IfcAnnotation`; ELM-019: names `TX-001`,
    /// `TX-002`, …; made with the text tool; no body, no material, no
    /// construction element, and stored outside the IFCX node tree
    /// (ANN-010).
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-text",
        noun_key: "model-kind-text-noun",
        node_prefix: "Text",
        code: "text",
        name_prefix: "TX",
        default_class: IfcClass::Annotation,
        ifc_classes: IfcClass::ANNOTATION_CLASSES,
        predefined_types: PredefinedType::for_class,
        has_tool: true,
        has_body: false,
        ifcx_node: false,
        construction: false,
        template_defaults: Some(template_defaults),
    };

    fn text_style(&self) -> Option<TextStyleId> {
        Some(self.style)
    }

    /// A placed text must have a visible character; the tool's defaults
    /// have no content until the user types it (ANN-007).
    fn check_complete(&self) -> Result<(), Message> {
        if self.content.trim().is_empty() {
            return Err(Message::new("model-error-text-content-empty"));
        }
        Ok(())
    }

    /// The anchor must be within the model's range, the angle finite, a
    /// size override a positive finite number of millimetres, and the view
    /// the plan view of the home storey (ANN-002). Whether the style exists
    /// is checked against the project by [`validate::validate_prototype`],
    /// the content by [`Kind::check_complete`].
    fn validate(&self, home: StoreyId, storeys: &StoreyList) -> Result<Self, Message> {
        validate::check_point(self.anchor)?;
        if !self.angle.is_finite() {
            return Err(Message::new("model-error-text-angle"));
        }
        if self.size_mm.is_some_and(|size| !(size.is_finite() && size > 0.0)) {
            return Err(Message::new("model-error-text-size"));
        }
        self.view.check_binding(home, storeys)?;
        Ok(self.clone())
    }

    /// The anchor moves, rotates and mirrors with the text, and a rotation
    /// adds its angle (EDT-001…EDT-003). A mirror never mirrors the glyphs:
    /// the text keeps reading upright and covers the mirror image of the
    /// block as the views draw it. The mirrored block reads along the
    /// mirrored reading direction with its top and bottom swapped, or, where
    /// that direction would read upside down, the opposite way with its left
    /// and right swapped; the anchor position swaps with them, so the block
    /// stays on the mirrored anchor. The style's alignment stays.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        self.anchor = transform.apply(self.anchor);
        if transform.is_translation() {
            return Ok(());
        }
        if !transform.is_mirror() {
            let turned = transform.apply_vector(DVec2::X);
            self.angle += turned.y.atan2(turned.x);
            return Ok(());
        }
        let reading = transform.apply_vector(DVec2::from_angle(upright(self.angle)));
        let angle = reading.y.atan2(reading.x);
        if (upright(angle) - angle).abs() < std::f64::consts::FRAC_PI_2 {
            self.angle = angle;
            self.anchor_pos = self.anchor_pos.flipped_across_height();
        } else {
            self.angle = upright(angle);
            self.anchor_pos = self.anchor_pos.flipped_across_width();
        }
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        zero_height_extent(home, storeys)
    }

    /// The text stays where it is in plan and moves to the plan view of
    /// `target`.
    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId, _target_elevation: f64) -> Option<()> {
        storeys.get(home)?;
        self.view.rebind(target);
        Some(())
    }

    /// A text made from the tool's defaults belongs to the plan view of its
    /// storey (ANN-002).
    fn place_on(&mut self, storey: StoreyId) {
        self.view.rebind(storey);
    }

    /// The anchor: the extent of the block depends on the font, which only
    /// the views know.
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        plan_bounds_of([self.anchor])
    }
}
