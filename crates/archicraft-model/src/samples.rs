//! Sample projects, built through commands like a user would build them. They
//! serve as reference models for golden-file and conformance tests (SRS 7.3)
//! and as a starting point in the application.

use glam::DVec2;

use crate::command::{Command, History};
use crate::element::{KindTag, Position};
use crate::ids::ElementId;
use crate::kinds::ReferenceLine;
use crate::project::Project;
use crate::template::default_metric_project;

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

/// Adds a closed ring of joined exterior walls, counter-clockwise.
fn add_wall_ring(project: &mut Project, history: &mut History, storey: crate::StoreyId, corners: &[DVec2]) {
    let walls = (0..corners.len())
        .map(|i| project.new_wall(storey, corners[i], corners[(i + 1) % corners.len()], i as u32))
        .collect();
    execute(project, history, Command::AddElements(walls));
}

fn execute(project: &mut Project, history: &mut History, command: Command) {
    if let Err(e) = history.execute(project, command) {
        unreachable!("sample commands are valid: {e}");
    }
}

/// The exit criterion of SRS increment 0: one storey with four exterior
/// walls joined at the corners and a floor slab.
pub fn storey_with_walls_and_slab() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    project.info.name = "Walls and slab".into();
    let ground = project.storeys.ground();
    let outline = rectangle(DVec2::ZERO, DVec2::new(10.0, 8.0));
    add_wall_ring(&mut project, &mut history, ground, &outline);
    let slab = project.new_slab(ground, outline, Vec::new(), 0);
    execute(&mut project, &mut history, Command::AddElements(vec![slab]));
    project
}

/// A two-storey shell: exterior walls on both storeys, an interior wall in a
/// T junction, a ground slab and an upper floor slab with a stair opening.
pub fn two_storey_shell() -> Project {
    let mut project = default_metric_project();
    let mut history = History::default();
    project.info.name = "Two-storey shell".into();
    let ground = project.storeys.ground();
    let upper = project.storeys.as_slice()[1].id;
    let outline = rectangle(DVec2::ZERO, DVec2::new(12.0, 9.0));

    for storey in [ground, upper] {
        add_wall_ring(&mut project, &mut history, storey, &outline);
        // Interior wall from the south wall's reference line northwards.
        let mut interior = project.new_wall(storey, DVec2::new(5.0, 0.0), DVec2::new(5.0, 6.0), 0);
        interior.position = Position::Interior;
        interior.layer = project
            .layers
            .iter()
            .find(|l| l.name == "Interior walls")
            .map_or(interior.layer, |l| l.id);
        if let Some(wall) = interior.as_wall_mut() {
            wall.thickness = 0.115;
            wall.reference_line = ReferenceLine::Center;
            if let Some(m) = project.materials.iter().find(|m| m.name == "Sand-lime brick") {
                wall.material = m.id;
            }
        }
        execute(&mut project, &mut history, Command::AddElements(vec![interior]));
    }

    let ground_slab = project.new_slab(ground, outline.clone(), Vec::new(), 0);
    let stair_hole = rectangle(DVec2::new(6.0, 1.0), DVec2::new(9.0, 2.2));
    let upper_slab = project.new_slab(upper, outline, vec![stair_hole], 1);
    execute(
        &mut project,
        &mut history,
        Command::AddElements(vec![ground_slab, upper_slab]),
    );
    project
}

/// Distance between the origins of neighbouring tiles of [`tiled_shell`],
/// in metres. The 12 m x 9 m shell leaves gaps of 8 m and 11 m, so no
/// junction forms between tiles.
pub const TILE_PITCH: f64 = 20.0;

/// Tiles per side of the `tiled_shell` bench model: 9 x 9 shells of 12
/// elements give 972 elements.
pub const BENCH_TILES: usize = 9;

