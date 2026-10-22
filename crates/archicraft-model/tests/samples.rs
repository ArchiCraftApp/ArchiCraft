//! The seeded fixture builder, the sample catalogue and the synthetic
//! stand-ins for RM-M and RM-L (SRS 7.3, NFR-PERF-006, VIEW-001, IFX-010,
//! IFX-011, NFR-MNT-003).

#![allow(clippy::unwrap_used)]

use std::collections::HashSet;
use std::time::Instant;

use archicraft_model::glam::DVec2;
use archicraft_model::kinds::Dummy;
use archicraft_model::samples::{self, CatalogueEntry, FIXTURE_NAMESPACE, FixtureBuilder, STAND_IN_SIZES};
use archicraft_model::template::{default_metric_project, default_metric_project_with};
use archicraft_model::{ElementId, IfcClass, KindTag, Position, Project};
use uuid::Uuid;

/// Every identity of a project: spatial structure, resources, elements and
/// views.
fn identities(p: &Project) -> Vec<Uuid> {
    [p.id.uuid(), p.site.id.uuid(), p.building.id.uuid()]
        .into_iter()
        .chain(p.storeys.as_slice().iter().map(|s| s.id.uuid()))
        .chain(p.layers.iter().map(|l| l.id.uuid()))
        .chain(p.materials.iter().map(|m| m.id.uuid()))
        .chain(p.elements.keys().map(|id| id.uuid()))
        .chain(p.views.iter().map(|v| v.id.uuid()))
        .collect()
}

/// IFX-010, IFX-011, NFR-MNT-003: building every catalogue sample and the
/// smallest stand-in twice gives equal projects, every identity included,
/// and the identities are name-based (version 5) and unique.
#[test]
fn fixtures_are_deterministic() {
    let builds: Vec<CatalogueEntry> = samples::catalogue()
        .iter()
        .copied()
        .chain([("stand-in-1000", (|| samples::stand_in(1_000)) as fn() -> Project)])
        .collect();
    for (name, build) in builds {
        let (a, b) = (build(), build());
        assert_eq!(a, b, "{name}");
        assert_eq!(identities(&a), identities(&b), "{name}");
        let ids = identities(&a);
        assert!(ids.iter().all(|id| id.get_version_num() == 5), "{name}: version 5 ids");
        assert_eq!(
            ids.iter().collect::<HashSet<_>>().len(),
            ids.len(),
            "{name}: unique ids"
        );
    }
}

/// The catalogue is the list of reference-model samples by file name; two
/// fixtures with different names never share an identity.
#[test]
fn the_catalogue_lists_the_reference_samples() {
    let names: Vec<&str> = samples::catalogue().iter().map(|(name, _)| *name).collect();
    assert_eq!(names, ["walls-and-slab", "two-storey-shell"]);
    let mut seen = HashSet::new();
    for (name, build) in samples::catalogue() {
        for id in identities(&build()) {
            assert!(seen.insert(id), "{name} reuses {id}");
        }
    }
}

/// IFX-011: the template takes its identities from the given source, one
/// call per object with its role, and the tool defaults refer to the
/// resources by those identities.
#[test]
fn the_template_takes_identities_from_the_source() {
    let mut roles = Vec::new();
    let mut n = 0u128;
    let p = default_metric_project_with(&mut |role| {
        roles.push(role.to_owned());
        n += 1;
        Uuid::from_u128(n)
    });
    let count = |role: &str| roles.iter().filter(|r| *r == role).count();
    assert_eq!(roles[..3], ["project", "site", "building"]);
    assert_eq!(count("material"), 25);
    assert_eq!(count("layer"), p.layers.len());
    assert_eq!(count("storey"), 2);
    assert_eq!(count("view"), 2);
    let ids: HashSet<Uuid> = (1..=n).map(Uuid::from_u128).collect();
    assert!(identities(&p).iter().all(|id| ids.contains(id)));
    for (_, slot) in p.tool_defaults.iter() {
        assert!(p.material(slot.kind.material()).is_some());
        assert!(p.layer(slot.layer).is_some());
    }
    // Without a source, every project still gets new random identities.
    let (a, b) = (default_metric_project(), default_metric_project());
    assert_ne!(a.id, b.id);
    assert_eq!(a.id.uuid().get_version_num(), 4);
}

