//! Declared element parameters: the fields of the properties panel and the
//! settings dialog (UI-002, UI-003, ELM-001, ELM-017).
//!
//! Each kind lists its parameters as [`FieldSpec`]s in its descriptor
//! ([`KindDescriptor::fields`](crate::kinds::KindDescriptor::fields)), and
//! [`COMMON_FIELDS`] lists those of the element envelope every kind has. A
//! spec is UI-free data (NFR-MNT-002): a stable key, a message key for the
//! label, the dialog section, the value type, the accepted range with the
//! validator's message, and plain function pointers that read, write and
//! enumerate values, so descriptors stay `const`. The specs are meant for
//! one renderer in the application that draws every kind, for the tool
//! defaults and the selection alike (ELM-001), and for other generic code,
//! such as transfer of settings or selection criteria, that matches elements
//! through the same getters.
//!
//! A getter returning `None` hides the field for that element, for example
//! the height of a wall whose top is linked to a storey.
//!
//! # Key vocabulary
//!
//! A key two kinds share names the same parameter, with the same
//! [`FieldType`], so that settings transfer between kinds
//! ([`crate::transfer`], ELM-003) and selection criteria match fields
//! without a table per pair of kinds. Keys are stable: they are part of
//! journals of transfers and of saved criteria. The shared keys are:
//!
//! - envelope ([`COMMON_FIELDS`]): `material`, `layer`, `ifc_class`,
//!   `predefined_type`, `position`, `structural_function`;
//! - construction: `thickness` (walls, slabs; `layer_structure` joins it
//!   with layer structures, PRF-001);
//! - vertical kinds ([`crate::kinds`]' base and top fields, ELM-005,
//!   ELM-006): `base_offset`, `top_mode`, `height`, `top_storey`,
//!   `linked_storey`, `top_offset`;
//! - kind-specific keys, such as `reference_line` and `reference_offset`
//!   (walls), `slab_reference` and `elevation_offset` (slabs), `long_name`
//!   and `number` (spaces), `content`, `text_style`, `size`,
//!   `anchor_position` and `angle` (texts), `x` and `y` (notes). A stair's
//!   rule set joins as one composite field, `rules`.

use crate::element::{Element, IfcClass, Position, PredefinedType, StructuralFunction};
use crate::ids::{LayerId, MaterialId, StoreyId, TextStyleId};
use crate::message::Message;
use crate::project::Project;

/// The settings-dialog section a field belongs to (UI-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    /// Geometry and positioning.
    Geometry,
    /// The kind's own parameters, titled with the kind's label, such as a
    /// space's room name and number or a text's content and style.
    Kind,
    /// Plan display.
    PlanDisplay,
    /// Model and appearance.
    ModelAppearance,
    /// Classification: layer, IFC class, position, structural function.
    Classification,
}

/// The type of a field's value, which picks its widget.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FieldType {
    /// A positive length, such as a thickness or height
    /// ([`FieldValue::Length`]).
    Length,
    /// A signed vertical or plan offset ([`FieldValue::Length`]).
    Elevation,
    /// An angle in radians ([`FieldValue::Angle`]).
    Angle,
    /// A whole number ([`FieldValue::Count`]).
    Count,
    /// A flag ([`FieldValue::Bool`]).
    Bool,
    /// One of the values [`FieldSpec::options`] lists.
    Choice,
    /// A single line of text ([`FieldValue::Text`]).
    Text,
    /// Text of several lines ([`FieldValue::Text`]).
    MultilineText,
    /// A size in paper millimetres, or none for the style's size
    /// ([`FieldValue::OptionalSize`], ANN-007).
    OptionalSize,
}

/// A field's value. Lengths are metres and angles radians (CLAUDE.md).
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Length(f64),
    Angle(f64),
    Count(i64),
    Bool(bool),
    /// The stable code of an enumerated value, such as
    /// [`Position::code`].
    Code(&'static str),
    Storey(StoreyId),
    Layer(LayerId),
    Material(MaterialId),
    TextStyle(TextStyleId),
    Text(String),
    OptionalSize(Option<f64>),
}

/// The label of a choice option.
#[derive(Clone, Debug, PartialEq)]
pub enum OptionLabel {
    /// A message the application translates (NFR-L10N-001).
    Message(Message),
    /// A name the user gave, such as a storey's or a material's.
    Name(String),
}

/// The values a numeric field, or a choice of counts, accepts, inclusive,
/// and the message key the validator rejects other values with (ELM-017,
/// NFR-REL-005). The kind's validator checks the same limits, through the
/// constants of [`crate::validate`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldRange {
    pub min: f64,
    pub max: f64,
    pub message: &'static str,
}

/// Reads a field from an element; `None` hides the field for it.
pub type Getter = fn(&Element, &Project) -> Option<FieldValue>;
/// Writes a value read from the field's widget; a value of another type
/// leaves the element unchanged. Validation runs when a command applies the
/// element.
pub type Setter = fn(&mut Element, &FieldValue, &Project);
/// The options of a choice field for an element, in display order.
pub type Options = fn(&Project, &Element) -> Vec<(FieldValue, OptionLabel)>;

