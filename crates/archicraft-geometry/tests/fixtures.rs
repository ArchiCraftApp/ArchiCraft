//! Generic geometry checks on every fixture of the registry (SRS Appendix
//! D.7): evaluation without problems and closed 2-manifold bodies (IFX-032).

#![allow(clippy::unwrap_used)]

use archicraft_geometry::ModelGeometry;
use archicraft_model::fixtures;

/// IFX-032: every built fixture evaluates without problems, and every part of
/// every element is a closed body.
#[test]
fn every_built_fixture_evaluates_to_closed_bodies() {
    let mut checked = 0;
    for f in fixtures::all().filter(|f| f.build.is_some()) {
        let project = fixtures::check(f).unwrap_or_else(|e| panic!("{e}"));
        let geometry = ModelGeometry::evaluate(&project);
        assert!(geometry.problems.is_empty(), "{}: {:?}", f.id, geometry.problems);
        for element in project.elements.values() {
            let body = geometry
                .get(element.id)
                .unwrap_or_else(|| panic!("{}: {} has no body", f.id, element.name));
            assert!(!body.parts.is_empty(), "{}: {} has no parts", f.id, element.name);
            for part in &body.parts {
                assert_eq!(part.mesh.check_closed(), Ok(()), "{}: {}", f.id, element.name);
            }
        }
        checked += 1;
    }
    assert!(checked >= 1);
}
