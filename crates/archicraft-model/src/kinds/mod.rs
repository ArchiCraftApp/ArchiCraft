//! Element kinds and their registry (SRS 3.2, ELM-011, ELM-019).
//!
//! Every element has a kind: the tool that created it. Each kind is one
//! module holding its parameter payload and an implementation of [`Kind`],
//! which carries the kind's [`KindDescriptor`] (message keys, file code, name
//! prefix, IFC classes) and its behaviour hooks; [`crate::element`] keeps the
//! kind-independent element envelope. Project queries, validation and
//! commands ask the descriptor or call the hooks instead of matching on
//! kinds, so adding a kind is one new module plus one line in the `kinds!`
//! invocation below.
//!
//! The registry is a static table of `'static` descriptors, not a plugin
//! system (SRS Section 8). The macro generates, from the one list:
//! [`ElementKind`] (the payload enum; serde variant names are the payload
//! names), [`KindTag`] (the payload-free tag with [`KindTag::ALL`] and
//! [`KindTag::descriptor`]), [`KINDS`], the `Element::as_<kind>()` accessors
//! and the methods of [`ElementKind`] that delegate to the hooks. Both enums
//! are `#[non_exhaustive]`: crates outside the model match them with a
//! wildcard arm for kinds they have no specific code for.
//!
//! The base and top of vertical kinds is a shared [`VerticalLink`] (ELM-006),
//! and plan moves, rotations and mirrors go through [`Kind::transform`].
//!
//! # The test kind
//!
//! One kind exists only for tests, behind the cargo feature `test-kinds`: it
//! proves that a kind is one module plus one line in the list below, and it
//! exercises the paths other crates take for kinds they do not know yet. The
//! feature is enabled through dev-dependencies, for this crate's own tests by
//! a dev-dependency on itself. Cargo (resolver 2 and later) activates the
//! features of dev-dependencies only in builds of tests, examples and
//! benchmarks, so a normal or release build never contains the test kind,
//! while `cargo test --workspace` builds every crate with it.

use glam::DVec2;
use serde::{Deserialize, Serialize};

use crate::element::{Element, IfcClass};
use crate::ids::{MaterialId, StoreyId};
use crate::message::Message;
use crate::project::DefaultsSlot;
use crate::storey::StoreyList;
use crate::template::TemplateContext;
use crate::transform::{PlanTransform, TransformError};

mod vertical;

pub use self::slab::SlabReference;
pub use self::vertical::VerticalLink;
pub use self::wall::ReferenceLine;

/// Static description of an element kind (ELM-011, ELM-019).
#[derive(Debug)]
pub struct KindDescriptor {
    /// Message key of the kind's name as a label, for example
    /// `model-kind-wall` ("Wall") (NFR-L10N-001).
    pub label_key: &'static str,
    /// Message key of the kind's noun in a sentence, with a `count`
    /// argument for the plural, for example `model-kind-wall-noun` ("wall",
    /// "walls"). [`crate::message::Arg::Kind`] arguments render with it.
    pub noun_key: &'static str,
    /// Prefix of the IFCX child keys of this kind's elements, for example
    /// `Wall` in `Wall_1a2b3c4d`. Part of the file format, not translated.
    pub node_prefix: &'static str,
    /// Stable identifier in files, for example `wall`.
    pub code: &'static str,
    /// Prefix of automatic element names, for example `W` for `W-001`
    /// (ELM-019).
    pub name_prefix: &'static str,
    /// The IFC class an element of this kind gets by default (ELM-011,
    /// Table 5-1).
    pub default_class: IfcClass,
    /// The IFC classes selectable for this kind, including the default
    /// (ELM-011).
    pub ifc_classes: &'static [IfcClass],
    /// Whether a drawing tool creates this kind. The name counters of such
    /// kinds are always written to files.
    pub has_tool: bool,
    /// The tool's defaults in the template (ELM-001, Appendix D.2), with
    /// the kind's default class (ELM-011); `None` for a kind without tool
    /// defaults. Every kind with template defaults has a slot in
    /// [`crate::project::ToolDefaults`].
    pub template_defaults: Option<fn(&TemplateContext<'_>) -> DefaultsSlot>,
}

/// A link from an element to a specific storey by ID (ELM-006). Relative
/// links follow the home storey and are not listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreyLinkRef {
    /// What is linked, for messages, for example "top".
    pub role: &'static str,
    pub storey: StoreyId,
}

