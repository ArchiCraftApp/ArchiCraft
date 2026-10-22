//! The English catalogue covers every message key archicraft-ifcx emits,
//! with the arguments the crate passes, and the problems of a damaged file
//! render as English sentences (NFR-L10N-001, PRJ-008).

#![allow(clippy::unwrap_used)]

mod catalogue_check;

use std::collections::BTreeSet;
use std::path::Path;

use archicraft_app::i18n::{self, Localizer};
use archicraft_ifcx::message::ALL_KEYS;
use archicraft_model::units::{LengthUnit, NumberFormat, UnitSettings};
use archicraft_model::{KindTag, Message};
use catalogue_check::assert_catalogue_matches;

/// A representative message for every key in `ALL_KEYS`, with the arguments
/// archicraft-ifcx passes.
fn representatives() -> Vec<Message> {
    let reason = || Message::new("ifcx-not-object");
    let key = |k| Message::new(k).text("key", "thickness");
    let text = |k, name| Message::new(k).text(name, "W-001");
    let count = |k, name| Message::new(k).count(name, 2);
    let nested = |k, name| Message::new(k).nested(name, reason());
    let mut messages: Vec<Message> = [
        "ifcx-schema-too-deep",
        "ifcx-schema-array-restrictions",
        "ifcx-schema-blob",
        "ifcx-composition-too-deep",
        "ifcx-composition-cycle",
        "ifcx-record-project",
        "ifcx-record-project-settings",
        "ifcx-record-project-location",
        "ifcx-record-project-information",
        "ifcx-record-tool-defaults",
        "ifcx-record-counters",
        "ifcx-record-site",
        "ifcx-record-building",
        "ifcx-point-not-list",
        "ifcx-point-not-numeric",
        "ifcx-point-coordinates",
        "ifcx-points-not-list",
        "ifcx-not-object",
        "ifcx-path-not-uuid",
        "ifcx-grid-spacings",
        "ifcx-format-version-invalid",
        "ifcx-tool-material-missing",
        "ifcx-tool-layer-missing",
        "ifcx-storey-listed-twice",
        "ifcx-no-storeys",
        "ifcx-ground-storey-undefined",
        "ifcx-element-data-not-object",
        "ifcx-element-layer-missing",
        "ifcx-no-project",
        "ifcx-project-not-object",
        "ifcx-no-site",
        "ifcx-no-building",
        "ifcx-no-views",
        "ifcx-no-materials",
        "ifcx-no-layers",
        "ifcx-data-version-invalid",
    ]
    .map(Message::new)
    .to_vec();
    messages.extend([
        Message::new("ifcx-problem-at")
            .text("path", "p")
            .nested("message", reason()),
        Message::new("ifcx-schema-unreadable")
            .text("id", "a::b")
            .text("detail", "EOF"),
        Message::new("ifcx-import-offline").text("uri", "https://example.com"),
        Message::new("ifcx-schema-unknown-inherited").text("id", "a::b"),
        Message::new("ifcx-value-type")
            .text("expected", "boolean")
            .text("found", "string"),
        Message::new("ifcx-value-not-option")
            .text("value", "x")
            .text("options", "a, b"),
        Message::new("ifcx-value-key-missing").text("key", "code"),
        Message::new("ifcx-value-in-key")
            .text("key", "code")
            .nested("reason", reason()),
        Message::new("ifcx-attribute-undeclared").text("id", "a::b"),
        Message::new("ifcx-attribute-invalid")
            .text("id", "a::b")
            .nested("reason", reason()),
        Message::new("ifcx-detail").text("detail", "EOF"),
        Message::new("ifcx-unknown-node").text("target", "p"),
        count("ifcx-roots", "count"),
        Message::new("ifcx-file-too-large").nested("limit", Message::new("ifcx-size-gb").count("size", 2)),
        count("ifcx-size-gb", "size"),
        count("ifcx-size-mb", "size"),
        count("ifcx-size-kb", "size"),
        count("ifcx-size-bytes", "size"),
        Message::new("ifcx-file-unreadable").text("detail", "denied"),
        count("ifcx-file-nested-too-deep", "max"),
        Message::new("ifcx-file-invalid").text("detail", "EOF"),
        count("ifcx-file-too-many-imports", "max"),
        Message::new("ifcx-version-unsupported")
            .text("version", "ifcx_omega")
            .text("supported", "ifcx_alpha"),
        count("ifcx-migration-unknown", "version"),
        Message::new("ifcx-migration-failed")
            .count("version", 1)
            .text("detail", "EOF"),
        Message::new("ifcx-migrated")
            .count("from", 1)
            .count("to", 2)
            .nested("change", reason()),
        Message::new("ifcx-skipped")
            .nested("record", Message::new("ifcx-record-project"))
            .nested("reason", reason()),
        Message::new("ifcx-repaired")
            .nested("record", Message::new("ifcx-record-project"))
            .nested("reason", reason()),
        count("ifcx-record-material", "index"),
        count("ifcx-record-layer", "index"),
        count("ifcx-record-view", "index"),
        text("ifcx-record-storey", "name"),
        text("ifcx-record-element", "name"),
        nested("ifcx-default-used", "reason"),
        nested("ifcx-left-out", "reason"),
        nested("ifcx-value-used", "reason").count("value", 3),
        key("ifcx-field-missing"),
        key("ifcx-field-not-number"),
        key("ifcx-field-not-integer"),
        key("ifcx-field-not-text"),
        key("ifcx-field-not-boolean"),
        key("ifcx-field-not-object"),
        key("ifcx-field-not-list"),
        key("ifcx-field-unknown-value").text("value", "door"),
        key("ifcx-field-not-uuid"),
        key("ifcx-field-out-of-range"),
        key("ifcx-field-point").nested("reason", Message::new("ifcx-point-not-list")),
        key("ifcx-field-colour-component"),
        key("ifcx-field-not-colour"),
        text("ifcx-no-name", "name"),
        text("ifcx-material-corrected", "name").nested("reason", Message::new("model-error-material-name")),
        key("ifcx-out-of-range-replaced").count("value", 8),
        count("ifcx-newer-format", "version"),
        nested("ifcx-ground-storey-repaired", "reason"),
        nested("ifcx-template-defaults-used", "reason"),
        nested("ifcx-counters-from-elements", "reason").text("counts", "wall: 3, slab: 1"),
        key("ifcx-counter-negative").count("value", 3),
        key("ifcx-tool-unknown"),
        nested("ifcx-tool-template-used", "reason"),
        Message::new("ifcx-tool-class-not-allowed")
            .text("class", "IfcSlab")
            .kind("kind", KindTag::Wall)
            .text("default", "IfcWall"),
        Message::new("ifcx-storey-no-data")
            .length("elevation", 0.0)
            .length("height", 3.0),
        Message::new("ifcx-storey-height-min").length("min", 0.01),
        text("ifcx-storey-moved-up", "name"),
        Message::new("ifcx-storeys-unreadable").nested("reason", Message::new("model-error-no-storeys")),
        text("ifcx-storey-lost", "name"),
        text("ifcx-element-class-unsupported", "default"),
        text("ifcx-element-material-missing", "name"),
        Message::new("ifcx-parameters-missing").kind("kind", KindTag::Wall),
        nested("ifcx-holes-left-out", "reason"),
        Message::new("ifcx-kind-unreadable").kind("kind", KindTag::Slab),
        text("ifcx-foreign-node", "name"),
        text("ifcx-duplicate", "name"),
        Message::new("ifcx-tool-defaults-not-saved").kind("kind", KindTag::Slab),
        text("ifcx-saved-without-body", "name"),
        text("ifcx-kind-not-storable", "name").kind("kind", KindTag::Slab),
    ]);
    messages
}

