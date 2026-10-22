//! Sample projects, built through commands like a user would build them. They
//! serve as reference models for golden-file and conformance tests (SRS 7.3)
//! and as a starting point in the application.
//!
//! Samples and stand-ins are built with a [`FixtureBuilder`], which gives
//! every object a name-based identity, so that building a sample twice gives
//! equal projects and the same IFCX file (IFX-010, IFX-011, NFR-MNT-003).

use std::collections::BTreeMap;

use glam::DVec2;
use uuid::Uuid;

use crate::command::{Command, History};
use crate::element::{Element, ElementKind, KindTag, Position};
use crate::ids::{ElementId, LayerId, MaterialId, StoreyId};
use crate::kinds::ReferenceLine;
use crate::project::Project;
use crate::storey::Storey;
use crate::template::default_metric_project_with;

/// Namespace of the name-based (version 5) identities of [`FixtureBuilder`].
pub const FIXTURE_NAMESPACE: Uuid = Uuid::from_u128(0x6d1f_4c1e_8a3b_5e27_9c4d_2b7a_0f15_e3c8);

/// Builds a reproducible project through commands, like a user would.
///
/// Every object gets a version 5 UUID under [`FIXTURE_NAMESPACE`] from a path
/// `<name>/<role>/<n>`: the template's objects by role (`project`, `storey`,
/// `layer`, `material`, ...), elements by kind code (`wall`, `slab`, ...),
/// with `n` counting from 1 per role. Element names follow the counters as
/// in the application (`W-001`, ...).
///
/// New elements are collected and added with one `AddElements` command
/// when the builder needs the model again ([`storey`](Self::storey) adding
/// a storey, [`edit`](Self::edit) of an added element, [`commit`](Self::commit)
/// or [`finish`](Self::finish)), which keeps large fixtures fast.
///
/// ```
/// use archicraft_model::glam::DVec2;
/// use archicraft_model::samples::FixtureBuilder;
///
/// let mut b = FixtureBuilder::new("example").titled("Example");
/// let ground = b.storey(0);
/// let wall = b.wall(ground, DVec2::ZERO, DVec2::new(5.0, 0.0));
/// b.edit(wall, |e| e.locked = true);
/// let project = b.finish();
/// assert_eq!(project.elements[&wall].name, "W-001");
/// assert_eq!(project, {
///     let mut b = FixtureBuilder::new("example").titled("Example");
///     let ground = b.storey(0);
///     let wall = b.wall(ground, DVec2::ZERO, DVec2::new(5.0, 0.0));
///     b.edit(wall, |e| e.locked = true);
///     b.finish()
/// });
/// ```
#[derive(Debug)]
pub struct FixtureBuilder {
    name: String,
    project: Project,
    history: History,
    /// The last `n` used per role.
    roles: BTreeMap<String, u32>,
    pending: Vec<Element>,
    pending_per_kind: BTreeMap<KindTag, u32>,
}

impl FixtureBuilder {
    /// A builder whose project comes from the default metric template, with
    /// identities derived from `name`.
    pub fn new(name: &str) -> Self {
        let mut roles = BTreeMap::new();
        let project = default_metric_project_with(&mut |role| next_id(name, &mut roles, role));
        Self {
            name: name.to_owned(),
            project,
            history: History::default(),
            roles,
            pending: Vec::new(),
            pending_per_kind: BTreeMap::new(),
        }
    }

    /// Sets the project name, as the template's project info.
    pub fn titled(mut self, title: &str) -> Self {
        self.project.info.name = title.to_owned();
        self
    }

    /// The next identity for `role`: `<name>/<role>/<n>`.
    pub fn id(&mut self, role: &str) -> Uuid {
        next_id(&self.name, &mut self.roles, role)
    }

    /// The project with every element added so far.
    pub fn project(&mut self) -> &Project {
        self.commit();
        &self.project
    }

    /// The layer named `name`, or the first layer.
    pub fn layer(&self, name: &str) -> LayerId {
        let layers = &self.project.layers;
        layers.iter().find(|l| l.name == name).map_or(layers[0].id, |l| l.id)
    }

    /// The material named `name`, or the first material.
    pub fn material(&self, name: &str) -> MaterialId {
        let materials = &self.project.materials;
        materials
            .iter()
            .find(|m| m.name == name)
            .map_or(materials[0].id, |m| m.id)
    }