/// Whether settings transfer copies a field (ELM-003, [`crate::transfer`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transfer {
    /// Copied wherever the target has the field.
    Always,
    /// Never copied: the element's own identity or placement, such as a
    /// room's name and number, a text's content or a note's position.
    Never,
    /// Copied only together with the field of this key, which it
    /// qualifies: the predefined type goes with the IFC class (IFX-024).
    With(&'static str),
}

/// One declared parameter (UI-002, UI-003, ELM-017).
#[derive(Clone, Copy, Debug)]
pub struct FieldSpec {
    /// Stable identifier, unique per kind; a key two kinds share names the
    /// same parameter (same type and label).
    pub key: &'static str,
    /// Message key of the label (NFR-L10N-001).
    pub label: &'static str,
    /// Message key of the label while the tool defaults are edited, where
    /// it differs, such as "Default name" for a space's room name.
    pub defaults_label: Option<&'static str>,
    pub section: Section,
    pub field_type: FieldType,
    pub range: Option<FieldRange>,
    /// Shown in the properties strip, not only in the settings dialog.
    pub in_strip: bool,
    /// Shown for placed elements only, not for the tool defaults: a value
    /// each element gets when placed, like a text's content
    /// ([`crate::kinds::Kind::check_complete`]).
    pub placed_only: bool,
    /// Whether settings transfer copies the field (ELM-003).
    pub transfer: Transfer,
    pub get: Getter,
    pub set: Setter,
    /// The options of a [`FieldType::Choice`] field; `None` for others.
    pub options: Option<Options>,
}

impl FieldSpec {
    /// A field shown in the strip and the dialog, without range or options.
    pub const fn new(
        key: &'static str,
        label: &'static str,
        section: Section,
        field_type: FieldType,
        get: Getter,
        set: Setter,
    ) -> Self {
        Self {
            key,
            label,
            defaults_label: None,
            section,
            field_type,
            range: None,
            in_strip: true,
            placed_only: false,
            transfer: Transfer::Always,
            get,
            set,
            options: None,
        }
    }

    /// A [`FieldType::Choice`] field over `options`.
    pub const fn choice(
        key: &'static str,
        label: &'static str,
        section: Section,
        get: Getter,
        set: Setter,
        options: Options,
    ) -> Self {
        Self {
            options: Some(options),
            ..Self::new(key, label, section, FieldType::Choice, get, set)
        }
    }

    /// The field with the range `min..=max`, whose violations the validator
    /// reports as `message` (ELM-017).
    pub const fn ranged(self, min: f64, max: f64, message: &'static str) -> Self {
        Self {
            range: Some(FieldRange { min, max, message }),
            ..self
        }
    }

    /// The field with the label `label` while the tool defaults are
    /// edited, such as "Default name" for a space's room name.
    pub const fn with_defaults_label(self, label: &'static str) -> Self {
        Self {
            defaults_label: Some(label),
            ..self
        }
    }

    /// The field shown for placed elements only, not for the tool defaults,
    /// like a text's content.
    pub const fn placed_only(self) -> Self {
        Self {
            placed_only: true,
            ..self
        }
    }

    /// The field that settings transfer never copies ([`Transfer::Never`]).
    pub const fn not_transferred(self) -> Self {
        Self {
            transfer: Transfer::Never,
            ..self
        }
    }

    /// The field that settings transfer copies only together with the field
    /// `key` ([`Transfer::With`]).
    pub const fn transferred_with(self, key: &'static str) -> Self {
        Self {
            transfer: Transfer::With(key),
            ..self
        }
    }

    /// Whether `value` lies in the field's range; true without one.
    pub fn in_range(&self, value: f64) -> bool {
        self.range
            .is_none_or(|r| value.is_finite() && (r.min..=r.max).contains(&value))
    }

    /// The validator's explanation of the range (NFR-REL-005), with the
    /// limits as lengths, angles or counts.
    pub fn range_error(&self) -> Option<Message> {
        let r = self.range?;
        let message = Message::new(r.message);
        Some(match self.field_type {
            FieldType::Angle => message.angle("min", r.min).angle("max", r.max),
            // A ranged choice offers counts, like the linked storey.
            FieldType::Count | FieldType::Choice => message.count("max", r.max as usize),
            _ => message.length("min", r.min).length("max", r.max),
        })
    }
}

/// The fields of `element`: its kind's, then [`COMMON_FIELDS`].
pub fn fields_of(element: &Element) -> impl Iterator<Item = &'static FieldSpec> {
    element.kind.descriptor().fields.iter().chain(COMMON_FIELDS)
}

/// The code standing for no predefined type in the `predefined_type` field
/// (IFX-024). Predefined type codes are upper case, so it never collides.
pub const NO_PREDEFINED_TYPE: &str = "none";

