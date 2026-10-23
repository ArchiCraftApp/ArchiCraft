//! The English catalogue covers every message key archicraft-geometry
//! emits, with the arguments the crate passes, and geometry problems render
//! as English text with the model's reason in place (NFR-L10N-001, UI-012).

#![allow(clippy::unwrap_used)]

mod catalogue_check;

use std::collections::BTreeSet;

use archicraft_app::i18n::{self, Localizer};
use archicraft_geometry::GeometryError;
use archicraft_geometry::message::ALL_KEYS;
use archicraft_model::units::{LengthUnit, NumberFormat, UnitSettings};
use archicraft_model::{KindTag, Message};
use catalogue_check::assert_catalogue_matches;

/// A representative message for every key in `ALL_KEYS`, with the arguments
/// archicraft-geometry passes.
fn representatives() -> Vec<Message> {
    let mut messages: Vec<Message> = [
        "geometry-no-height",
        "geometry-home-storey-missing",
        "geometry-linked-storey-missing",
        "geometry-evaluation-panicked",
        "geometry-polygon-too-few-points",
        "geometry-polygon-not-finite",
        "geometry-polygon-coordinate-range",
        "geometry-polygon-duplicate-point",
        "geometry-polygon-self-intersection",
        "geometry-polygon-no-area",
    ]
    .map(Message::new)
    .to_vec();
    messages.extend([
        Message::new("geometry-invalid-polygon").nested("reason", Message::new("geometry-polygon-no-area")),
        Message::new("geometry-kind-unsupported").kind("kind", KindTag::Wall),
    ]);
    messages
}

/// NFR-L10N-001: every key archicraft-geometry emits is in geometry.ftl,
/// geometry.ftl has no other keys, and each message uses exactly the
/// arguments the crate passes.
#[test]
fn geometry_ftl_matches_the_geometry_keys_and_arguments() {
    assert_catalogue_matches("en/geometry.ftl", ALL_KEYS, &representatives(), |_| BTreeSet::new());
}

/// Renders `message` in English with lengths in millimetres.
fn english(message: &Message) -> String {
    let units = UnitSettings {
        length: LengthUnit::Millimeter,
        ..UnitSettings::default()
    };
    let nf = NumberFormat {
        decimal_separator: '.',
        grouping: Some(','),
    };
    let localizer = std::rc::Rc::new(Localizer::english());
    let text = i18n::with(std::rc::Rc::clone(&localizer), || i18n::render(message, &units, &nf));
    assert!(localizer.reported_keys().is_empty(), "{:?}", localizer.reported_keys());
    text
}

/// UI-012, NFR-L10N-001: each error renders as English text, the model's
/// reason for an invalid outline and the kind's noun in place.
#[test]
fn geometry_errors_render_in_english() {
    let cases = [
        (
            GeometryError::InvalidPolygon(Message::new("model-error-polygon-self-intersection")),
            "the outline is invalid: The polygon outline intersects itself.",
        ),
        (
            GeometryError::InvalidPolygon(Message::new("geometry-polygon-no-area")),
            "the outline is invalid: it has no area",
        ),
        (GeometryError::InvalidExtent, "the element has no height"),
        (GeometryError::MissingHomeStorey, "the home storey is missing"),
        (GeometryError::MissingLinkedStorey, "the linked storey is missing"),
        (
            GeometryError::UnsupportedKind(KindTag::Slab),
            "elements of the kind “slab” have no geometry yet",
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(english(&error.message()), expected, "{error}");
    }
}