    /// The storey at `index`, counted from the lowest storey. Missing
    /// storeys are inserted on top through `InsertStorey`, each with the top
    /// storey's height and the name `NN Floor NN`.
    pub fn storey(&mut self, index: usize) -> StoreyId {
        while self.project.storeys.len() <= index {
            self.commit();
            let id = StoreyId::from_uuid(self.id("storey"));
            let list = self.project.storeys.as_slice();
            let top = &list[list.len() - 1];
            let number = list.len();
            let storey = Storey {
                id,
                ..Storey::new(format!("{number:02} Floor {number:02}"), 0.0, top.height)
            };
            let command = Command::InsertStorey {
                reference: top.id,
                above: true,
                storey,
            };
            execute(&mut self.project, &mut self.history, command);
        }
        self.project.storeys.as_slice()[index].id
    }

    /// Creates an element of any registered kind on `storey`. Walls and
    /// slabs get the layer, position and IFC class of their tool defaults
    /// (ELM-001); other kinds get their default IFC class, the first layer
    /// and an undefined position, like [`crate::kinds`]' test kind.
    pub fn element(&mut self, storey: StoreyId, kind: impl Into<ElementKind>) -> ElementId {
        let kind = kind.into();
        let tag = kind.tag();
        let (layer, position, ifc_class) = match self.project.tool_defaults.get(tag) {
            Some(slot) => (slot.layer, slot.position, slot.ifc_class),
            None => (
                self.project.layers[0].id,
                Position::Undefined,
                tag.descriptor().default_class,
            ),
        };
        let nth = self.pending_per_kind.entry(tag).or_default();
        let name = self.project.next_element_name(tag, *nth);
        *nth += 1;
        let id = ElementId::from_uuid(self.id(tag.descriptor().code));
        self.pending.push(Element {
            id,
            name,
            storey,
            layer,
            ifc_class,
            position,
            locked: false,
            snap_excluded: false,
            kind,
        });
        id
    }

    /// A wall from `start` to `end` with the wall tool's defaults.
    pub fn wall(&mut self, storey: StoreyId, start: DVec2, end: DVec2) -> ElementId {
        let wall = self.project.new_wall(storey, start, end, 0).kind;
        self.element(storey, wall)
    }

    /// A slab over `outline` with `holes` and the slab tool's defaults.
    pub fn slab(&mut self, storey: StoreyId, outline: Vec<DVec2>, holes: Vec<Vec<DVec2>>) -> ElementId {
        let slab = self.project.new_slab(storey, outline, holes, 0).kind;
        self.element(storey, slab)
    }

    /// Changes an element. An element not added yet is changed before it is
    /// added; an added one through `UpdateElements`.
    ///
    /// # Panics
    ///
    /// If `id` is not an element this builder created.
    pub fn edit(&mut self, id: ElementId, change: impl FnOnce(&mut Element)) {
        if let Some(element) = self.pending.iter_mut().find(|e| e.id == id) {
            change(element);
            return;
        }
        self.commit();
        let Some(mut element) = self.project.element(id).cloned() else {
            panic!("edit: the builder did not create element {id:?}");
        };
        change(&mut element);
        execute(
            &mut self.project,
            &mut self.history,
            Command::UpdateElements(vec![element]),
        );
    }

    /// Adds the elements created since the last commit with one
    /// `AddElements` command.
    pub fn commit(&mut self) {
        if self.pending.is_empty() {
            return;
        }
        let elements = std::mem::take(&mut self.pending);
        self.pending_per_kind.clear();
        execute(&mut self.project, &mut self.history, Command::AddElements(elements));
    }

    /// The finished project.
    pub fn finish(mut self) -> Project {
        self.commit();
        self.project
    }
}

fn next_id(name: &str, roles: &mut BTreeMap<String, u32>, role: &str) -> Uuid {
    let n = roles.entry(role.to_owned()).or_default();
    *n += 1;
    Uuid::new_v5(&FIXTURE_NAMESPACE, format!("{name}/{role}/{n}").as_bytes())
}

fn execute(project: &mut Project, history: &mut History, command: Command) {
    if let Err(e) = history.execute(project, command) {
        unreachable!("fixture commands are valid: {e}");
    }
}

