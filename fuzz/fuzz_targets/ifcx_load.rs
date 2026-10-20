//! The IFCX reader on arbitrary bytes, along the application's open path
//! (NFR-REL-007, NFR-SEC-003): `ArchiCraftApp::open_path` loads the file and
//! `Document::opened` evaluates the geometry, with no other validation in
//! between. A loaded project is then saved and loaded again. None of it may
//! panic, and a rejected file carries a message.
//!
//! `crates/archicraft-ifcx/tests/fuzz_regressions.rs` replays the same path
//! on stable over the committed regression files.

#![no_main]

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::WriteOptions;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    match archicraft_ifcx::load(data) {
        Ok(outcome) => {
            let geometry = ModelGeometry::evaluate(&outcome.project);
            let options = WriteOptions {
                author: "Fuzz".into(),
                timestamp: "2026-10-20T00:00:00Z".into(),
                data_version: 1,
            };
            let (saved, _warnings) = archicraft_ifcx::save(&outcome.project, &geometry, &options, false);
            let _ = archicraft_ifcx::load(&saved);
        }
        Err(problem) => assert!(!problem.message.is_empty(), "rejected without a message"),
    }
});
