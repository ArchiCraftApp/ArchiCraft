//! Writes the reference models as IFCX files into a directory, for golden
//! files and conformance tests (SRS 7.3, NFR-MNT-003).
//!
//! Usage: `cargo run -p archicraft-ifcx --example reference_models -- [--indented] <dir>`
//!
//! `--indented` writes indented, human-readable IFCX (IFX-008) as
//! `<name>-indented.ifcx`, so that conformance also checks that layout.

use std::path::PathBuf;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use archicraft_model::samples;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let usage = "usage: reference_models [--indented] <output directory>";
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let indented = args.first().is_some_and(|a| a == "--indented");
    if indented {
        args.remove(0);
    }
    let [dir] = args.as_slice() else {
        return Err(usage.into());
    };
    let dir = PathBuf::from(dir);
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
        let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options, indented);
        for w in warnings {
            eprintln!("{name}: {w}");
        }
        let suffix = if indented { "-indented" } else { "" };
        let path = dir.join(format!("{name}{suffix}.ifcx"));
        std::fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    Ok(())
}
