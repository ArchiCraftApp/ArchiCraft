//! The `archicraft::` attribute codecs of the element kinds (SRS 3.2,
//! IFX-060, IFX-061).
//!
//! Each kind's parameters are stored in one attribute of their own
//! (`archicraft::wall`, `archicraft::slab`, …) next to the common
//! `archicraft::element`. A kind's codec declares that attribute, writes and
//! reads it, and does the same for the kind-specific members of the kind's
//! tool-defaults slot (ELM-001). [`CODECS`] lists the codecs in registry
//! order; the schema, the writer and the reader ask the table instead of
//! matching on kinds, so a new kind is one module here plus one line in the
//! `codecs!` invocation below, next to its line in the model's `kinds!`
//! list. A kind without a codec is saved without its parameters, with a
//! warning, and read as an unsupported node. A kind this format version
//! cannot store yet has a placeholder codec ([`KindCodec::PLACEHOLDER`]):
//! its elements, defaults slot and counter are left out of the file, with one
//! warning per kind naming the elements (PRJ-008). The elements of a kind
//! that is not an IFCX node (texts, markers, dimensions; ANN-010) are
//! entries of the storey's `archicraft::annotations` instead ([`annotations`]),
//! with the codec's members under the kind code.
//!
//! A file declares the attributes of the kinds the project stores
//! ([`present`]), not of every codec this build has: the model's test kinds
//! and their codecs exist only in test builds (the cargo feature `test-kinds`,
//! enabled through the dev-dependency on this crate), and a project without
//! them must be written the same by a test build and a release build.
//!
//! Every field of the model has a stored member: [`KindCodec::FIELDS`] maps
//! each payload's serde fields and [`PROJECT_FIELDS`] the project's to where
//! the file keeps them, and a test compares the tables with the serde forms,
//! so a field added to the model fails the test until the file stores it
//! (IFX-060).

use archicraft_model::{ElementKind, KindTag, Project};
use serde_json::{Map, Value};

use super::reader::{Object, Report};
use crate::message::Message;

pub mod annotations;
pub(crate) mod common;

/// What the reader knows when it reads an element's parameters: the project
/// read so far, with its resources and storeys but not yet its elements.
pub(crate) struct ReadContext<'a> {
    pub project: &'a Project,
}

/// Where a type's serde fields are stored: each field's name with the
/// members of the attribute that hold it (IFX-060).
pub type Fields = &'static [(&'static str, &'static [&'static str])];

/// The attribute of one element kind (IFX-060, IFX-061), implemented for
/// the model's payload types (a local trait, so the orphan rule allows it).
pub(crate) trait KindCodec: Sized {
    /// The attribute holding the parameters, `archicraft::<kind code>`. A
    /// kind that is not an IFCX node has no attribute of its own: its
    /// elements' annotation entries hold the members under the kind code
    /// ([`annotations`]).
    const ATTRIBUTE: &'static str;

    /// Every serde field of the payload with the members of
    /// [`Self::ATTRIBUTE`] that store it.
    const FIELDS: Fields;

    /// True for a kind that exists only in test builds: the published schema
    /// document leaves it out (IFX-092), because the example that writes the
    /// document is built with the test kinds like every test.
    const TEST_ONLY: bool = false;

    /// True for a kind whose elements may have no material (SPC-001,
    /// ANN-007): the material member of its defaults slot is then declared
    /// optional and written only when the slot has one.
    const OPTIONAL_MATERIAL: bool = false;

    /// True for a kind this format version cannot store yet, like spaces
    /// (SPC-001) until their IFCX content exists: the writer leaves its
    /// elements, its defaults slot and its counter out of the file and warns
    /// once with the names of the elements it left out (PRJ-008), the file
    /// declares nothing for it ([`present`], [`published`]), and the reader
    /// takes the template's slot and counts its elements, without a report.
    /// Such a codec has no [`Self::FIELDS`].
    const PLACEHOLDER: bool = false;

    /// The value description of [`Self::ATTRIBUTE`] (IFX-061).
    fn schema() -> Value;

    /// The attribute's members, in the order they are written.
    fn write(&self) -> Map<String, Value>;