fn rectangle(min: DVec2, max: DVec2) -> Vec<DVec2> {
    vec![min, DVec2::new(max.x, min.y), max, DVec2::new(min.x, max.y)]
}

/// Adds a closed ring of joined exterior walls, counter-clockwise.
fn add_wall_ring(b: &mut FixtureBuilder, storey: StoreyId, corners: &[DVec2]) {
    for i in 0..corners.len() {
        b.wall(storey, corners[i], corners[(i + 1) % corners.len()]);
    }
}

/// Turns a wall into an interior wall: 11.5 cm sand-lime brick on the
/// interior walls layer, referenced at its centre.
fn make_interior(b: &mut FixtureBuilder, wall: ElementId) {
    let layer = b.layer("Interior walls");
    let material = b.material("Sand-lime brick");
    b.edit(wall, |e| {
        e.position = Position::Interior;
        e.layer = layer;
        if let Some(wall) = e.as_wall_mut() {
            wall.thickness = 0.115;
            wall.reference_line = ReferenceLine::Center;
            wall.material = material;
        }
    });
}

/// The exit criterion of SRS increment 0: one storey with four exterior
/// walls joined at the corners and a floor slab.
pub fn storey_with_walls_and_slab() -> Project {
    let mut b = FixtureBuilder::new("walls-and-slab").titled("Walls and slab");
    let ground = b.storey(0);
    let outline = rectangle(DVec2::ZERO, DVec2::new(10.0, 8.0));
    add_wall_ring(&mut b, ground, &outline);
    b.commit();
    b.slab(ground, outline, Vec::new());
    b.finish()
}

/// A two-storey shell: exterior walls on both storeys, an interior wall in a
/// T junction, a ground slab and an upper floor slab with a stair opening.
pub fn two_storey_shell() -> Project {
    let mut b = FixtureBuilder::new("two-storey-shell").titled("Two-storey shell");
    let ground = b.storey(0);
    let upper = b.storey(1);
    let outline = rectangle(DVec2::ZERO, DVec2::new(12.0, 9.0));

    for storey in [ground, upper] {
        add_wall_ring(&mut b, storey, &outline);
        b.commit();
        // Interior wall from the south wall's reference line northwards.
        let interior = b.wall(storey, DVec2::new(5.0, 0.0), DVec2::new(5.0, 6.0));
        make_interior(&mut b, interior);
        b.commit();
    }

    b.slab(ground, outline.clone(), Vec::new());
    let stair_hole = rectangle(DVec2::new(6.0, 1.0), DVec2::new(9.0, 2.2));
    b.slab(upper, outline, vec![stair_hole]);
    b.finish()
}

/// A sample of the catalogue: its file name (without `.ifcx`) and
/// constructor.
pub type CatalogueEntry = (&'static str, fn() -> Project);

/// The samples that are reference models: [`catalogue`] lists them.
const CATALOGUE: &[CatalogueEntry] = &[
    ("walls-and-slab", storey_with_walls_and_slab),
    ("two-storey-shell", two_storey_shell),
];

/// The reference-model samples by file name, the single list behind the
/// golden files and `examples/reference_models.rs` of the IFCX crate
/// (NFR-MNT-003). Adding a sample is one line in this list; its golden file
/// is then created with `ARCHICRAFT_BLESS=1`.
pub fn catalogue() -> &'static [CatalogueEntry] {
    CATALOGUE
}

/// The sizes of the synthetic stand-ins for the reference models RM-M and
/// RM-L (SRS 7.3), for benches and the NFR-PERF-006 budget.
pub const STAND_IN_SIZES: [usize; 3] = [1_000, 5_000, 20_000];

/// Storeys of one stand-in block, like RM-M.
const STAND_IN_STOREYS: usize = 5;
/// Elements of one stand-in block, about RM-M's size.
const STAND_IN_BLOCK: usize = 5_000;
/// Room size of the stand-in grid, in metres.
const ROOM: f64 = 4.0;
/// Gap between the exterior wall rings of neighbouring blocks, in metres.
const BLOCK_GAP: f64 = 2.0;

