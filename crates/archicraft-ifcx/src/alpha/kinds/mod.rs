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
//! warning, and read as an unsupported node.

use archicraft_model::{ElementKind, KindTag, Project};
use serde_json::{Map, Value};

use super::reader::{Object, Report};
use crate::message::Message;

pub(crate) mod common;

/// What the reader knows when it reads an element's parameters: the project
/// read so far, with its resources and storeys but not yet its elements.
pub(crate) struct ReadContext<'a> {
    pub project: &'a Project,
}

/// The attribute of one element kind (IFX-060, IFX-061), implemented for
/// the model's payload types (a local trait, so the orphan rule allows it).
pub(crate) trait KindCodec: Sized {
    /// The attribute holding the parameters, `archicraft::<kind code>`.
    const ATTRIBUTE: &'static str;

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
    fn schema(&self) -> Value;
    fn slot_schema(&self) -> Vec<(&'static str, Value)>;
    fn write(&self, kind: &ElementKind) -> Option<Map<String, Value>>;
    fn write_slot(&self, kind: &ElementKind) -> Option<Map<String, Value>>;
    fn read(&self, attr: &Object, context: &ReadContext<'_>, report: &mut Report<'_>) -> Result<ElementKind, Message>;
    fn read_slot(&self, attr: &Object, default: &ElementKind, report: &mut Report<'_>) -> Option<ElementKind>;
}

/// [`Codec`] for the payload type `P`, with its kind and its place in the
/// model's [`ElementKind`] from `codecs!`.
struct Erased<P: 'static> {
    tag: KindTag,
    of: fn(&ElementKind) -> Option<&P>,
}

impl<P: KindCodec + Into<ElementKind> + Sync> Codec for Erased<P> {
    fn tag(&self) -> KindTag {
        self.tag
    }

    fn attribute(&self) -> &'static str {
        P::ATTRIBUTE
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
// line here.
codecs! {
    wall => Wall,
    slab => Slab,
}

/// The codec of `kind`, if this build stores the kind.
pub(crate) fn codec(kind: KindTag) -> Option<&'static dyn Codec> {
    CODECS.iter().copied().find(|c| c.tag() == kind)
}

/// Each registered kind with its attribute, in registry order (for tests).
pub fn registered() -> Vec<(KindTag, &'static str)> {
    CODECS.iter().map(|c| (c.tag(), c.attribute())).collect()
}
