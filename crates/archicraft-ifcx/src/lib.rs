//! IFCX (IFC5) reading and writing for ArchiCraft (SRS Section 5.2).
//!
//! IFCX is ArchiCraft's native and only file format (D-1, D-11). The file
//! holds standard IFCX content (spatial tree, classes, names, evaluated meshes,
//! materials) that any IFCX consumer understands, plus the full parametric
//! model in the declared `archicraft::` namespace.
//!
//! The format-specific code lives in versioned adapter modules ([`alpha`]),
//! isolated from the domain model and geometry (NFR-MNT-004). [`load`] picks
//! the adapter from the table [`ADAPTERS`] by the file's `ifcxVersion`; each
//! adapter migrates files of earlier ArchiCraft format versions before
//! reading them ([`alpha::migrate`], IFX-091). The cargo feature
//! `successor-stub` adds a mock successor version (`successor`) that proves
//! the table takes a second IFCX version; docs/ifcx-evolution.md is the
//! runbook for adopting a real one.

pub mod alpha;
pub mod container;
pub mod json;
pub mod registry;
#[cfg(feature = "successor-stub")]
pub mod successor;
pub mod validate;

use std::path::Path;

use archicraft_geometry::ModelGeometry;
use archicraft_model::Project;
use serde_json::value::RawValue;

pub use alpha::reader::ReadOutcome;
pub use alpha::writer::{WriteOptions, WriteOutcome};
pub use container::IfcxFile;
pub use validate::{Problem, Severity};

/// The IFCX version this release writes (IFX-090).
pub const WRITTEN_VERSION: &str = alpha::writer::IFCX_VERSION;

/// Serialises a project with its evaluated geometry. Returns the file bytes
/// and warnings (for example elements saved without a body). The file is
/// written one node at a time, so saving needs little memory beyond the
/// bytes it returns (NFR-SEC-003).
pub fn save(
    project: &Project,
    geometry: &ModelGeometry,
    options: &WriteOptions,
    pretty: bool,
) -> (Vec<u8>, Vec<Problem>) {
    alpha::writer::save(project, geometry, options, pretty)
}

/// The reader of one IFCX version (IFX-090).
#[derive(Debug)]
pub struct Adapter {
    /// The `header.ifcxVersion` this adapter reads.
    pub version: &'static str,
    /// Reads a parsed file of that version, its attribute values raw JSON
    /// text ([`validate::parse_raw`]); [`ReadOutcome::source_version`] names
    /// the adapter's version.
    pub read: fn(&IfcxFile<&RawValue>) -> Result<ReadOutcome, Problem>,
}

/// Every IFCX version an ArchiCraft release has written, and its reader
/// (IFX-090). A new IFCX version adds an entry and keeps the old ones, so
/// that existing project files keep opening (IFX-091). With the feature
/// `successor-stub` the mock successor follows `ifcx_alpha`.
pub const ADAPTERS: &[Adapter] = &[
    Adapter {
        version: alpha::writer::IFCX_VERSION,
        read: alpha::reader::read,
    },
    #[cfg(feature = "successor-stub")]
    Adapter {
        version: successor::IFCX_VERSION,
        read: successor::reader::read,
    },
];

/// Reads a project from IFCX bytes. Every IFCX version that an ArchiCraft
/// release has written is read here (IFX-090). The attribute values stay
/// raw JSON text borrowed from `bytes` until the reader reads them, one
/// attribute at a time, so that opening a file needs little memory beyond
/// its bytes (NFR-SEC-003).
pub fn load(bytes: &[u8]) -> Result<ReadOutcome, Problem> {
    let file = validate::parse_raw(bytes)?;
    let version = file.header.ifcx_version.as_str();
    let Some(adapter) = ADAPTERS.iter().find(|a| a.version == version) else {
        let supported: Vec<&str> = ADAPTERS.iter().map(|a| a.version).collect();
        return Err(Problem::error(
            None,
            format!(
                "the IFCX version “{version}” is not supported; this release reads {}",
                supported.join(", ")
            ),
        ));
    };
    (adapter.read)(&file)
}

/// Reads a project file, checking its size against the 2 GB limit of
/// Appendix D.9 before reading it (NFR-SEC-003).
pub fn load_path(path: &Path) -> Result<ReadOutcome, Problem> {
    load_path_with_limit(path, validate::MAX_FILE_SIZE as u64)
}

/// [`load_path`] with another size limit, so that callers can test the
/// limit without 2 GB files.
pub fn load_path_with_limit(path: &Path, limit: u64) -> Result<ReadOutcome, Problem> {
    let cannot_read = |e: std::io::Error| Problem::error(None, format!("the file cannot be read: {e}"));
    let file = std::fs::File::open(path).map_err(cannot_read)?;
    // A pipe or device has no length; an empty length is no promise either.
    let declared_len = file
        .metadata()
        .ok()
        .filter(|m| m.is_file() && m.len() > 0)
        .map(|m| m.len());
    let bytes = validate::read_bounded(file, declared_len, limit)?;
    load(&bytes)
}