/// NFR-L10N-001: every key archicraft-ifcx emits is in ifcx.ftl, ifcx.ftl has
/// no other keys, and each message uses exactly the arguments the crate
/// passes (a count also as `<name>_text`).
#[test]
fn ifcx_ftl_matches_the_ifcx_keys_and_arguments() {
    assert_catalogue_matches("en/ifcx.ftl", ALL_KEYS, &representatives(), |_| BTreeSet::new());
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

/// PRJ-008, NFR-L10N-001: the reports of a damaged file render as English
/// sentences, records, reasons and the model's errors nested in place,
/// lengths in the project unit.
#[test]
fn load_problems_render_in_english() {
    let fixture = |name: &str| {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../archicraft-ifcx/tests/corrupt/{name}.ifcx"));
        archicraft_ifcx::load_path(&path).unwrap()
    };
    let texts = |name: &str| -> Vec<String> { fixture(name).problems.iter().map(|p| english(&p.message)).collect() };
    let invalid_wall = texts("invalid-wall");
    assert!(
        invalid_wall
            .iter()
            .any(|t| t.starts_with("element W-002 skipped: Thickness must be between ")),
        "{invalid_wall:#?}"
    );
    let layer = texts("non-object-layer");
    assert!(
        layer.contains(&"layer record 1 skipped: it is not an object".to_owned()),
        "{layer:#?}"
    );
    assert!(
        layer.contains(&"attribute “archicraft::project”: layers: expected object, found number".to_owned()),
        "{layer:#?}"
    );
    let counters = texts("out-of-range-counters");
    assert!(
        counters.contains(&"counters repaired: “wall” is out of range; 4 is used".to_owned()),
        "{counters:#?}"
    );
    let storey = texts("storey-below-minimum-height");
    assert!(
        storey.contains(&"storey 01 First floor repaired: “height” is below the minimum; 10.000 mm is used".to_owned()),
        "{storey:#?}"
    );
    for name in ["invalid-wall", "non-object-layer", "out-of-range-counters"] {
        for problem in fixture(name).problems {
            let located = english(&problem.located());
            let path = problem.path.as_deref().unwrap_or_default();
            assert!(located.starts_with(&format!("[{path}] ")), "{located}");
        }
    }
}

/// NFR-L10N-001: the file-level problems name their limits with the user's
/// number format.
#[test]
fn file_problems_render_in_english() {
    let too_large = Message::new("ifcx-file-too-large").nested("limit", Message::new("ifcx-size-gb").count("size", 2));
    assert_eq!(english(&too_large), "the file is larger than the limit of 2 GB");
    let bytes = Message::new("ifcx-size-bytes").count("size", 1500);
    assert_eq!(english(&bytes), "1,500 bytes");
    assert_eq!(
        english(
            &Message::new("ifcx-version-unsupported")
                .text("version", "ifcx_omega")
                .text("supported", "ifcx_alpha")
        ),
        "the IFCX version “ifcx_omega” is not supported; this release reads ifcx_alpha"
    );
}