    /// The parameters from the attribute's members (IFX-083). `Err` skips
    /// the element with the reason; a member with a sensible default is
    /// repaired on `report` instead.
    fn read(attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<Self, Message>;

    /// The kind-specific members of the tool's defaults slot (ELM-001):
    /// the parameters without the placeholder geometry. The common members
    /// (material, layer, position, IFC class, predefined type) follow them.
    fn slot_schema() -> Vec<(&'static str, Value)>;

    fn write_slot(&self) -> Map<String, Value>;

    /// The slot's parameters, with `default`'s values for missing or invalid
    /// members, reported, and its placeholder geometry.
    fn read_slot(attr: &Object, default: &Self, report: &mut Report<'_>) -> Self;
}

/// The object-safe form of a [`KindCodec`], one per kind in [`CODECS`]. The
/// methods that take an [`ElementKind`] answer `Some` only for the codec's
/// own kind, so a caller finds the codec with `find_map`.
pub(crate) trait Codec: Sync {
    fn tag(&self) -> KindTag;
    fn attribute(&self) -> &'static str;
    fn fields(&self) -> Fields;
    fn test_only(&self) -> bool;
    fn optional_material(&self) -> bool;
    fn placeholder(&self) -> bool;
    fn schema(&self) -> Value;
    fn slot_schema(&self) -> Vec<(&'static str, Value)>;
    fn write(&self, kind: &ElementKind) -> Option<Map<String, Value>>;
    fn write_slot(&self, kind: &ElementKind) -> Option<Map<String, Value>>;
    fn read(&self, attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<ElementKind, Message>;
    fn read_slot(&self, attr: &Object, default: &ElementKind, report: &mut Report<'_>) -> Option<ElementKind>;
}

/// [`Codec`] for the payload type `P`, with its kind and its place in the
/// model's [`ElementKind`] from `codecs!`. It holds a tag and a function
/// pointer, so it is `Sync` whatever `P` is.
struct Erased<P: 'static> {
    tag: KindTag,
    of: fn(&ElementKind) -> Option<&P>,
}

impl<P: KindCodec + Into<ElementKind>> Codec for Erased<P> {
    fn tag(&self) -> KindTag {
        self.tag
    }

    fn attribute(&self) -> &'static str {
        P::ATTRIBUTE
    }

    fn fields(&self) -> Fields {
        P::FIELDS
    }

    fn test_only(&self) -> bool {
        P::TEST_ONLY
    }

    fn optional_material(&self) -> bool {
        P::OPTIONAL_MATERIAL
    }

    fn placeholder(&self) -> bool {
        P::PLACEHOLDER
    }

    fn schema(&self) -> Value {
        P::schema()
    }

    fn slot_schema(&self) -> Vec<(&'static str, Value)> {
        P::slot_schema()
    }

    fn write(&self, kind: &ElementKind) -> Option<Map<String, Value>> {
        (self.of)(kind).map(KindCodec::write)
    }

    fn write_slot(&self, kind: &ElementKind) -> Option<Map<String, Value>> {
        (self.of)(kind).map(KindCodec::write_slot)
    }

    fn read(&self, attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<ElementKind, Message> {
        P::read(attr, context, report).map(Into::into)
    }

    fn read_slot(&self, attr: &Object, default: &ElementKind, report: &mut Report<'_>) -> Option<ElementKind> {
        (self.of)(default).map(|d| P::read_slot(attr, d, report).into())
    }
}

/// Generates [`CODECS`] from one list of `module => Payload` entries, each
/// optionally behind attributes such as `#[cfg(…)]`: the module holds the
/// payload's [`KindCodec`] implementation; the payload is named through
/// `archicraft_model::kinds`, so an entry needs no import.
macro_rules! codecs {
    ($($(#[$meta:meta])* $module:ident => $Payload:ident),+ $(,)?) => {
        $($(#[$meta])* mod $module;)+

        /// The codec of every kind this build stores, in registry order.
        pub(crate) static CODECS: &[&dyn Codec] = &[$(
            $(#[$meta])*
            &Erased::<archicraft_model::kinds::$Payload> {
                tag: KindTag::$Payload,
                of: |kind| match kind {
                    ElementKind::$Payload(payload) => Some(payload),
                    _ => None,
                },
            },
        )+];
    };
}

// Every kind with an attribute, once. Adding a kind is a new module plus one
// line here. The test kinds' lines are compiled only with the feature their
// kinds need (see the module documentation).
codecs! {
    wall => Wall,
    slab => Slab,
    space => Space,
    #[cfg(feature = "test-kinds")] dummy => Dummy,
    #[cfg(feature = "test-kinds")] note => Note,
}

/// The codec of `kind`, if this build stores the kind.
pub(crate) fn codec(kind: KindTag) -> Option<&'static dyn Codec> {
    CODECS.iter().copied().find(|c| c.tag() == kind)
}

/// Whether `kind`'s codec is a [`KindCodec::PLACEHOLDER`]: the file
/// stores nothing of the kind yet.
pub(crate) fn placeholder(kind: KindTag) -> bool {
    codec(kind).is_some_and(|c| c.placeholder())
}

/// The kinds `project` stores, in registry order: those with an element, a
/// tool-defaults slot or a non-zero name counter, without the kinds whose
/// codec is a placeholder. The file declares the attributes, the kind
/// enumeration options, the defaults slots and the counters of these kinds
/// and no others (IFX-061), so what a file declares follows from the
/// project, not from the codecs this build happens to have.
pub fn present(project: &Project) -> Vec<KindTag> {
    KindTag::ALL
        .iter()
        .copied()
        .filter(|&kind| {
            !placeholder(kind)
                && (project.tool_defaults.get(kind).is_some()
                    || project.counters.get(kind) != 0
                    || project.elements.values().any(|e| e.tag() == kind))
        })
        .collect()
}

/// The kinds a release build stores, in registry order: every registered
/// kind that is neither [`KindCodec::TEST_ONLY`] nor a
/// [`KindCodec::PLACEHOLDER`]. The published schema document declares these
/// (IFX-092).
pub fn published() -> Vec<KindTag> {
    CODECS
        .iter()
        .filter(|c| !c.test_only() && !c.placeholder())
        .map(|c| c.tag())
        .collect()
}

/// The registered kinds whose codec is a [`KindCodec::PLACEHOLDER`], in
/// registry order (for tests).
pub fn placeholders() -> Vec<KindTag> {
    CODECS.iter().filter(|c| c.placeholder()).map(|c| c.tag()).collect()
}

/// Each registered kind with its attribute, in registry order (for tests).
pub fn registered() -> Vec<(KindTag, &'static str)> {
    CODECS.iter().map(|c| (c.tag(), c.attribute())).collect()
}

/// Each registered kind with its [`KindCodec::FIELDS`], in registry order
/// (for tests).
pub fn fields() -> Vec<(KindTag, Fields)> {
    CODECS.iter().map(|c| (c.tag(), c.fields())).collect()
}

/// Where the file keeps a field of the project (IFX-060).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stored {
    /// The project node: its path, `bsi::ifc::prop::Name` and
    /// `bsi::ifc::prop::Description`.
    Root,
    /// The nodes whose child key starts with this prefix (`Site`,
    /// `Building`, `Storey`, `Material`): their path and their `bsi::` and
    /// `archicraft::` attributes.
    Node(&'static str),
    /// The element nodes: `archicraft::element` and the attribute of the
    /// kind's codec, whose [`KindCodec::FIELDS`] map the payload.
    Elements,
    /// The entries of `archicraft::annotations` on the storey nodes, for the
    /// elements of the kinds that are not IFCX nodes (ANN-010): the envelope
    /// and the codec's members under the kind code, which
    /// [`KindCodec::FIELDS`] map ([`annotations`]).
    Annotations,
    /// A member of `archicraft::project` on the project node, as a path such
    /// as `counters.group`.
    Member(&'static str),
    /// The element nodes' `archicraft::relations`, each explicit relation
    /// on the node of its first end ([`super::relations`], SRS 3.3), or in
    /// the annotation entry of its first end under `relations`.
    Relations,
    /// A field of the file header.
    Header(&'static str),
}

/// Where the file keeps each top-level serde field of
/// [`archicraft_model::Project`] (IFX-060). `revision` is `#[serde(skip)]`
/// and not stored. A field without an entry fails the test that compares
/// this table with the project's serde form.
pub const PROJECT_FIELDS: &[(&str, &[Stored])] = &[
    ("id", &[Stored::Root]),
    ("info", &[Stored::Member("info"), Stored::Root]),
    ("settings", &[Stored::Member("settings")]),
    ("site", &[Stored::Node("Site"), Stored::Member("location")]),
    ("building", &[Stored::Node("Building")]),
    ("storeys", &[Stored::Node("Storey"), Stored::Member("groundStorey")]),
    ("layers", &[Stored::Member("layers")]),
    ("materials", &[Stored::Member("materials"), Stored::Node("Material")]),
    ("elements", &[Stored::Elements, Stored::Annotations]),
    ("groups", &[Stored::Member("groups")]),
    ("relations", &[Stored::Relations]),
    ("tool_defaults", &[Stored::Member("toolDefaults")]),
    ("views", &[Stored::Member("views")]),
    ("counters", &[Stored::Member("counters")]),
    ("group_counter", &[Stored::Member("counters.group")]),
    ("data_version", &[Stored::Header("dataVersion")]),
];
