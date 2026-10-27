//! Message keys of the geometry problems archicraft-geometry reports
//! (NFR-L10N-001, UI-012).
//!
//! The crate has no message catalogue and no user-interface dependencies
//! (NFR-MNT-002). A [`GeometryError`](crate::GeometryError) and every entry
//! of [`ModelGeometry::problems`](crate::ModelGeometry::problems) is a
//! [`Message`] of archicraft-model: a `geometry-` key and typed arguments,
//! which the application looks up in its catalogue
//! (`locales/en/geometry.ftl` in archicraft-app) and renders in the user's
//! language. An outline the model rejects keeps the model's error nested in
//! it as it is.
//!
//! Every key the crate emits is listed in [`ALL_KEYS`], so the application
//! can check that its catalogue covers them.

pub use archicraft_model::Message;

/// Every message key archicraft-geometry emits.
pub const ALL_KEYS: &[&str] = &[
    // GeometryError::message (lib.rs).
    "geometry-invalid-polygon",
    "geometry-no-height",
    "geometry-home-storey-missing",
    "geometry-linked-storey-missing",
    "geometry-kind-unsupported",
    // ModelGeometry::evaluate (evaluate.rs).
    "geometry-evaluation-panicked",
    // Why triangulation refused a polygon (triangulate.rs).
    "geometry-polygon-too-few-points",
    "geometry-polygon-not-finite",
    "geometry-polygon-coordinate-range",
    "geometry-polygon-duplicate-point",
    "geometry-polygon-self-intersection",
    "geometry-polygon-no-area",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_prefixed() {
        let unique: std::collections::BTreeSet<_> = ALL_KEYS.iter().collect();
        assert_eq!(unique.len(), ALL_KEYS.len());
        assert!(ALL_KEYS.iter().all(|k| k.starts_with("geometry-")));
    }

    /// Every key the sources pass to `Message::new` is in [`ALL_KEYS`], and
    /// every key in it is used, so the catalogue check in the application
    /// covers exactly what the crate emits.
    #[test]
    fn all_keys_lists_the_keys_the_sources_use() {
        let sources = [
            include_str!("lib.rs"),
            include_str!("evaluate.rs"),
            include_str!("extrude.rs"),
            include_str!("mesh.rs"),
            include_str!("plan.rs"),
            include_str!("slice.rs"),
            include_str!("triangulate.rs"),
            include_str!("wall.rs"),
        ];
        let mut used = std::collections::BTreeSet::new();
        for source in sources {
            // The code before its tests.
            let source = source.split("#[cfg(test)]").next().unwrap_or(source);
            for (i, _) in source.match_indices("\"geometry-") {
                let rest = &source[i + 1..];
                let end = rest.find('"').unwrap_or(rest.len());
                used.insert(&rest[..end]);
            }
        }
        let listed: std::collections::BTreeSet<&str> = ALL_KEYS.iter().copied().collect();
        assert_eq!(used, listed);
    }
}
