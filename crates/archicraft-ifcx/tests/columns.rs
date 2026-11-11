//! Columns before their IFCX content (COL-001, PRJ-008). Until columns are
//! stored as `IfcColumn` nodes (archicraft-i4gl.3), the column kind's
//! placeholder codec keeps them out of the file: no node, no declaration, no
//! defaults slot and no name counter, so files of projects with columns are
//! those of the same projects without them, and saving says what it left
//! out in one warning instead of dropping it silently.

#![allow(clippy::unwrap_used)]

use std::path::Path;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::CrossSection;
use archicraft_model::{Arg, Command, History, KindTag, Project, samples};

/// The two-storey shell with a column on each storey, `C-001` and `C-002`,
/// one of them circular and turned.
fn shell_with_columns() -> Project {
    let mut p = samples::two_storey_shell();
    let ground = p.storeys.ground();
    let upper = p.storeys.relative(ground, 1).unwrap().id;
    let hall = p.new_column(ground, DVec2::new(4.0, 3.0), 0.0, 0);
    let mut landing = p.new_column(upper, DVec2::new(2.0, 2.0), 0.5, 1);
    let column = landing.as_column_mut().unwrap();
    column.section = CrossSection::Circular { diameter: 0.4 };
    column.link.top = archicraft_model::TopLink::Fixed { height: 2.6 };
    History::default()
        .execute(&mut p, Command::AddElements(vec![hall, landing]))
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

/// COL-001, PRJ-008: the shell with two columns saves with exactly one
/// warning, which names both columns, and gives the shell's golden file byte
/// for byte: the file holds nothing of the columns, their tool's defaults or
/// their counter, so it validates and is the file the gate's conformance run
/// checks (IFX-006).
#[test]
fn columns_are_left_out_with_one_warning_naming_them() {
    let p = shell_with_columns();
    let (golden, options) = golden_shell();
    let geometry = ModelGeometry::evaluate(&p);
    assert!(geometry.problems.is_empty(), "{:?}", geometry.problems);
    let (bytes, warnings) = archicraft_ifcx::save(&p, &geometry, &options, false);

    assert_eq!(warnings.len(), 1, "{warnings:#?}");
    let warning = &warnings[0];
    assert_eq!((warning.severity, warning.path.as_deref()), (Severity::Warning, None));
    let message = &warning.message;
    assert_eq!(message.key(), "ifcx-kind-not-saved-yet");
    assert_eq!(message.arg("kind"), Some(&Arg::Kind(KindTag::Column)));
    assert_eq!(message.arg("count"), Some(&Arg::Count(2)));
    assert_eq!(message.arg("names"), Some(&Arg::Text("C-001, C-002".into())));

    assert!(bytes == golden, "the file differs from the shell's golden file");
    let parsed = validate::parse(&bytes).unwrap();
    assert!(validate::validate(&parsed).is_empty());
}

/// COL-001: the file reads back as the shell without the columns and
/// without problems: the column tool's defaults are the template's, read
/// without a report, and no column was counted.
#[test]
fn a_file_saved_with_columns_reads_back_without_them_or_problems() {
    let p = shell_with_columns();
    let (_, options) = golden_shell();
    let (bytes, _) = archicraft_ifcx::save(&p, &ModelGeometry::evaluate(&p), &options, false);
    let loaded = archicraft_ifcx::load(&bytes).unwrap();
    assert!(loaded.problems.is_empty(), "{:#?}", loaded.problems);
    let read = loaded.project;
    assert!(read.elements.values().all(|e| e.tag() != KindTag::Column));
    assert_eq!(read.elements.len(), p.elements.len() - 2);
    assert_eq!(
        read.tool_defaults.get(KindTag::Column),
        p.tool_defaults.get(KindTag::Column)
    );
    assert_eq!(read.counters.get(KindTag::Column), 0);
}
