//! IFCX benches on every bench model (NFR-PERF-001): the save and open
//! quantities of NFR-PERF-002. Bench ids are `ifcx/<bench>/<model>`;
//! docs/performance/README.md lists them.
//!
//! Inputs (evaluated geometry, file bytes) are built and checked outside the
//! timed closures, so a model that fails to save or load cannot look fast.

use std::hint::black_box;

use archicraft_geometry::ModelGeometry;
use archicraft_ifcx::{WriteOptions, validate};
use archicraft_model::Project;
use archicraft_model::samples::bench_models_to_run;
use criterion::{Criterion, criterion_group, criterion_main};

/// Fixed options, so every run writes the same bytes.
fn options() -> WriteOptions {
    WriteOptions {
        author: "ArchiCraft benches".into(),
        timestamp: "2026-10-20T00:00:00Z".into(),
        data_version: 1,
    }
}

/// Each bench model with its evaluated geometry and its compact file.
fn models() -> Vec<(&'static str, Project, ModelGeometry, Vec<u8>)> {
    bench_models_to_run()
        .into_iter()
        .map(|m| {
            let project = (m.build)();
            let geometry = ModelGeometry::evaluate(&project);
            assert!(geometry.problems.is_empty(), "{}: {:?}", m.id, geometry.problems);
            let (bytes, warnings) = archicraft_ifcx::save(&project, &geometry, &options(), false);
            assert!(warnings.is_empty(), "{}: {warnings:?}", m.id);
            (m.id, project, geometry, bytes)
        })
        .collect()
}

/// Write and serialise: compact (`save`) and indented (`save_indented`).
fn save(c: &mut Criterion) {
    for (bench, pretty) in [("ifcx/save", false), ("ifcx/save_indented", true)] {
        let mut group = c.benchmark_group(bench);
        for (id, project, geometry, _) in models() {
            group.bench_function(id, |b| {
                b.iter(|| archicraft_ifcx::save(black_box(&project), black_box(&geometry), &options(), pretty))
            });
        }
        group.finish();
    }
}

/// Parse and read the compact file into a project.
fn load(c: &mut Criterion) {
    let mut group = c.benchmark_group("ifcx/load");
    for (id, project, _, bytes) in models() {
        let loaded = archicraft_ifcx::load(&bytes).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        assert!(loaded.problems.is_empty(), "{id}: {:?}", loaded.problems);
        assert_eq!(loaded.project.elements.len(), project.elements.len(), "{id}");
        group.bench_function(id, |b| b.iter(|| archicraft_ifcx::load(black_box(&bytes))));
    }
    group.finish();
}

/// Schema and structure validation of the parsed file.
fn validate(c: &mut Criterion) {
    let mut group = c.benchmark_group("ifcx/validate");
    for (id, _, _, bytes) in models() {
        let file = validate::parse(&bytes).unwrap_or_else(|e| panic!("{id}: {e:?}"));
        let problems = validate::validate(&file);
        assert!(problems.is_empty(), "{id}: {problems:?}");
        group.bench_function(id, |b| b.iter(|| validate::validate(black_box(&file))));
    }
    group.finish();
}

criterion_group!(benches, save, load, validate);
criterion_main!(benches);
