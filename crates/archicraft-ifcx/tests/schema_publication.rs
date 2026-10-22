//! Readiness for publishing the `archicraft::` schema as a standalone IFCX
//! document on ifcx.dev (IFX-092). Publishing itself is a manual step
//! (docs/ifcx-evolution.md); the writer keeps declaring the schema in-file
//! (IFX-005, IFX-061).

#![allow(clippy::unwrap_used)]

use std::path::Path;

use archicraft_ifcx::alpha::schema::{self, FORMAT_VERSION};
use archicraft_ifcx::alpha::writer::{IFCX_VERSION, to_bytes};
use archicraft_ifcx::container::Import;
use archicraft_ifcx::validate::{self, Severity};
use archicraft_ifcx::{IfcxFile, registry};

/// Where the schema would be published; a placeholder until it is.
const PLACEHOLDER_URI: &str = "https://ifcx.dev/@archicraft/archicraft@v1.ifcx";

/// The schema document as the `archicraft_schema` example writes it.
fn schema_document() -> IfcxFile {
    let bytes = to_bytes(&schema::document("2026-10-20T00:00:00Z"), true);
    validate::parse(&bytes).unwrap()
}

fn golden_files() -> Vec<(String, IfcxFile)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|e| e == "ifcx"))
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
        .into_iter()
        .map(|path| {
            let file = validate::parse(&std::fs::read(&path).unwrap()).unwrap();
            (path.display().to_string(), file)
        })
        .collect()
}

/// The golden file with its in-file `archicraft::` declarations replaced by
/// an import of the published schema.
fn importing_the_schema(mut file: IfcxFile) -> IfcxFile {
    file.schemas.retain(|id, _| !id.starts_with("archicraft::"));
    file.imports.push(Import {
        uri: PLACEHOLDER_URI.to_owned(),
        integrity: None,
    });
    file
}

/// IFX-092: the schema document holds exactly the declarations the writer
/// puts into every file, no data and no imports, and is versioned by the
/// ArchiCraft format version.
#[test]
fn ifx_092_the_schema_document_holds_only_the_archicraft_declarations() {
    let document = schema_document();
    assert_eq!(document.schemas, schema::declarations());
    assert!(document.schemas.keys().all(|id| id.starts_with("archicraft::")));
    assert!(document.data.is_empty());
    assert!(document.imports.is_empty());
    assert_eq!(document.header.ifcx_version, IFCX_VERSION);
    assert_eq!(document.header.data_version, FORMAT_VERSION.to_string());
    assert_eq!(document.header.timestamp, "2026-10-20T00:00:00Z");
    let errors: Vec<_> = validate::validate(&document)
        .into_iter()
        .filter(|p| p.severity == Severity::Error)
        .collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

/// IFX-092: a file that imports the schema instead of declaring it validates
/// once the import resolves to the schema document. Without it the
/// `archicraft::` attributes are undeclared, so the import is what declares
/// them.
#[test]
fn ifx_092_a_file_importing_the_published_schema_validates() {
    assert!(registry::bundled(PLACEHOLDER_URI).is_none());
    let document = schema_document();
    for (name, golden) in golden_files() {
        assert!(validate::validate(&golden).is_empty(), "{name}");
        let file = importing_the_schema(golden);

        let problems = validate::validate_with(&file, &[(PLACEHOLDER_URI, &document)]);
        assert!(problems.is_empty(), "{name}: {problems:#?}");

        let unresolved = validate::validate(&file);
        assert!(
            unresolved
                .iter()
                .any(|p| p.severity == Severity::Warning && p.message.to_string().contains(PLACEHOLDER_URI)),
            "{name}: {unresolved:#?}"
        );
        assert!(
            unresolved.iter().any(|p| p.severity == Severity::Error
                && p.message
                    .to_string()
                    .contains(r#"ifcx-attribute-undeclared(id="archicraft::project")"#)),
            "{name}: {unresolved:#?}"
        );
    }
}

/// IFX-092: extra sources only resolve the imports a file names; they do not
/// add declarations to a file that does not import them.
#[test]
fn ifx_092_extra_sources_resolve_only_named_imports() {
    let document = schema_document();
    for (name, golden) in golden_files() {
        let mut file = importing_the_schema(golden);
        file.imports.retain(|import| import.uri != PLACEHOLDER_URI);
        let problems = validate::validate_with(&file, &[(PLACEHOLDER_URI, &document)]);
        assert!(
            problems.iter().any(|p| p.severity == Severity::Error
                && p.message
                    .to_string()
                    .contains(r#"ifcx-attribute-undeclared(id="archicraft::project")"#)),
            "{name}: {problems:#?}"
        );
    }
}
