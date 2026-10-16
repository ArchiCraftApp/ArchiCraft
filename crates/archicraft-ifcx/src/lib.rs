//! IFCX (IFC5) reading and writing for ArchiCraft (SRS Section 5.2).
//!
//! IFCX is ArchiCraft's native and only file format (D-1, D-11). The file
//! holds standard IFCX content (spatial tree, classes, names, evaluated meshes,
//! materials) that any IFCX consumer understands, plus the full parametric
//! model in the declared `archicraft::` namespace.
//!
//! The format-specific code lives in versioned adapter modules ([`alpha`]),
//! isolated from the domain model and geometry (NFR-MNT-004).

pub mod alpha;
pub mod container;
pub mod json;
pub mod registry;
pub mod validate;

use archicraft_geometry::ModelGeometry;
use archicraft_model::Project;

pub use alpha::reader::ReadOutcome;
pub use alpha::writer::{WriteOptions, WriteOutcome};
pub use container::IfcxFile;
pub use validate::{Problem, Severity};

/// The IFCX version this release writes (IFX-090).
pub const WRITTEN_VERSION: &str = alpha::writer::IFCX_VERSION;

/// Serialises a project with its evaluated geometry. Returns the file bytes
/// and warnings (for example elements saved without a body).
pub fn save(
    project: &Project,
    geometry: &ModelGeometry,
    options: &WriteOptions,
    pretty: bool,
) -> (Vec<u8>, Vec<Problem>) {
    let outcome = alpha::writer::write(project, geometry, options);
    (alpha::writer::to_bytes(&outcome.file, pretty), outcome.warnings)
}

/// Reads a project from IFCX bytes. Every IFCX version that an ArchiCraft
/// release has written is read here (IFX-090).
pub fn load(bytes: &[u8]) -> Result<ReadOutcome, Problem> {
    let file = validate::parse(bytes)?;
    match file.header.ifcx_version.as_str() {
        alpha::writer::IFCX_VERSION => alpha::reader::read(&file),
        other => Err(Problem::error(
            None,
            format!("the IFCX version “{other}” is not supported; this release reads {WRITTEN_VERSION}"),
        )),
    }
}
