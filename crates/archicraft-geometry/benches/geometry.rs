//! Geometry benches on every bench model (NFR-PERF-001): evaluation, plan
//! generation, slicing and the headless part of an edit (NFR-PERF-006).
//! Bench ids are `geometry/<bench>/<model>`; docs/performance/README.md
//! lists them.
//!
//! Inputs are built and checked outside the timed closures: a model whose
//! elements fail to evaluate would make a bench look fast.

use std::hint::black_box;

use archicraft_geometry::slice::slice_loops;
use archicraft_geometry::{ModelGeometry, PlanDrawing, ViewRange, plan_drawing};
use archicraft_model::samples::bench_models;
use archicraft_model::{Command, ElementId, History, Position, Project, glam::DVec2};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};

fn models() -> Vec<(&'static str, Project)> {
    bench_models().into_iter().map(|m| (m.id, (m.build)())).collect()
}

fn evaluated(id: &str, project: &Project) -> ModelGeometry {
    let geometry = ModelGeometry::evaluate(project);
    assert!(geometry.problems.is_empty(), "{id}: {:?}", geometry.problems);
    assert_eq!(geometry.elements.len(), project.elements.len(), "{id}");
    geometry
}

/// The plans of every storey, as the plan views draw them.
fn plans(project: &Project, geometry: &ModelGeometry) -> Vec<PlanDrawing> {
    project
        .storeys
        .as_slice()
        .iter()
        .map(|s| plan_drawing(project, geometry, s.id, ViewRange::for_storey(project, s.id), |_| true))
        .collect()
}

fn evaluate(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry/evaluate");
    for (id, project) in models() {
        evaluated(id, &project);
        group.bench_function(id, |b| b.iter(|| ModelGeometry::evaluate(black_box(&project))));
    }
    group.finish();
}

/// Slice and classification of every storey's plan.
fn plan(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry/plan");
    for (id, project) in models() {
        let geometry = evaluated(id, &project);
        let drawn: usize = plans(&project, &geometry).iter().map(|p| p.elements.len()).sum();
        assert_eq!(drawn, project.elements.len(), "{id}: every element is drawn");
        group.bench_function(id, |b| b.iter(|| plans(black_box(&project), black_box(&geometry))));
    }
    group.finish();
}

/// `slice_loops` on every wall mesh at its storey's cut plane.
fn slice(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry/slice");
    for (id, project) in models() {
        let geometry = evaluated(id, &project);
        let cuts: Vec<_> = project
            .elements
            .values()
            .filter(|e| e.as_wall().is_some())
            .flat_map(|e| {
                let z = project.cut_plane_height(e.storey);
                geometry.elements[&e.id].parts.iter().map(move |p| (&p.mesh, z))
            })
            .collect();
        assert!(!cuts.is_empty(), "{id} has walls");
        for (mesh, z) in &cuts {
            assert!(!slice_loops(mesh, *z).is_empty(), "{id}: every wall is cut");
        }
        group.bench_function(id, |b| {
            b.iter(|| {
                cuts.iter()
                    .map(|(mesh, z)| slice_loops(black_box(mesh), *z).len())
                    .sum::<usize>()
            })
        });
    }
    group.finish();
}

/// Moves the first interior wall along its T junction and updates the 3D
/// bodies and the plan of its storey: the headless part of NFR-PERF-006.
fn edit(project: &mut Project, wall: ElementId) -> (ModelGeometry, PlanDrawing) {
    let command = Command::MoveElements {
        ids: vec![wall],
        delta: DVec2::new(0.5, 0.0),
    };
    if let Err(e) = History::default().execute(project, command) {
        panic!("the bench edit is valid: {e}");
    }
    let geometry = ModelGeometry::evaluate(project);
    let storey = project.elements[&wall].storey;
    let plan = plan_drawing(
        project,
        &geometry,
        storey,
        ViewRange::for_storey(project, storey),
        |_| true,
    );
    (geometry, plan)
}

fn edit_then_evaluate(c: &mut Criterion) {
    let mut group = c.benchmark_group("geometry/edit_then_evaluate");
    for (id, project) in models() {
        let wall = project
            .elements
            .values()
            .find(|e| e.position == Position::Interior && e.as_wall().is_some())
            .map(|e| e.id)
            .unwrap_or_else(|| panic!("{id} has an interior wall"));
        let (geometry, _) = edit(&mut project.clone(), wall);
        assert!(geometry.problems.is_empty(), "{id}: {:?}", geometry.problems);
        group.bench_function(id, |b| {
            b.iter_batched(
                || project.clone(),
                |mut p| {
                    let updated = edit(&mut p, wall);
                    // Returned, so dropping the project is not timed.
                    (p, updated)
                },
                BatchSize::LargeInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, evaluate, plan, slice, edit_then_evaluate);
criterion_main!(benches);
