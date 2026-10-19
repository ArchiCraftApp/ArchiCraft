//! The Appendix D.1 plan view range of the default metric template (STR-008,
//! VIEW-010). The other Appendix D values are pinned by
//! crates/archicraft-model/tests/defaults.rs, whose table names this file.

use archicraft_geometry::ViewRange;
use archicraft_model::template::default_metric_project;

const REFERENCE: &str = "D.1 Plan cut plane; view range: bottom at the storey elevation, top at the next storey";

/// Appendix D.1 (STR-008): on every storey of the default template the plan
/// is cut 1.10 m above the storey elevation, and the view range runs from the
/// storey elevation to the next storey (the nominal height on the top one).
#[test]
fn default_view_range_runs_from_the_storey_elevation_to_the_next_storey() {
    let p = default_metric_project();
    let storeys = p.storeys.as_slice();
    for (index, storey) in storeys.iter().enumerate() {
        let range = ViewRange::for_storey(&p, storey.id);
        let next = storeys
            .get(index + 1)
            .map_or(storey.height, |n| n.elevation - storey.elevation);
        let expected = ViewRange {
            cut: 1.10,
            bottom: 0.0,
            top: next,
        };
        assert_eq!(range, expected, "{REFERENCE} ({})", storey.name);
    }
    assert_eq!(
        ViewRange::for_storey(&p, p.storeys.ground()).top,
        3.0,
        "{REFERENCE}: the ground storey ends at 01 First floor (+3.000 m)"
    );
}
