//! Reads a mock successor file by mapping its header to `ifcx_alpha` and
//! delegating to [`alpha::reader`] (IFX-091).

use serde_json::value::RawValue;

use super::IFCX_VERSION;
use crate::alpha;
use crate::container::IfcxFile;
use crate::{Problem, ReadOutcome};

/// Reads a parsed `ifcx_successor_mock` file; [`ReadOutcome::source_version`]
/// names the successor, so that saving converts the file to
/// [`crate::WRITTEN_VERSION`] (IFX-090). The copy of the file holds the
/// same references to the raw attribute values, not copies of them.
pub fn read(file: &IfcxFile<&RawValue>) -> Result<ReadOutcome, Problem> {
    let mut file = file.clone();
    alpha::writer::IFCX_VERSION.clone_into(&mut file.header.ifcx_version);
    let mut outcome = alpha::reader::read(&file)?;
    outcome.source_version = IFCX_VERSION;
    Ok(outcome)
}