/// The plan of one stand-in storey: a ring of `columns * ROOM` by
/// `rows * ROOM`, interior walls along every column line from ring to ring,
/// and in each column cross walls every 4 m that end on the column walls or
/// the ring. Odd columns are shifted by 2 m, so every cross wall end meets a
/// single wall body: T junctions everywhere, L junctions at the corners.
struct StoreyPlan {
    columns: usize,
    rows: usize,
}

impl StoreyPlan {
    fn width(&self) -> f64 {
        self.columns as f64 * ROOM
    }

    fn depth(&self) -> f64 {
        self.rows as f64 * ROOM
    }

    fn cross_walls_in(&self, column: usize) -> usize {
        if column.is_multiple_of(2) {
            self.rows - 1
        } else {
            self.rows
        }
    }

    /// Elements per storey: ring, column walls, cross walls and the slab.
    fn elements(&self) -> usize {
        let cross: usize = (0..self.columns).map(|c| self.cross_walls_in(c)).sum();
        4 + (self.columns - 1) + cross + 1
    }

    /// The largest plan with at most `budget` elements, as square as the
    /// budget allows, or `None` below the smallest plan with a T junction.
    fn fitting(budget: usize) -> Option<Self> {
        let mut best: Option<(usize, usize, Self)> = None;
        for columns in 2..=budget {
            let smallest = Self { columns, rows: 1 };
            if smallest.elements() > budget {
                break;
            }
            for rows in 1..=budget {
                let plan = Self { columns, rows };
                let elements = plan.elements();
                if elements > budget {
                    break;
                }
                let skew = columns.abs_diff(rows);
                if best
                    .as_ref()
                    .is_none_or(|(e, s, _)| elements > *e || (elements == *e && skew < *s))
                {
                    best = Some((elements, skew, plan));
                }
            }
        }
        best.map(|(_, _, plan)| plan)
    }

    /// Adds the storey's walls and slab with the plan's south-west corner
    /// at `origin`.
    fn build(&self, b: &mut FixtureBuilder, storey: StoreyId, origin: DVec2) {
        let (w, d) = (self.width(), self.depth());
        let outline = rectangle(origin, origin + DVec2::new(w, d));
        add_wall_ring(b, storey, &outline);
        for c in 1..self.columns {
            let x = origin.x + c as f64 * ROOM;
            let wall = b.wall(storey, DVec2::new(x, origin.y), DVec2::new(x, origin.y + d));
            make_interior(b, wall);
        }
        for c in 0..self.columns {
            let x = origin.x + c as f64 * ROOM;
            let shift = if c.is_multiple_of(2) { ROOM } else { ROOM / 2.0 };
            for k in 0..self.cross_walls_in(c) {
                let y = origin.y + shift + k as f64 * ROOM;
                let wall = b.wall(storey, DVec2::new(x, y), DVec2::new(x + ROOM, y));
                make_interior(b, wall);
            }
        }
        b.slab(storey, outline, Vec::new());
    }

    /// Start points of 1 m stubs that touch no wall: in every column at odd
    /// metres, where no cross wall runs.
    fn stub_starts(&self, origin: DVec2) -> impl Iterator<Item = DVec2> + '_ {
        (0..self.columns).flat_map(move |c| {
            (0..2 * self.rows).map(move |k| origin + DVec2::new(c as f64 * ROOM + 1.5, 2.0 * k as f64 + 1.0))
        })
    }
}