/// The builder derives element ids from `<name>/<kind code>/<n>` and names
/// elements like the tools do.
#[test]
fn the_builder_derives_ids_from_name_kind_and_number() {
    let mut b = FixtureBuilder::new("ids");
    let ground = b.storey(0);
    let first = b.wall(ground, DVec2::ZERO, DVec2::X * 3.0);
    let second = b.wall(ground, DVec2::X * 3.0, DVec2::new(3.0, 3.0));
    let slab = b.slab(ground, vec![DVec2::ZERO, DVec2::X, DVec2::ONE], Vec::new());
    let p = b.finish();
    let v5 = |path: &str| Uuid::new_v5(&FIXTURE_NAMESPACE, path.as_bytes());
    assert_eq!(first.uuid(), v5("ids/wall/1"));
    assert_eq!(second.uuid(), v5("ids/wall/2"));
    assert_eq!(slab.uuid(), v5("ids/slab/1"));
    assert_eq!(p.id.uuid(), v5("ids/project/1"));
    assert_eq!(p.storeys.ground().uuid(), v5("ids/storey/1"));
    let names: Vec<&str> = p.elements.values().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["W-001", "W-002", "SL-001"]);
    assert_eq!(p.counters.get(KindTag::Wall), 2);
}

/// `element` takes any registered kind, the test kind included, with its
/// default IFC class; walls get the wall tool's defaults.
#[test]
fn the_builder_creates_elements_of_any_kind() {
    let mut b = FixtureBuilder::new("kinds");
    let ground = b.storey(0);
    let material = b.material("Concrete");
    let dummy = b.element(ground, Dummy::new(DVec2::ZERO, DVec2::ONE, material));
    let wall = b.wall(ground, DVec2::ZERO, DVec2::X);
    let p = b.finish();
    let dummy = &p.elements[&dummy];
    assert_eq!(dummy.tag(), KindTag::Dummy);
    assert_eq!(dummy.ifc_class, KindTag::Dummy.descriptor().default_class);
    assert_eq!(dummy.layer, p.layers[0].id);
    assert_eq!(dummy.position, Position::Undefined);
    assert_eq!(dummy.name, "X-001");
    let wall = &p.elements[&wall];
    assert_eq!(wall.ifc_class, IfcClass::Wall);
    assert_eq!(wall.layer, p.tool_defaults.get(KindTag::Wall).unwrap().layer);
    assert_eq!(wall.position, Position::Exterior);
    assert!(p.integrity_problems().is_empty());
}

/// `storey` inserts missing storeys on top with the top storey's height,
/// and `edit` changes elements before and after they are added.
#[test]
fn the_builder_adds_storeys_and_edits_elements() {
    let mut b = FixtureBuilder::new("edits");
    let third = b.storey(3);
    let wall = b.wall(third, DVec2::ZERO, DVec2::X * 2.0);
    b.edit(wall, |e| e.name = "Pending".into());
    b.commit();
    b.edit(wall, |e| e.locked = true);
    let p = b.finish();
    let elevations: Vec<f64> = p.storeys.as_slice().iter().map(|s| s.elevation).collect();
    assert_eq!(elevations, [0.0, 3.0, 6.0, 9.0]);
    assert_eq!(p.storeys.as_slice()[3].id, third);
    assert_eq!(p.storeys.as_slice()[3].name, "03 Floor 03");
    let wall = &p.elements[&wall];
    assert_eq!(wall.name, "Pending");
    assert!(wall.locked);
}

/// `edit` panics, as documented, for an id the builder did not create.
#[test]
#[should_panic(expected = "the builder did not create")]
fn editing_an_unknown_element_panics() {
    let mut b = FixtureBuilder::new("unknown");
    b.edit(ElementId::new(), |e| e.locked = true);
}

