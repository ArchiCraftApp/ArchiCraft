//! Writes the reference models as IFCX files into a directory, for golden
//! files and conformance tests (SRS 7.3, NFR-MNT-003).
//!
//! Every model is written twice: compact as `<name>.ifcx` and indented,
//! human-readable as `<name>-indented.ifcx` (IFX-008), so that conformance
//! checks both layouts the application can save. The models are the sample
//! catalogue (`samples::catalogue`), built with seeded identities, so every
//! run writes the same bytes and the compact files equal the golden files.
//!
//! Usage: `cargo run -p archicraft-ifcx --example reference_models -- <dir>`

use std::path::PathBuf;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::samples;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: reference_models <output directory>")?,
    );
    std::fs::create_dir_all(&dir)?;
    let options = WriteOptions {
        author: "ArchiCraft reference models".into(),
        timestamp: "2026-10-16T00:00:00Z".into(),
        data_version: 1,
    };
    for (name, build) in samples::catalogue() {
        let project = build();
        let geometry = ModelGeometry::evaluate(&project);
        for (suffix, indented) in [("", false), ("-indented", true)] {
            let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options, indented);
            for w in warnings {
                eprintln!("{name}{suffix}: {w}");
            }
            let path = dir.join(format!("{name}{suffix}.ifcx"));
            std::fs::write(&path, bytes)?;
            println!("{}", path.display());
        }
    }
    Ok(())
}