/// A synthetic stand-in for the reference models (SRS 7.3) with exactly `n`
/// elements, for benches and the NFR-PERF-006 and VIEW-001 budgets until
/// RM-M and RM-L exist.
///
/// Like RM-M, a block has five 3.0 m storeys of about `n / 5` elements each
/// (about 1,000 at `n = 5,000`), every storey a grid of 4 m rooms (see
/// `StoreyPlan`) with an exterior wall ring, interior walls in T junctions
/// and one slab. Beyond 5,000 elements, further five-storey blocks stand side
/// by side along +X with a 2 m gap, like RM-L's repeated RM-M blocks. The
/// rest up to `n` are free-standing 1 m interior wall stubs on the top storey
/// of the last block. Below 35 elements the storeys are fewer, and below 7
/// there are only stubs.
pub fn stand_in(n: usize) -> Project {
    let name = format!("stand-in-{n}");
    let mut b = FixtureBuilder::new(&name).titled(&format!("Stand-in, {n} elements"));
    let blocks = n.div_ceil(STAND_IN_BLOCK).max(1);
    let storeys = STAND_IN_STOREYS.min(n / 7).max(1);
    let plan = StoreyPlan::fitting(n / (blocks * storeys));
    let mut origin = DVec2::ZERO;
    let mut top = b.storey(0);
    let mut added = 0;
    if let Some(plan) = &plan {
        for block in 0..blocks {
            origin = DVec2::new(block as f64 * (plan.width() + BLOCK_GAP), 0.0);
            for index in 0..storeys {
                top = b.storey(index);
                plan.build(&mut b, top, origin);
                b.commit();
                added += plan.elements();
            }
        }
    }
    let inside = plan.iter().flat_map(|p| p.stub_starts(origin));
    // Past the free spots inside, stubs continue in a row north of the ring.
    let north = origin.y + plan.as_ref().map_or(0.0, |p| p.depth()) + BLOCK_GAP;
    let outside = (0..).map(|k| DVec2::new(origin.x + 2.0 * k as f64, north));
    for start in inside.chain(outside).take(n - added) {
        let stub = b.wall(top, start, start + DVec2::X);
        make_interior(&mut b, stub);
    }
    b.finish()
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
/// Provisional bench model (NFR-PERF-001): the seeded [`stand_in`] models
/// of [`STAND_IN_SIZES`] and RM-M/RM-L (SRS 7.3) will replace it in
/// [`bench_models`] (regression-and-status).
pub fn tiled_shell(n: usize) -> Project {
    let mut project = two_storey_shell();
    let mut history = History::default();
    project.info.name = format!("Tiled shell {n} x {n}");
    let tile: Vec<_> = project.elements.values().cloned().collect();
    for (i, j) in (0..n).flat_map(|i| (0..n).map(move |j| (i, j))).skip(1) {
        let offset = DVec2::new(i as f64, j as f64) * TILE_PITCH;
        let mut created: BTreeMap<KindTag, u32> = BTreeMap::new();
        let copies = tile
            .iter()
            .map(|element| {
                let mut copy = element.clone();
                copy.id = ElementId::new();
                copy.kind.translate(offset);
                let nth = created.entry(copy.tag()).or_default();
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
    /// Builds the model's project afresh for each bench run.
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

/// The environment variable that puts a bench run into the CI regression
/// gate's smoke mode (`scripts/bench-ci.sh`, docs/performance/README.md).
pub const BENCH_CI_ENV: &str = "ARCHICRAFT_BENCH_CI";

/// The bench models a bench run measures (NFR-PERF-001): only those flagged
/// [`BenchModel::ci`] when [`BENCH_CI_ENV`] is set, every model otherwise.
pub fn bench_models_to_run() -> Vec<BenchModel> {
    bench_models_for(std::env::var_os(BENCH_CI_ENV).is_some())
}

/// The bench models of a CI run (`ci_only`) or of a full run.
pub fn bench_models_for(ci_only: bool) -> Vec<BenchModel> {
    select_bench_models(bench_models(), ci_only)
}

fn select_bench_models(models: Vec<BenchModel>, ci_only: bool) -> Vec<BenchModel> {
    models.into_iter().filter(|m| m.ci || !ci_only).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// NFR-PERF-001: the CI regression gate's smoke mode runs only the
    /// models flagged `ci`; a full run measures every model.
    #[test]
    fn a_ci_run_measures_only_the_models_flagged_ci() {
        let model = |id, ci| BenchModel {
            id,
            ci,
            build: two_storey_shell,
        };
        let models = vec![model("small", true), model("rm_l", false), model("tiled", true)];
        let ids = |ms: Vec<BenchModel>| ms.iter().map(|m| m.id).collect::<Vec<_>>();
        assert_eq!(ids(select_bench_models(models.clone(), true)), ["small", "tiled"]);
        assert_eq!(ids(select_bench_models(models, false)), ["small", "rm_l", "tiled"]);
        assert_eq!(ids(bench_models_for(true)), ["two_storey_shell", "tiled_shell"]);
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
        assert!(
            (max_x - (TILE_PITCH + 12.0)).abs() < 1e-9,
            "the second tile ends one pitch past the first, got {max_x}"
        );
        assert_eq!(tiled_shell(1).elements.len(), 12);
    }
}
