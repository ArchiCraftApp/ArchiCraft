//! Walls (SRS 4.11; WAL-001, WAL-003).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor, KindTag, StoreyLinkRef, Vertical, VerticalLink, plan_bounds_of, vertical};
use crate::element::{Element, IfcClass, Position, PredefinedType, StructuralFunction, TopLink};
use crate::fields::{self, FieldSpec, FieldType, FieldValue, Section};
use crate::ids::{MaterialId, StoreyId};
use crate::message::Message;
use crate::project::{DefaultsSlot, Project};
use crate::storey::StoreyList;
use crate::template::TemplateContext;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// Location of a wall's reference line within its thickness (WAL-003).
///
/// The reference line runs from start to end. The exterior side is on the
/// right of that direction and the interior side on the left, so walls drawn
/// counter-clockwise around a building have their exterior faces outside.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReferenceLine {
    Outside,
    Center,
    Inside,
    /// Distance of the reference line from the exterior face, towards the
    /// interior face.
    Custom(f64),
}

impl ReferenceLine {
    /// Offsets of the exterior and interior faces from the reference line,
    /// measured along the left-hand normal of the reference line.
    pub fn face_offsets(self, thickness: f64) -> (f64, f64) {
        let from_exterior = match self {
            ReferenceLine::Outside => 0.0,
            ReferenceLine::Center => thickness / 2.0,
            ReferenceLine::Inside => thickness,
            ReferenceLine::Custom(offset) => offset,
        };
        (-from_exterior, thickness - from_exterior)
    }

    /// Message key of the location's name (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        match self {
            ReferenceLine::Outside => "model-reference-line-outside",
            ReferenceLine::Center => "model-reference-line-center",
            ReferenceLine::Inside => "model-reference-line-inside",
            ReferenceLine::Custom(_) => "model-reference-line-custom",
        }
    }

    /// The locations the `reference_line` field offers, in display order; a
    /// custom location stands for every offset.
    pub const ALL: [ReferenceLine; 4] = [
        ReferenceLine::Outside,
        ReferenceLine::Center,
        ReferenceLine::Inside,
        ReferenceLine::Custom(0.0),
    ];

    /// The location with the code `code` ([`Self::code`]); the code
    /// `custom` gives a custom location at `custom_offset` from the exterior
    /// face. `None` for an unknown code.
    pub fn from_code(code: &str, custom_offset: f64) -> Option<Self> {
        Self::ALL.into_iter().find(|r| r.code() == code).map(|r| match r {
            ReferenceLine::Custom(_) => ReferenceLine::Custom(custom_offset),
            other => other,
        })
    }

    /// The location's code in the `reference_line` field, as in files.
    pub fn code(self) -> &'static str {
        match self {
            ReferenceLine::Outside => "outside",
            ReferenceLine::Center => "center",
            ReferenceLine::Inside => "inside",
            ReferenceLine::Custom(_) => "custom",
        }
    }

    /// Next location, for cycling while drawing (WAL-007).
    pub fn next(self) -> Self {
        match self {
            ReferenceLine::Outside => ReferenceLine::Center,
            ReferenceLine::Center => ReferenceLine::Inside,
            ReferenceLine::Inside | ReferenceLine::Custom(_) => ReferenceLine::Outside,
        }
    }
}

/// A straight wall (WAL-001, WAL-003).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wall {
    pub start: DVec2,
    pub end: DVec2,
    pub thickness: f64,
    pub reference_line: ReferenceLine,
    /// Base offset relative to the home storey elevation.
    pub base_offset: f64,
    pub top: TopLink,
    pub material: MaterialId,
}

impl Wall {
    pub fn length(&self) -> f64 {
        self.start.distance(self.end)
    }

    pub fn direction(&self) -> DVec2 {
        (self.end - self.start).normalize_or_zero()
    }

    /// The wall's base offset and top as a [`VerticalLink`] (ELM-006).
    pub fn link(&self) -> VerticalLink {
        VerticalLink {
            base_offset: self.base_offset,
            top: self.top,
        }
    }

    /// Sets the wall's base offset and top from `link` (ELM-006). Does not
    /// validate the link; validation runs when a command applies the wall.
    pub fn set_link(&mut self, link: VerticalLink) {
        self.base_offset = link.base_offset;
        self.top = link.top;
    }

    /// Applies `change` to the wall's vertical link. The [`VerticalLink`]
    /// helpers leave the link unchanged when they fail.
    fn change_link<T>(&mut self, change: impl FnOnce(&mut VerticalLink) -> T) -> T {
        let mut link = self.link();
        let result = change(&mut link);
        self.set_link(link);
        result
    }
}

/// The wall tool's defaults in the template (Appendix D.2): 0.25 m brick on
/// the exterior walls layer, the reference line on the exterior face, the
/// top linked to the home storey + 1.
fn template_defaults(context: &TemplateContext<'_>) -> DefaultsSlot {
    DefaultsSlot {
        layer: context.layer("Exterior walls"),
        position: Position::Exterior,
        structural: StructuralFunction::Undefined,
        ifc_class: Wall::DESCRIPTOR.default_class,
        predefined_type: None,
        kind: Wall {
            start: DVec2::ZERO,
            end: DVec2::X,
            thickness: 0.25,
            reference_line: ReferenceLine::Outside,
            base_offset: 0.0,
            top: TopLink::Relative {
                storeys: 1,
                offset: 0.0,
            },
            material: context.material("Brick"),
        }
        .into(),
    }
}

