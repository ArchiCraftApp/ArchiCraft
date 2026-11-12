//! The IFCX reader on arbitrary bytes, along the application's open path
//! (NFR-REL-007, NFR-SEC-003): `ArchiCraftApp::open_path` loads the file and
//! `Document::opened` evaluates the geometry, with no other validation in
//! between. A loaded project is then saved and loaded again, without its
//! body meshes when they have more than
//! `archicraft_ifcx::fuzz::ROUND_TRIP_MESH_POINTS` points (archicraft-tuy0).
//! None of it may panic, and a rejected file carries a message.
//!
//! `archicraft_ifcx::fuzz::open_path` holds the path, and
//! `crates/archicraft-ifcx/tests/fuzz_regressions.rs` replays it on stable
//! over the committed regression files.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    archicraft_ifcx::fuzz::open_path(data);
});