/// A part of a compound element, written as a child node with its own IFC
/// class (IFX-016, Table 5-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartSpec {
    /// Role and child name, for example `Flight_1`.
    pub role: String,
    pub class: IfcClass,
    /// User-facing name, for example "Flight 1".
    pub label: String,
}

/// The spatial node an element is placed under (Table 5-1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Container {
    /// Below its home storey, like walls and slabs.
    #[default]
    Storey,
    /// Below the site, like terrain.
    Site,
}

/// The parameters and behaviour of one element kind (SRS 3.2).
///
/// Hooks that depend on storeys take a [`StoreyList`] rather than the whole
/// project, so that elements can be checked against a proposed storey list
/// before it is applied.
pub trait Kind: Sized {
    const DESCRIPTOR: KindDescriptor;

    /// The element's (main) material.
    fn material(&self) -> MaterialId;

    fn set_material(&mut self, material: MaterialId);

    /// Range checks (ELM-017, NFR-REL-005) and references to storeys.
    /// Returns the payload with normalised geometry, for example polygons
    /// cleaned and oriented (EDT-018). Errors are message keys
    /// (NFR-L10N-001).
    fn validate(&self, storeys: &StoreyList) -> Result<Self, Message>;

    /// Moves, rotates or mirrors the element in plan (EDT-001…EDT-003).
    /// Every kind supports translations. A kind that cannot represent the
    /// result refuses with [`TransformError::Unsupported`] and stays
    /// unchanged.
    fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError>;

    /// Absolute bottom and top elevations for an element on `home`
    /// (ELM-005, ELM-006). Storey links are resolved against `storeys`, so
    /// linked elements follow storey edits (STR-006). `None` if a storey it
    /// refers to is missing.
    fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)>;

    /// The storeys the element refers to by ID, besides its home storey
    /// (ELM-006).
    fn storey_links(&self) -> Vec<StoreyLinkRef> {
        Vec::new()
    }

    /// Re-expresses the element for a new home storey at
    /// `target_elevation`, keeping its absolute position; storey links become
    /// fixed values (STR-004: moving the elements of a deleted storey).
    /// `None`, leaving the element unchanged, if a storey is missing.
    fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()>;

    /// Re-expresses the element for the new home storey `target`, keeping
    /// its absolute position and its links to other storeys (ELM-005).
    fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        let target_elevation = storeys.get(target)?.elevation;
        self.rehome(home, storeys, target_elevation)
    }

    /// Replaces links to `storey`, which is about to be deleted, by fixed
    /// values that keep the element where it is (STR-004). Returns whether
    /// anything changed.
    fn unlink_storey(&mut self, _home: StoreyId, _storeys: &StoreyList, _storey: StoreyId) -> bool {
        false
    }

    /// The parts of a compound element (IFX-016); empty for simple ones.
    fn parts(&self) -> Vec<PartSpec> {
        Vec::new()
    }

    /// The spatial node the element is placed under (Table 5-1).
    fn container(&self) -> Container {
        Container::Storey
    }

    /// Smallest and largest plan coordinates the element covers. `None` if
    /// it has no points.
    fn plan_bounds(&self) -> Option<(DVec2, DVec2)>;
}

/// Bounding box of plan points, `None` if there are none.
pub(crate) fn plan_bounds_of(points: impl IntoIterator<Item = DVec2>) -> Option<(DVec2, DVec2)> {
    points
        .into_iter()
        .map(|p| (p, p))
        .reduce(|(lo, hi), (l, h)| (lo.min(l), hi.max(h)))
}

