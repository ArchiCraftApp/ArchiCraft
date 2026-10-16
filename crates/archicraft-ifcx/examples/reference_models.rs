//! Writes the reference models as IFCX files into a directory, for golden
//! files and conformance tests (SRS 7.3, NFR-MNT-003).
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
    for (name, project) in [
        ("walls-and-slab", samples::storey_with_walls_and_slab()),
        ("two-storey-shell", samples::two_storey_shell()),
    ] {
        let geometry = ModelGeometry::evaluate(&project);
        let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options, false);
        for w in warnings {
            eprintln!("{name}: {w}");
        }
        let path = dir.join(format!("{name}.ifcx"));
        std::fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    Ok(())
}
