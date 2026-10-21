//! A test-only element kind, behind the cargo feature `test-kinds` (see the
//! documentation of [`crate::kinds`]).
//!
//! The dummy proves that a kind is one module plus one registry line: it is
//! registered like walls and slabs, but nothing outside this module and its
//! line in the registry names it. It is a box over an axis-aligned plan
//! rectangle with a [`VerticalLink`] (ELM-006), and carries flags for later
//! registry tests: compound parts (IFX-016), a panicking geometry generator
//! (NFR-REL-006) and placement under the site (Table 5-1).

use glam::DVec2;
use serde::{Deserialize, Serialize};

use super::{Container, Kind, KindDescriptor, KindTag, PartSpec, StoreyLinkRef, VerticalLink, plan_bounds_of};
use crate::element::{Element, IfcClass, Position, TopLink};
use crate::ids::{ElementId, MaterialId, StoreyId};
use crate::message::Message;
use crate::project::Project;
use crate::storey::StoreyList;
use crate::transform::{PlanTransform, TransformError};
use crate::validate;

/// A box over the plan rectangle from `origin` to `origin + size`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Dummy {
    /// Corner of the plan rectangle with the smallest coordinates.
    pub origin: DVec2,
    /// Extent of the plan rectangle along X and Y, both positive.
    pub size: DVec2,
    pub link: VerticalLink,
    pub material: MaterialId,
    /// Asks for compound parts. [`Kind::parts`] stays empty until the part
    /// classes (`IfcMember`, `IfcPlate`) exist.
    pub parts: bool,
    /// Asks the geometry generator to panic, to exercise the fallback for
    /// failed elements (NFR-REL-006).
    pub panic_on_evaluate: bool,
    /// Places the element under the site instead of a storey (Table 5-1).
    pub site_level: bool,
}

impl Dummy {
    /// A 1 m high box on its home storey's elevation, without flags.
    pub fn new(origin: DVec2, size: DVec2, material: MaterialId) -> Self {
        Self {
            origin,
            size,
            link: VerticalLink {
                base_offset: 0.0,
                top: TopLink::Fixed { height: 1.0 },
            },
            material,
            parts: false,
            panic_on_evaluate: false,
            site_level: false,
        }
    }

    /// An element of this kind on `storey` with the kind's default IFC
    /// class, the project's first layer and the next automatic name; `nth`
    /// numbers elements created together (ELM-019).
    pub fn into_element(self, project: &Project, storey: StoreyId, nth: u32) -> Element {
        Element {
            id: ElementId::new(),
            name: project.next_element_name(KindTag::Dummy, nth),
            storey,
            layer: project.layers[0].id,
            ifc_class: Self::DESCRIPTOR.default_class,
            position: Position::Undefined,
            locked: false,
            kind: self.into(),
        }
    }

    fn corners(&self) -> [DVec2; 4] {
        let far = self.origin + self.size;
        [
            self.origin,
            DVec2::new(far.x, self.origin.y),
            far,
            DVec2::new(self.origin.x, far.y),
        ]
    }
}

impl Kind for Dummy {
    /// `IfcBuildingElementProxy` by default, `IfcFooting` selectable; names
    /// `X-001`, `X-002`, …; no drawing tool.
    const DESCRIPTOR: KindDescriptor = KindDescriptor {
        label_key: "model-kind-test",
        noun_key: "model-kind-test-noun",
        node_prefix: "Dummy",
        code: "dummy",
        name_prefix: "X",
        default_class: IfcClass::BuildingElementProxy,
        ifc_classes: &[IfcClass::BuildingElementProxy, IfcClass::Footing],
        has_tool: false,
    };

    fn material(&self) -> MaterialId {
        self.material
    }

    fn set_material(&mut self, material: MaterialId) {
        self.material = material;
    }

    /// The rectangle is checked as a polygon; a missing linked storey shows
    /// as an undeterminable extent in [`validate::validate_element`].
    fn validate(&self, _storeys: &StoreyList) -> Result<Self, Message> {
        if !(self.size.x > 0.0 && self.size.y > 0.0) {
            return Err(Message::new("model-error-polygon-area"));
        }
        validate::validate_ring(&self.corners())?;
        validate::check_vertical_link(&self.link)?;
        Ok(self.clone())
    }

    /// Only translations: an axis-aligned rectangle cannot represent a
    /// rotated or mirrored one.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
        if !transform.is_translation() {
            return Err(TransformError::Unsupported);
        }
        self.origin = transform.apply(self.origin);
        Ok(())
    }

    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        self.link.extent(home, storeys)
    }

    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        self.link.storey_links()
    }

    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()> {
        self.link.rehome(home, storeys, target_elevation)
    }

    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        self.link.relink(home, storeys, target)
    }

    fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        self.link.unlink_storey(home, storeys, storey)
    }

    fn parts(&self) -> Vec<PartSpec> {
        Vec::new()
    }

    fn container(&self) -> Container {
        if self.site_level {
            Container::Site
        } else {
            Container::Storey
        }
    }

    fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
        plan_bounds_of(self.corners())
    }
}
