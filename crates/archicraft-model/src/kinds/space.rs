//! Spaces (SRS 4.25; SPC-001): a room with a name, a number, a boundary
//! and a storey-linked height.
//!
//! A space is no construction element: it has a body, the volume of the
//! room, but no material, joins nothing and bounds no region in contour
//! detection ([`KindDescriptor::construction`]). Rooms and zones with area
//! calculation rules, deductions, schedules and finishes are BIM information
//! management and outside v1 (SRS 4.25, Section 8), so the name and the
//! number are free text without uniqueness checks.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Kind, KindDescriptor, KindTag, StoreyLinkRef, Vertical, VerticalLink, transform_outline, vertical};
use crate::element::{Element, IfcClass, Position, PredefinedType, StructuralFunction, TopLink};
use crate::fields::{FieldSpec, FieldType, FieldValue, Section};
use crate::ids::StoreyId;
use crate::message::Message;
use crate::outline::Outline;
use crate::project::{DefaultsSlot, Project};
use crate::storey::StoreyList;
use crate::template::TemplateContext;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// Where the boundary of a space comes from (SPC-001, SPC-002).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub enum BoundarySource {
    /// Drawn node by node.
    #[default]
    Drawn,
    /// Detected as the region around `seed` bounded by walls (INP-018). With
    /// `auto_update`, the boundary follows when the bounding walls move
    /// (SPC-002).
    Detected { seed: DVec2, auto_update: bool },
}

/// A space (SPC-001).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Space {
    /// The room name, for example "Kitchen". The element's name keeps the
    /// automatic identifier `SP-001` (ELM-019).
    pub long_name: String,
    /// The room number, for example "0.01"; see
    /// [`Project::suggested_space_number`].
    pub number: String,
    /// The outer ring, then the holes inside it. After [`Kind::validate`]
    /// it holds what a slab's outline holds (see [`crate::Slab::outline`]):
    /// the outer ring counter-clockwise, the holes clockwise, each cleaned
    /// and without touching edges.
    pub boundary: Outline,
    /// Base offset and top: by default the base at the home storey and the
    /// top at the next storey (ELM-006), so the space follows storey edits
    /// (STR-006).
    pub link: VerticalLink,
    pub source: BoundarySource,
    /// Offset of the plan stamp from the boundary's interior point; zero by
    /// default, so files stay deterministic.
    pub stamp_offset: DVec2,
}

impl Space {
    /// The area inside the boundary less its holes, in square metres
    /// (SPC-001). No deductions or area rules apply (SRS Section 8).
    pub fn net_area(&self) -> f64 {
        self.boundary.area()
    }
}

/// The space tool's defaults in the template. Appendix D has no values for
/// spaces; proposed: the top linked to the home storey + 1 with offset 0, as
/// for walls (Appendix D.2), no name and no number. The template has no
/// layer for spaces (LAY-004 lists none), so [`TemplateContext::layer`]
/// gives the first layer until it gets one.
fn template_defaults(context: &TemplateContext<'_>) -> DefaultsSlot {
    DefaultsSlot {
        layer: context.layer("Spaces"),
        position: Position::Undefined,
        structural: StructuralFunction::Undefined,
        ifc_class: Space::DESCRIPTOR.default_class,
        predefined_type: None,
        kind: Space {
            long_name: String::new(),
            number: String::new(),
            boundary: Outline::from_point_rings([[DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y]]),
            link: VerticalLink {
                base_offset: 0.0,
                top: TopLink::Relative {
                    storeys: 1,
                    offset: 0.0,
                },
            },
            source: BoundarySource::Drawn,
            stamp_offset: DVec2::ZERO,
        }
        .into(),
    }
}

impl Project {
    /// A new space with the boundary `outer` and `holes` on `storey` with
    /// the space tool's defaults (ELM-001). `nth` numbers spaces created
    /// together. The rings are kept as given; validation orients them.
    pub fn new_space(&self, storey: StoreyId, outer: Vec<DVec2>, holes: Vec<Vec<DVec2>>, nth: u32) -> Element {
        let mut element = self
            .new_element(KindTag::Space, storey, nth)
            .unwrap_or_else(|| unreachable!("spaces have template defaults"));
        if let Some(space) = element.as_space_mut() {
            space.boundary = Outline::from_point_rings(std::iter::once(outer).chain(holes));
        }
        element
    }

    /// The number suggested for the next space on `storey` (SPC-001): the
    /// storey number (0 for the ground storey, STR-001), a point and the
    /// count of the storey's spaces plus one in two digits, for example
    /// `0.01`, `0.02`, `1.01` or `-1.01`. A suggestion only: numbers are free
    /// text and may repeat. `None` if the storey does not exist.
    pub fn suggested_space_number(&self, storey: StoreyId) -> Option<String> {
        let number = self.storeys.number(storey)?;
        let spaces = self.elements_on(storey).filter(|e| e.tag() == KindTag::Space).count();
        Some(format!("{number}.{:02}", spaces + 1))
    }
}

impl Vertical for Space {
    fn link_of(element: &Element) -> Option<VerticalLink> {
        element.as_space().map(|s| s.link)
    }

    fn set_link_of(element: &mut Element, link: VerticalLink) {
        if let Some(space) = element.as_space_mut() {
            space.link = link;
        }
    }
}

const HEIGHT_FIELDS: [FieldSpec; 6] = vertical::fields::<Space>("model-error-space-height-range");