/// The fields of the element envelope, which every kind has: material (for
/// kinds built of one), layer, IFC class (ELM-011), predefined type
/// (IFX-024), position (ELM-012) and structural function (ELM-020).
pub static COMMON_FIELDS: &[FieldSpec] = &[
    FieldSpec::choice(
        "material",
        "model-field-material",
        Section::ModelAppearance,
        |e, _| e.kind.material().map(FieldValue::Material),
        |e, v, _| {
            if let FieldValue::Material(m) = v {
                e.kind.set_material(*m);
            }
        },
        |p, _| {
            p.materials
                .iter()
                .map(|m| (FieldValue::Material(m.id), OptionLabel::Name(m.name.clone())))
                .collect()
        },
    ),
    FieldSpec::choice(
        "layer",
        "model-field-layer",
        Section::Classification,
        |e, _| Some(FieldValue::Layer(e.layer)),
        |e, v, _| {
            if let FieldValue::Layer(l) = v {
                e.layer = *l;
            }
        },
        |p, _| {
            p.layers
                .iter()
                .map(|l| (FieldValue::Layer(l.id), OptionLabel::Name(l.name.clone())))
                .collect()
        },
    ),
    FieldSpec::choice(
        "ifc_class",
        "model-field-ifc-class",
        Section::Classification,
        |e, _| Some(FieldValue::Code(e.ifc_class.code())),
        |e, v, _| {
            if let FieldValue::Code(code) = v
                && let Some(class) = IfcClass::from_code(code)
            {
                e.ifc_class = class;
                // IFX-024: a type the new class does not offer is dropped.
                if e.predefined_type
                    .is_some_and(|t| !e.tag().predefined_types(class).contains(&t))
                {
                    e.predefined_type = None;
                }
            }
        },
        |_, e| {
            e.tag()
                .ifc_classes()
                .iter()
                .map(|c| (FieldValue::Code(c.code()), message(c.label_key())))
                .collect()
        },
    ),
    FieldSpec::choice(
        "predefined_type",
        "model-field-predefined-type",
        Section::Classification,
        // Shown only when the class offers types.
        |e, _| {
            (!e.tag().predefined_types(e.ifc_class).is_empty())
                .then(|| FieldValue::Code(e.predefined_type.map_or(NO_PREDEFINED_TYPE, PredefinedType::code)))
        },
        |e, v, _| {
            if let FieldValue::Code(code) = v {
                e.predefined_type =
                    PredefinedType::from_code(code).filter(|t| e.tag().predefined_types(e.ifc_class).contains(t));
            }
        },
        |_, e| {
            let offered = e.tag().predefined_types(e.ifc_class);
            std::iter::once((
                FieldValue::Code(NO_PREDEFINED_TYPE),
                message("model-predefined-type-none"),
            ))
            .chain(
                offered
                    .iter()
                    .map(|t| (FieldValue::Code(t.code()), message(t.label_key()))),
            )
            .collect()
        },
    )
    .transferred_with("ifc_class"),
    FieldSpec::choice(
        "position",
        "model-field-position",
        Section::Classification,
        |e, _| Some(FieldValue::Code(e.position.code())),
        |e, v, _| {
            if let FieldValue::Code(code) = v
                && let Some(p) = Position::from_code(code)
            {
                e.position = p;
            }
        },
        |_, _| {
            Position::ALL
                .iter()
                .map(|p| (FieldValue::Code(p.code()), message(p.label_key())))
                .collect()
        },
    ),
    FieldSpec::choice(
        "structural_function",
        "model-field-structural-function",
        Section::Classification,
        |e, _| Some(FieldValue::Code(e.structural.code())),
        |e, v, _| {
            if let FieldValue::Code(code) = v
                && let Some(s) = StructuralFunction::from_code(code)
            {
                e.structural = s;
            }
        },
        |_, _| {
            StructuralFunction::ALL
                .iter()
                .map(|s| (FieldValue::Code(s.code()), message(s.label_key())))
                .collect()
        },
    ),
];

/// An option label from a message key.
pub(crate) fn message(key: &'static str) -> OptionLabel {
    OptionLabel::Message(Message::new(key))
}

/// The `Length` of a field value, for setters.
pub(crate) fn length(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Length(v) => Some(*v),
        _ => None,
    }
}

/// The `Code` of a field value, for setters.
pub(crate) fn code(value: &FieldValue) -> Option<&'static str> {
    match value {
        FieldValue::Code(c) => Some(c),
        _ => None,
    }
}

/// The storeys of the project, highest first, as choice options.
pub(crate) fn storey_options(project: &Project, _: &Element) -> Vec<(FieldValue, OptionLabel)> {
    project
        .storeys
        .highest_first()
        .map(|s| (FieldValue::Storey(s.id), OptionLabel::Name(s.name.clone())))
        .collect()
}