impl Project {
    /// A new wall from `start` to `end` on `storey` with the wall tool's
    /// defaults (ELM-001). `nth` numbers walls created together.
    pub fn new_wall(&self, storey: StoreyId, start: DVec2, end: DVec2, nth: u32) -> Element {
        let mut element = self.new_element(KindTag::Wall, storey, nth)
            .unwrap_or_else(|| unreachable!("walls have template defaults"));
        if let Some(wall) = element.as_wall_mut() {
            (wall.start, wall.end) = (start, end);
        }
        element
    }
}

impl Vertical for Wall {
    fn link_of(element: &Element) -> Option<VerticalLink> {
        element.as_wall().map(Wall::link)
    }

    fn set_link_of(element: &mut Element, link: VerticalLink) {
        if let Some(wall) = element.as_wall_mut() {
            wall.set_link(link);
        }
    }
}

const HEIGHT_FIELDS: [FieldSpec; 6] = vertical::fields::<Wall>("model-error-wall-height-range");

/// The wall's parameters (WAL-001, WAL-003, ELM-006), ranged as
/// [`validate::validate_wall`] checks them (ELM-017).
const FIELDS: &[FieldSpec] = &[
    FieldSpec::new(
        "thickness",
        "model-field-thickness",
        Section::Geometry,
        FieldType::Length,
        |e, _| e.as_wall().map(|w| FieldValue::Length(w.thickness)),
        |e, v, _| {
            if let (Some(w), Some(v)) = (e.as_wall_mut(), fields::length(v)) {
                w.thickness = v;
            }
        },
    )
    .ranged(validate::MIN_THICKNESS, validate::MAX_THICKNESS, "model-error-thickness-range"),
    FieldSpec::choice(
        "reference_line",
        "model-field-reference-line",
        Section::Geometry,
        |e, _| e.as_wall().map(|w| FieldValue::Code(w.reference_line.code())),
        |e, v, _| {
            let Some(w) = e.as_wall_mut() else { return };
            // A custom offset starts where the line is, so it does not move
            // the wall.
            let current = -w.reference_line.face_offsets(w.thickness).0;
            if let Some(line) = fields::code(v).and_then(|c| ReferenceLine::from_code(c, current)) {
                w.reference_line = line;
            }
        },
        |_, _| {
            ReferenceLine::ALL
                .iter()
                .map(|r| (FieldValue::Code(r.code()), fields::message(r.label_key())))
                .collect()
        },
    ),
    FieldSpec::new(
        "reference_offset",
        "model-field-reference-offset",
        Section::Geometry,
        FieldType::Elevation,
        |e, _| match e.as_wall()?.reference_line {
            ReferenceLine::Custom(offset) => Some(FieldValue::Length(offset)),
            _ => None,
        },
        |e, v, _| {
            if let (Some(w), Some(v)) = (e.as_wall_mut(), fields::length(v))
                && let ReferenceLine::Custom(_) = w.reference_line
            {
                w.reference_line = ReferenceLine::Custom(v);
            }
        },
    )
    .ranged(
        -validate::MAX_THICKNESS,
        validate::MAX_THICKNESS,
        "model-error-reference-line-range",
    ),
    HEIGHT_FIELDS[0],
    HEIGHT_FIELDS[1],
    HEIGHT_FIELDS[2],
    HEIGHT_FIELDS[3],
    HEIGHT_FIELDS[4],
    HEIGHT_FIELDS[5],
];

impl Kind for Wall {
    /// ELM-011: `IfcWall` by default, `IfcFooting` selectable; ELM-019:
    /// names `W-001`, `W-002`, …
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-wall",
        noun_key: "model-kind-wall-noun",
        node_prefix: "Wall",
        code: "wall",
        name_prefix: "W",
        default_class: IfcClass::Wall,
        ifc_classes: IfcClass::WALL_CLASSES,
        predefined_types: PredefinedType::for_class,
        has_tool: true,
        has_body: true,
        ifcx_node: true,
        construction: true,
        template_defaults: Some(template_defaults),
        fields: FIELDS,
    };

    fn material(&self) -> Option<MaterialId> {
        Some(self.material)
    }

    fn set_material(&mut self, material: MaterialId) {
        self.material = material;
    }

    fn validate(&self, _home: StoreyId, storeys: &StoreyList) -> Result<Self, Message> {
        validate::validate_wall(self)?;
        if self.link().missing_storey(storeys).is_some() {
            return Err(Message::new("model-error-wall-top-storey-missing"));
        }
        Ok(self.clone())
    }

    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        let (start, end) = (transform.apply(self.start), transform.apply(self.end));
        // A mirror swaps the sides of the reference line. Swapping the ends
        // keeps the exterior face on the right, now on the mirrored exterior
        // side (WAL-003).
        (self.start, self.end) = if transform.is_mirror() {
            (end, start)
        } else {
            (start, end)
        };
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        self.link().extent(home, storeys)
    }

    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        self.link().storey_links()
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, _target: StoreyId, target_elevation: f64) -> Option<()> {
        self.change_link(|link| link.rehome(home, storeys, target_elevation))
    }

    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        self.change_link(|link| link.relink(home, storeys, target))
    }

    fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        self.change_link(|link| link.unlink_storey(home, storeys, storey))
    }

    fn fix_top(&mut self, height: f64) -> bool {
        self.change_link(|link| link.fix_top(height))
    }

    /// The corners of the unjoined footprint: both faces at both ends.
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        let normal = self.direction().perp();
        let (exterior, interior) = self.reference_line.face_offsets(self.thickness);
        plan_bounds_of(
            [self.start, self.end]
                .into_iter()
                .flat_map(|p| [p + normal * exterior, p + normal * interior]),
        )
    }
}