/// The two samples keep their geometry and contents, now with seeded
/// identities.
#[test]
fn samples_are_valid() {
    let p = samples::storey_with_walls_and_slab();
    assert_eq!(p.info.name, "Walls and slab");
    assert_eq!(p.elements.len(), 5);
    assert!(p.integrity_problems().is_empty());
    let p = samples::two_storey_shell();
    assert_eq!(p.info.name, "Two-storey shell");
    assert_eq!(p.elements.len(), 12);
    assert!(p.integrity_problems().is_empty());
    let names: Vec<&str> = p.elements.values().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "W-001", "W-002", "W-003", "W-004", "W-005", "W-006", "W-007", "W-008", "W-009", "W-010", "SL-001",
            "SL-002"
        ]
    );
    let interior: Vec<_> = p
        .elements
        .values()
        .filter(|e| e.position == Position::Interior)
        .collect();
    assert_eq!(interior.len(), 2);
    for e in interior {
        let wall = e.as_wall().unwrap();
        assert_eq!((wall.start, wall.end), (DVec2::new(5.0, 0.0), DVec2::new(5.0, 6.0)));
        assert_eq!(wall.thickness, 0.115);
        assert_eq!(p.layer(e.layer).unwrap().name, "Interior walls");
        assert_eq!(p.material(wall.material).unwrap().name, "Sand-lime brick");
    }
    let upper = p
        .elements
        .values()
        .find(|e| e.name == "SL-002")
        .unwrap()
        .as_slab()
        .unwrap();
    assert_eq!(upper.holes.len(), 1);
}

/// NFR-PERF-006, VIEW-001: a stand-in has exactly the requested number of
/// elements and is valid, for the benchmark sizes and for odd sizes.
#[test]
fn stand_ins_have_exactly_n_valid_elements() {
    for n in [0, 1, 6, 7, 34, 35, 1_000, 1_234] {
        let p = samples::stand_in(n);
        assert_eq!(p.elements.len(), n, "stand_in({n})");
        assert!(p.integrity_problems().is_empty(), "stand_in({n})");
    }
    assert_eq!(STAND_IN_SIZES, [1_000, 5_000, 20_000]);
}

/// RM-M shape: five 3.0 m storeys of about 200 elements at n = 1,000, each
/// with one slab; no test kinds.
#[test]
fn a_stand_in_has_five_storeys_of_rooms() {
    let p = samples::stand_in(1_000);
    let elevations: Vec<f64> = p.storeys.as_slice().iter().map(|s| s.elevation).collect();
    assert_eq!(elevations, [0.0, 3.0, 6.0, 9.0, 12.0]);
    for storey in p.storeys.as_slice() {
        let on: Vec<_> = p.elements_on(storey.id).collect();
        assert!((190..=260).contains(&on.len()), "{}: {}", storey.name, on.len());
        assert_eq!(on.iter().filter(|e| e.tag() == KindTag::Slab).count(), 1);
    }
    assert!(
        p.elements
            .values()
            .all(|e| matches!(e.tag(), KindTag::Wall | KindTag::Slab))
    );
}

/// RM-L shape: beyond 5,000 elements, further five-storey blocks stand side
/// by side along +X.
#[test]
fn a_stand_in_of_5000_has_exactly_5000_elements() {
    let started = Instant::now();
    let p = samples::stand_in(5_000);
    eprintln!("stand_in(5_000) built in {:?}", started.elapsed());
    assert_eq!(p.elements.len(), 5_000);
    assert_eq!(p.storeys.len(), 5);
    assert!(p.integrity_problems().is_empty());
}

/// The largest stand-in has exactly 20,000 elements in four blocks. It
/// builds in well under a second in the dev profile (about 20 ms on an M3
/// Max), so it runs in the normal suite.
#[test]
fn a_stand_in_of_20000_has_exactly_20000_elements() {
    let started = Instant::now();
    let p = samples::stand_in(20_000);
    eprintln!("stand_in(20_000) built in {:?}", started.elapsed());
    assert_eq!(p.elements.len(), 20_000);
    assert!(p.integrity_problems().is_empty());
    let max_x = p
        .elements
        .values()
        .filter_map(|e| e.as_wall())
        .map(|w| w.start.x.max(w.end.x))
        .fold(f64::MIN, f64::max);
    let block = samples::stand_in(5_000);
    let block_x = block
        .elements
        .values()
        .filter_map(|e| e.as_wall())
        .map(|w| w.start.x.max(w.end.x))
        .fold(f64::MIN, f64::max);
    assert!(max_x > 3.0 * block_x, "four blocks along +X: {max_x} vs {block_x}");
}