/// Generates the registry from one list of
/// `module => Payload (as_kind, as_kind_mut)` entries, each optionally behind
/// attributes such as `#[cfg(…)]`; see the module documentation.
macro_rules! kinds {
    ($($(#[$meta:meta])* $module:ident => $Payload:ident ($as_kind:ident, $as_kind_mut:ident)),+ $(,)?) => {
        $($(#[$meta])* mod $module;)+
        $($(#[$meta])* pub use self::$module::$Payload;)+

        /// The kind-specific parameters of an element (SRS 3.2).
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum ElementKind {
            $($(#[$meta])* $Payload($Payload),)+
        }

        /// An element kind without its parameters: the tool that creates
        /// it (SRS 3.2).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[non_exhaustive]
        pub enum KindTag {
            $($(#[$meta])* $Payload,)+
        }

        impl KindTag {
            /// Every kind of this build, in registry order.
            pub const ALL: &'static [KindTag] = &[$($(#[$meta])* KindTag::$Payload,)+];

            /// The kind's entry in the registry.
            pub fn descriptor(self) -> &'static KindDescriptor {
                match self {
                    $($(#[$meta])* KindTag::$Payload => &<$Payload as Kind>::DESCRIPTOR,)+
                }
            }
        }

        /// The registry: one descriptor per kind, in the order of
        /// [`KindTag::ALL`].
        pub static KINDS: &[&KindDescriptor] = &[$($(#[$meta])* &<$Payload as Kind>::DESCRIPTOR,)+];

        $(
            $(#[$meta])*
            impl From<$Payload> for ElementKind {
                fn from(payload: $Payload) -> Self {
                    ElementKind::$Payload(payload)
                }
            }
        )+

        impl Element {
            $(
                $(#[$meta])*
                #[doc = concat!("The parameters of the element if it is a ", stringify!($module), ".")]
                pub fn $as_kind(&self) -> Option<&$Payload> {
                    match &self.kind {
                        ElementKind::$Payload(payload) => Some(payload),
                        _ => None,
                    }
                }

                $(#[$meta])*
                #[doc = concat!("The parameters of the element, for changing, if it is a ", stringify!($module), ".")]
                pub fn $as_kind_mut(&mut self) -> Option<&mut $Payload> {
                    match &mut self.kind {
                        ElementKind::$Payload(payload) => Some(payload),
                        _ => None,
                    }
                }
            )+
        }

        impl ElementKind {
            pub fn tag(&self) -> KindTag {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(_) => KindTag::$Payload,)+
                }
            }

            /// See [`Kind::material`].
            pub fn material(&self) -> MaterialId {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.material(),)+
                }
            }

            /// See [`Kind::set_material`].
            pub fn set_material(&mut self, material: MaterialId) {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.set_material(material),)+
                }
            }

            /// See [`Kind::validate`].
            pub fn validate(&self, storeys: &StoreyList) -> Result<ElementKind, Message> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.validate(storeys).map(ElementKind::$Payload),)+
                }
            }

            /// See [`Kind::transform`].
            pub fn transform(&mut self, transform: &PlanTransform) -> Result<(), TransformError> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.transform(transform),)+
                }
            }

            /// See [`Kind::vertical_extent`].
            pub fn vertical_extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.vertical_extent(home, storeys),)+
                }
            }

            /// See [`Kind::storey_links`].
            pub fn storey_links(&self) -> Vec<StoreyLinkRef> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.storey_links(),)+
                }
            }

            /// See [`Kind::rehome`].
            pub fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.rehome(home, storeys, target_elevation),)+
                }
            }

            /// See [`Kind::relink`].
            pub fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.relink(home, storeys, target),)+
                }
            }

            /// See [`Kind::unlink_storey`].
            pub fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.unlink_storey(home, storeys, storey),)+
                }
            }

            /// See [`Kind::parts`].
            pub fn parts(&self) -> Vec<PartSpec> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.parts(),)+
                }
            }

            /// See [`Kind::container`].
            pub fn container(&self) -> Container {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.container(),)+
                }
            }

            /// See [`Kind::plan_bounds`].
            pub fn plan_bounds(&self) -> Option<(DVec2, DVec2)> {
                match self {
                    $($(#[$meta])* ElementKind::$Payload(k) => k.plan_bounds(),)+
                }
            }
        }
    };
}

// Every element kind, once. Adding a kind is a new module plus one line here.
kinds! {
    wall => Wall (as_wall, as_wall_mut),
    slab => Slab (as_slab, as_slab_mut),
    #[cfg(feature = "test-kinds")] dummy => Dummy (as_dummy, as_dummy_mut),
}

impl KindTag {
    /// The kind with the file code `code`, if this build has it.
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|k| k.code() == code)
    }

    /// Message key of the kind's label (NFR-L10N-001).
    pub fn label_key(self) -> &'static str {
        self.descriptor().label_key
    }

    /// Message key of the kind's noun in a sentence, taking a `count`
    /// argument (NFR-L10N-001).
    pub fn noun_key(self) -> &'static str {
        self.descriptor().noun_key
    }

    pub fn code(self) -> &'static str {
        self.descriptor().code
    }

    /// Prefix of automatic element names (ELM-019).
    pub fn name_prefix(self) -> &'static str {
        self.descriptor().name_prefix
    }

    /// IFC classes selectable for this kind (ELM-011).
    pub fn ifc_classes(self) -> &'static [IfcClass] {
        self.descriptor().ifc_classes
    }
}

impl ElementKind {
    /// The kind's entry in the registry.
    pub fn descriptor(&self) -> &'static KindDescriptor {
        self.tag().descriptor()
    }
}
