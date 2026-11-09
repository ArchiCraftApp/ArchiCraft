//! Spaces before their IFCX content (SPC-001, PRJ-008). Until spaces are
//! stored as `IfcSpace` (archicraft-awj), the space kind's placeholder codec
//! keeps them out of the file: no element node, no declaration, no defaults
//! slot and no name counter, so files of projects with spaces are those of
//! the same projects without them, and saving says what it left out in one
//! warning instead of dropping it silently.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::{Arg, Command, History, KindTag, Project, samples};

/// The two-storey shell with a space in the ground room and one on the
/// upper storey, `SP-001` and `SP-002`.
fn shell_with_spaces() -> Project {
    let mut p = samples::two_storey_shell();
    let ground = p.storeys.ground();
    let upper = p.storeys.relative(ground, 1).unwrap().id;
    let room = |x0: f64, x1: f64| {
        vec![
            DVec2::new(x0, 1.0),
            DVec2::new(x1, 1.0),
            DVec2::new(x1, 5.0),
            DVec2::new(x0, 5.0),
        ]
    };
    let mut kitchen = p.new_space(ground, room(1.0, 4.0), Vec::new(), 0);
    kitchen.as_space_mut().unwrap().long_name = "Kitchen".into();
    let bedroom = p.new_space(upper, room(6.0, 10.0), Vec::new(), 1);
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

/// SPC-001, PRJ-008: the shell with two spaces saves with exactly one
/// warning, which names both spaces, and gives the shell's golden file byte
/// for byte: the file holds nothing of the spaces, so it validates and is
/// the file the gate's conformance run checks (IFX-006).
#[test]
fn spaces_are_left_out_with_one_warning_naming_them() {
    let p = shell_with_spaces();
    let (golden, options) = golden_shell();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let (bytes, warnings) = archicraft_ifcx::save(&p, &geometry, &options, false);

    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    let warning = &warnings[0];
    assert_eq!((warning.severity, warning.path.as_deref()), (Severity::Warning, None));
    let message = &warning.message;
    assert_eq!(message.key(), "ifcx-kind-not-saved-yet");
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Space)));
    assert_eq!(message.arg("count"), Some(&Arg::Count(2)));
    assert_eq!(message.arg("names"), Some(&Arg::Text("SP-001, SP-002".into())));

    assert!(bytes == golden, "the file differs from the shell's golden file");
    let parsed = validate::parse(&bytes).unwrap();
    assert!(validate::validate(&parsed).is_empty());
}

/// SPC-001: the file reads back as the shell without the spaces and
/// without problems: the space tool's defaults are the template's, read
/// without a report, and no space was counted.
#[test]
fn a_file_saved_with_spaces_reads_back_without_them_or_problems() {
    let p = shell_with_spaces();
    let (_, options) = golden_shell();
    let (bytes, _) = archicraft_ifcx::save(&p, &ModelGeometry::evaluate(&p), &options, false);
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let read = loaded.project;
    assert!(read.elements.values().all(|e| e.tag() != KindTag::Space));
    assert_eq!(read.elements.len(), p.elements.len() - 2);
    assert_eq!(
        read.tool_defaults.get(KindTag::Space),
        p.tool_defaults.get(KindTag::Space)
    );
    assert_eq!(read.counters.get(KindTag::Space), 0);
}
