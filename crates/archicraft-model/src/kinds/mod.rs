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
//! and the methods of [`ElementKind`] that delegate to the hooks.

use serde::{Deserialize, Serialize};

use crate::element::{Element, IfcClass};
use crate::ids::MaterialId;
use crate::storey::StoreyList;

pub use self::slab::SlabReference;
pub use self::wall::ReferenceLine;

/// Static description of an element kind (ELM-011, ELM-019).
#[derive(Debug, PartialEq, Eq)]
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
    /// cleaned and oriented (EDT-018).
    fn validate(&self, storeys: &StoreyList) -> Result<Self, String>;
}

/// Generates the registry from one list of `module => Payload (as_kind)`
/// entries; see the module documentation.
macro_rules! kinds {
    ($($module:ident => $Payload:ident ($as_kind:ident)),+ $(,)?) => {
        $(mod $module;)+
        $(pub use self::$module::$Payload;)+

        /// The kind-specific parameters of an element (SRS 3.2).
        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        pub enum ElementKind {
            $($Payload($Payload),)+
        }

        /// An element kind without its parameters: the tool that creates
        /// it (SRS 3.2).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        pub enum KindTag {
            $($Payload,)+
        }

        impl KindTag {
            /// Every kind, in registry order.
            pub const ALL: &'static [KindTag] = &[$(KindTag::$Payload,)+];

            /// The kind's entry in the registry.
            pub fn descriptor(self) -> &'static KindDescriptor {
                match self {
                    $(KindTag::$Payload => &<$Payload as Kind>::DESCRIPTOR,)+
                }
            }
        }

        /// The registry: one descriptor per kind, in the order of
        /// [`KindTag::ALL`].
        pub static KINDS: &[&KindDescriptor] = &[$(&<$Payload as Kind>::DESCRIPTOR,)+];

        $(
            impl From<$Payload> for ElementKind {
                fn from(payload: $Payload) -> Self {
                    ElementKind::$Payload(payload)
                }
            }
        )+

        impl Element {
            $(
                #[doc = concat!("The parameters of the element if it is a ", stringify!($module), ".")]
                pub fn $as_kind(&self) -> Option<&$Payload> {
                    match &self.kind {
                        ElementKind::$Payload(payload) => Some(payload),
                        _ => None,
                    }
                }
            )+
        }

        impl ElementKind {
            pub fn tag(&self) -> KindTag {
                match self {
                    $(ElementKind::$Payload(_) => KindTag::$Payload,)+
                }
            }

            /// See [`Kind::material`].
            pub fn material(&self) -> MaterialId {
                match self {
                    $(ElementKind::$Payload(k) => k.material(),)+
                }
            }

            /// See [`Kind::set_material`].
            pub fn set_material(&mut self, material: MaterialId) {
                match self {
                    $(ElementKind::$Payload(k) => k.set_material(material),)+
                }
            }

            /// See [`Kind::validate`].
            pub fn validate(&self, storeys: &StoreyList) -> Result<ElementKind, String> {
                match self {
                    $(ElementKind::$Payload(k) => k.validate(storeys).map(ElementKind::$Payload),)+
                }
            }
        }
    };
}

// Every element kind, once. Adding a kind is a new module plus one line here.
kinds! {
    wall => Wall (as_wall),
    slab => Slab (as_slab),
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