/// [`two_storey_shell`] repeated on an `n` x `n` grid with [`TILE_PITCH`]
/// spacing, built by copying its walls and slabs through `AddElements`.
///
/// Provisional bench model (NFR-PERF-001): it stands in for the seeded
/// 1,000/5,000/20,000-element models of `element-kind-framework` and for
/// RM-M/RM-L (SRS 7.3), which replace it.
pub fn tiled_shell(n: usize) -> Project {
    let mut project = two_storey_shell();
    let mut history = History::default();
    project.info.name = format!("Tiled shell {n} x {n}");
    let tile: Vec<_> = project.elements.values().cloned().collect();
    for (i, j) in (0..n).flat_map(|i| (0..n).map(move |j| (i, j))).skip(1) {
        let offset = DVec2::new(i as f64, j as f64) * TILE_PITCH;
        let (mut walls, mut slabs) = (0, 0);
        let copies = tile
            .iter()
            .map(|element| {
                let mut copy = element.clone();
                copy.id = ElementId::new();
                copy.kind.translate(offset);
                let nth = match copy.tag() {
                    KindTag::Wall => &mut walls,
                    KindTag::Slab => &mut slabs,
                };
                copy.name = project.next_element_name(copy.tag(), *nth);
                *nth += 1;
                copy
            })
            .collect();
        execute(&mut project, &mut history, Command::AddElements(copies));
    }
    project
}

/// A model the performance benches run on (NFR-PERF-001).
#[derive(Clone, Copy, Debug)]
pub struct BenchModel {
    /// Stable id, the last part of every bench id
    /// (`ifcx/save/two_storey_shell`): the regression gate compares by id.
    pub id: &'static str,
    /// Whether the CI regression gate runs it; RM-M and RM-L will not.
    pub ci: bool,
    pub build: fn() -> Project,
}

/// The `tiled_shell` bench model: [`tiled_shell`] with [`BENCH_TILES`].
pub fn tiled_shell_bench() -> Project {
    tiled_shell(BENCH_TILES)
}

/// The bench models, the single extension point for larger models: the
/// seeded stand-ins and RM-M/RM-L are appended here (`ci: false` for those
/// too slow for CI).
pub fn bench_models() -> Vec<BenchModel> {
    vec![
        BenchModel {
            id: "two_storey_shell",
            ci: true,
            build: two_storey_shell,
        },
        BenchModel {
            id: "tiled_shell",
            ci: true,
            build: tiled_shell_bench,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_are_valid() {
        let p = storey_with_walls_and_slab();
        assert_eq!(p.elements.len(), 5);
        let p = two_storey_shell();
        assert_eq!(p.elements.len(), 12);
        assert!(p.integrity_problems().is_empty());
    }

    /// NFR-PERF-001: the bench models, in registry order, with their element
    /// counts; both run in CI.
    #[test]
    fn bench_models_are_registered_with_their_sizes() {
        let models = bench_models();
        let ids: Vec<&str> = models.iter().map(|m| m.id).collect();
        assert_eq!(ids, ["two_storey_shell", "tiled_shell"]);
        assert!(models.iter().all(|m| m.ci), "{models:?}");
        let counts: Vec<usize> = models.iter().map(|m| (m.build)().elements.len()).collect();
        assert_eq!(counts, [12, 972]);
    }

    /// The tiled shell repeats the two-storey shell on an n x n grid with
    /// unique element names, and stays valid.
    #[test]
    fn tiled_shell_repeats_the_shell_on_a_grid() {
        let p = tiled_shell(2);
        assert_eq!(p.elements.len(), 4 * 12);
        assert!(p.integrity_problems().is_empty());
        let mut names: Vec<&str> = p.elements.values().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), p.elements.len(), "element names are unique");
        let max_x = p
            .elements
            .values()
            .filter_map(|e| e.as_wall())
            .map(|w| w.start.x.max(w.end.x))
            .fold(f64::MIN, f64::max);
        assert_eq!(max_x, TILE_PITCH + 12.0);
        assert_eq!(tiled_shell(1).elements.len(), 12);
    }
}
