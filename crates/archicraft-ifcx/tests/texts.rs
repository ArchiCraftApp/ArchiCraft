//! Texts before their IFCX content (ANN-007, ANN-010, PRJ-008). Until texts
//! are stored as entries of `archicraft::annotations` (archicraft-u4vm.2.2),
//! the text kind's placeholder codec keeps them out of the file: no entry,
//! no declaration, no defaults slot and no name counter, so files of
//! projects with texts are those of the same projects without them, and
//! saving says what it left out in one warning instead of dropping it
//! silently. The text styles are not in the file either: the reader takes
//! the template's, which no command changes yet.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::template::default_metric_project;
use archicraft_model::{Arg, Command, History, KindTag, Project, samples};

/// The two-storey shell with a two-line text on the ground storey and one
/// on the upper storey, `TX-001` and `TX-002`.
fn shell_with_texts() -> Project {
    let mut p = samples::two_storey_shell();
    let ground = p.storeys.ground();
    let upper = p.storeys.relative(ground, 1).unwrap().id;
    let kitchen = p.new_text(ground, DVec2::new(2.0, 3.0), "Kitchen\n12 m²", 0);
    let mut bedroom = p.new_text(upper, DVec2::new(7.0, 2.0), "Bedroom", 1);
    bedroom.as_text_mut().unwrap().size_mm = Some(5.0);
    History::default()
        .execute(&mut p, Command::AddElements(vec![kitchen, bedroom]))
        .unwrap();
    p
}

/// The golden file of the two-storey shell and the header options it was
/// written with.
fn golden_shell() -> (Vec<u8>, WriteOptions) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/two-storey-shell.ifcx");
    let bytes = std::fs::read(&path).unwrap();
    let header = validate::parse(&bytes).unwrap().header;
    let options = WriteOptions {
        author: header.author,
        timestamp: header.timestamp,
        data_version: header.data_version.parse().unwrap(),
    };
    (bytes, options)
}

/// ANN-007, PRJ-008: the shell with two texts saves with exactly one
/// warning, which names both texts, and gives the shell's golden file byte
/// for byte: the file holds nothing of the texts or the text styles, so it
/// validates and is the file the gate's conformance run checks (IFX-006).
#[test]
fn texts_are_left_out_with_one_warning_naming_them() {
    let p = shell_with_texts();
    let (golden, options) = golden_shell();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let (bytes, warnings) = archicraft_ifcx::save(&p, &geometry, &options, false);

    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    let warning = &warnings[0];
    assert_eq!((warning.severity, warning.path.as_deref()), (Severity::Warning, None));
    let message = &warning.message;
    assert_eq!(message.key(), "ifcx-kind-not-saved-yet");
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Text)));
    assert_eq!(message.arg("count"), Some(&Arg::Count(2)));
    assert_eq!(message.arg("names"), Some(&Arg::Text("TX-001, TX-002".into())));

    assert!(bytes == golden, "the file differs from the shell's golden file");
    let parsed = validate::parse(&bytes).unwrap();
    assert!(validate::validate(&parsed).is_empty());
}

/// ANN-007: the file reads back as the shell without the texts and without
/// problems: the text tool's defaults are the template's, read without a
/// report, no text was counted, and the text styles are the template's,
/// which are the project's (no command changes them yet).
#[test]
fn a_file_saved_with_texts_reads_back_without_them_or_problems() {
    let p = shell_with_texts();
    let (_, options) = golden_shell();
    let (bytes, _) = archicraft_ifcx::save(&p, &ModelGeometry::evaluate(&p), &options, false);
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let read = loaded.project;
    assert!(read.elements.values().all(|e| e.tag() != KindTag::Text));
    assert_eq!(read.elements.len(), p.elements.len() - 2);
    assert_eq!(
        read.tool_defaults.get(KindTag::Text),
        p.tool_defaults.get(KindTag::Text)
    );
    assert_eq!(read.counters.get(KindTag::Text), 0);
    assert_eq!(read.text_styles, default_metric_project().text_styles);
    assert_eq!(read.text_styles, p.text_styles);
}