/// The space's parameters (SPC-001): the room name, which for the tool's
/// defaults is the default name with `{n}` for the space's number in its
/// kind, the number and the base and top, ranged as
/// [`validate::validate_space`] checks them (ELM-017).
const FIELDS: &[FieldSpec] = &[
    FieldSpec::new(
        "long_name",
        "model-field-room-name",
        Section::Kind,
        FieldType::Text,
        |e, _| e.as_space().map(|s| FieldValue::Text(s.long_name.clone())),
        |e, v, _| {
            if let (Some(s), FieldValue::Text(v)) = (e.as_space_mut(), v) {
                s.long_name.clone_from(v);
            }
        },
    )
    .with_defaults_label("model-field-default-name"),
    FieldSpec::new(
        "number",
        "model-field-number",
        Section::Kind,
        FieldType::Text,
        |e, _| e.as_space().map(|s| FieldValue::Text(s.number.clone())),
        |e, v, _| {
            if let (Some(s), FieldValue::Text(v)) = (e.as_space_mut(), v) {
                s.number.clone_from(v);
            }
        },
    ),
    HEIGHT_FIELDS[0],
    HEIGHT_FIELDS[1],
    HEIGHT_FIELDS[2],
    HEIGHT_FIELDS[3],
    HEIGHT_FIELDS[4],
    HEIGHT_FIELDS[5],
];

impl Kind for Space {
    /// ELM-011, Table 5-1: always an `IfcSpace`; ELM-019: names `SP-001`,
    /// `SP-002`, …; a body without a material and no construction element.
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-space",
        noun_key: "model-kind-space-noun",
        node_prefix: "Space",
        code: "space",
        name_prefix: "SP",
        default_class: IfcClass::Space,
        ifc_classes: IfcClass::SPACE_CLASSES,
        predefined_types: PredefinedType::for_class,
        has_tool: true,
        has_body: true,
        ifcx_node: true,
        construction: false,
        template_defaults: Some(template_defaults),
        fields: FIELDS,
    };

    /// Normalises the boundary (EDT-018); a positive height is checked
    /// against the storeys by [`validate::validate_element`].
    fn validate(&self, _home: StoreyId, _storeys: &StoreyList) -> Result<Self, Message> {
        validate::validate_space(self)
    }

    /// The boundary and the seed move with the space and the stamp offset
    /// turns with it, unchanged by a move; the boundary stays oriented when
    /// mirrored.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        transform_outline(&mut self.boundary, transform);
        if let BoundarySource::Detected { seed, .. } = &mut self.source {
            *seed = transform.apply(*seed);
        }
        if !transform.is_translation() {
            self.stamp_offset = transform.apply(self.stamp_offset) - transform.apply(DVec2::ZERO);
        }
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        self.link.extent(home, storeys)
    }

    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        self.link.storey_links()
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, _target: StoreyId, target_elevation: f64) -> Option<()> {
        self.link.rehome(home, storeys, target_elevation)
    }

    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        self.link.relink(home, storeys, target)
    }

    fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        self.link.unlink_storey(home, storeys, storey)
    }

    fn fix_top(&mut self, height: f64) -> bool {
        self.link.fix_top(height)
    }

    /// Holes lie inside the boundary, so the outer ring bounds the space.
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        self.boundary.bounds()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline::Ring;
    use crate::template::default_metric_project;

    fn detected(seed: DVec2) -> Space {
        let p = default_metric_project();
        let mut space = p
            .new_space(
                p.storeys.ground(),
                vec![DVec2::ZERO, DVec2::X, DVec2::ONE, DVec2::Y],
                Vec::new(),
                0,
            )
            .as_space()
            .cloned()
            .unwrap_or_else(|| unreachable!("a new space is a space"));
        space.boundary.rings.push(Ring::from_points([
            DVec2::splat(0.25),
            DVec2::new(0.25, 0.5),
            DVec2::splat(0.5),
            DVec2::new(0.5, 0.25),
        ]));
        space.source = BoundarySource::Detected {
            seed,
            auto_update: true,
        };
        space.stamp_offset = DVec2::new(0.1, 0.0);
        space
    }

    /// SPC-001, EDT-003: a mirrored space keeps its outer ring
    /// counter-clockwise and its hole clockwise, its seed moves with it and
    /// its stamp offset is mirrored without the translation.
    #[test]
    fn a_mirrored_space_keeps_its_boundary_oriented() {
        let mut space = detected(DVec2::new(0.75, 0.75));
        let mirror = PlanTransform::mirror(DVec2::new(2.0, 0.0), DVec2::new(2.0, 1.0))
            .unwrap_or_else(|| unreachable!("the axis has a direction"));
        space.transform(&mirror).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(space.boundary.outer().is_counter_clockwise());
        assert!(!space.boundary.holes()[0].is_counter_clockwise());
        assert_eq!(space.boundary.outer().node(0), DVec2::new(4.0, 0.0));
        let BoundarySource::Detected { seed, auto_update } = space.source else {
            panic!("the source stays detected: {:?}", space.source);
        };
        assert!(seed.distance(DVec2::new(3.25, 0.75)) < 1e-12 && auto_update, "{seed}");
        assert!(
            space.stamp_offset.distance(DVec2::new(-0.1, 0.0)) < 1e-12,
            "{}",
            space.stamp_offset
        );
        assert!((space.net_area() - (1.0 - 0.0625)).abs() < 1e-12);
    }

    /// SPC-001, EDT-001: a translated space moves its boundary and seed but
    /// keeps its stamp offset.
    #[test]
    fn a_moved_space_keeps_its_stamp_offset() {
        let mut space = detected(DVec2::splat(0.75));
        space
            .transform(&PlanTransform::translation(DVec2::new(3.0, -1.0)))
            .unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(space.boundary.outer().node(0), DVec2::new(3.0, -1.0));
        assert_eq!(
            space.source,
            BoundarySource::Detected {
                seed: DVec2::new(3.75, -0.25),
                auto_update: true
            }
        );
        assert_eq!(space.stamp_offset, DVec2::new(0.1, 0.0));
    }
}
