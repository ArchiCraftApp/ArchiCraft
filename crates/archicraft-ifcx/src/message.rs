//! Message keys of the problems archicraft-ifcx reports (NFR-L10N-001,
//! PRJ-008).
//!
//! The crate has no message catalogue and no user-interface dependencies
//! (NFR-MNT-002). Every [`Problem`](crate::Problem) carries a [`Message`] of
//! archicraft-model: an `ifcx-` key and typed arguments, which the
//! application looks up in its catalogue (`locales/en/ifcx.ftl` in
//! archicraft-app) and renders in the user's language. Errors of the model
//! (element, material and storey validation) are nested in them as they
//! are, and texts of libraries (the JSON parser, the operating system) are
//! passed as `detail` text arguments.
//!
//! Every key the crate emits is listed in [`ALL_KEYS`], so the application
//! can check that its catalogue covers them.

pub use archicraft_model::{Arg, Message};

/// Every message key archicraft-ifcx emits.
pub const ALL_KEYS: &[&str] = &[
    // A problem with its node path (Problem::located).
    "ifcx-problem-at",
    // Reading and validating files (validate.rs, lib.rs).
    "ifcx-schema-unreadable",
    "ifcx-import-offline",
    "ifcx-schema-too-deep",
    "ifcx-schema-unknown-inherited",
    "ifcx-schema-array-restrictions",
    "ifcx-schema-blob",
    "ifcx-value-type",
    "ifcx-value-not-option",
    "ifcx-value-key-missing",
    "ifcx-value-in-key",
    "ifcx-attribute-undeclared",
    "ifcx-attribute-invalid",
    "ifcx-detail",
    "ifcx-composition-too-deep",
    "ifcx-composition-cycle",
    "ifcx-unknown-node",
    "ifcx-roots",
    "ifcx-file-too-large",
    "ifcx-size-gb",
    "ifcx-size-mb",
    "ifcx-size-kb",
    "ifcx-size-bytes",
    "ifcx-file-unreadable",
    "ifcx-file-nested-too-deep",
    "ifcx-file-invalid",
    "ifcx-file-too-many-imports",
    "ifcx-version-unsupported",
    // Migration (alpha/migrate.rs).
    "ifcx-migration-unknown",
    "ifcx-migration-failed",
    "ifcx-migrated",
    // The load report (alpha/reader.rs): what happened to a record ...
    "ifcx-skipped",
    "ifcx-repaired",
    // ... which record ...
    "ifcx-record-project",
    "ifcx-record-project-settings",
    "ifcx-record-project-location",
    "ifcx-record-project-information",
    "ifcx-record-material",
    "ifcx-record-layer",
    "ifcx-record-view",
    "ifcx-record-tool-defaults",
    "ifcx-record-counters",
    "ifcx-record-site",
    "ifcx-record-building",
    "ifcx-record-storey",
    "ifcx-record-element",
    // ... why, and what was used instead.
    "ifcx-default-used",
    "ifcx-left-out",
    "ifcx-value-used",
    "ifcx-field-missing",
    "ifcx-field-not-number",
    "ifcx-field-not-integer",
    "ifcx-field-not-text",
    "ifcx-field-not-boolean",
    "ifcx-field-not-object",
    "ifcx-field-not-list",
    "ifcx-field-unknown-value",
    "ifcx-field-not-uuid",
    "ifcx-field-out-of-range",
    "ifcx-field-point",
    "ifcx-field-colour-component",
    "ifcx-field-not-colour",
    "ifcx-point-not-list",
    "ifcx-point-not-numeric",
    "ifcx-point-coordinates",
    "ifcx-points-not-list",
    "ifcx-not-object",
    "ifcx-path-not-uuid",
    "ifcx-no-name",
    "ifcx-material-corrected",
    "ifcx-grid-spacings",
    "ifcx-out-of-range-replaced",
    "ifcx-newer-format",
    "ifcx-format-version-invalid",
    "ifcx-ground-storey-repaired",
    "ifcx-template-defaults-used",
    "ifcx-counters-from-elements",
    "ifcx-counter-negative",
    "ifcx-tool-unknown",
    "ifcx-tool-template-used",
    "ifcx-tool-material-missing",
    "ifcx-tool-layer-missing",
    "ifcx-tool-class-not-allowed",
    "ifcx-storey-listed-twice",
    "ifcx-storey-no-data",
    "ifcx-storey-height-min",
    "ifcx-no-storeys",
    "ifcx-ground-storey-undefined",
    "ifcx-storey-moved-up",
    "ifcx-storeys-unreadable",
    "ifcx-storey-lost",
    "ifcx-element-data-not-object",
    "ifcx-element-layer-missing",
    "ifcx-element-class-unsupported",
    "ifcx-element-material-missing",
    "ifcx-parameters-missing",
    "ifcx-holes-left-out",
    "ifcx-kind-unreadable",
    "ifcx-no-project",
    "ifcx-project-not-object",
    "ifcx-no-site",
    "ifcx-no-building",
    "ifcx-no-views",
    "ifcx-no-materials",
    "ifcx-no-layers",
    "ifcx-data-version-invalid",
    "ifcx-foreign-node",
    "ifcx-duplicate",
    // Saving (alpha/writer.rs).
    "ifcx-tool-defaults-not-saved",
    "ifcx-saved-without-body",
    "ifcx-kind-not-storable",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_prefixed() {
        let unique: std::collections::BTreeSet<_> = ALL_KEYS.iter().collect();
        assert_eq!(unique.len(), ALL_KEYS.len());
        assert!(ALL_KEYS.iter().all(|k| k.starts_with("ifcx-")));
    }

    /// Every key the sources pass to `Message::new` is in [`ALL_KEYS`], and
    /// every key in it is used, so the catalogue check in the application
    /// covers exactly what the crate emits.
    #[test]
    fn all_keys_lists_the_keys_the_sources_use() {
        let sources = [
            include_str!("validate.rs"),
            include_str!("lib.rs"),
            include_str!("alpha/migrate.rs"),
            include_str!("alpha/reader.rs"),
            include_str!("alpha/writer.rs"),
        ];
        let mut used = std::collections::BTreeSet::new();
        for source in sources {
            // The code before its tests.
            let source = source.split("#[cfg(test)]").next().unwrap_or(source);
            for (i, _) in source.match_indices("\"ifcx-") {
                let rest = &source[i + 1..];
                let end = rest.find('"').unwrap_or(rest.len());
                used.insert(&rest[..end]);
            }
        }
        let listed: std::collections::BTreeSet<&str> = ALL_KEYS.iter().copied().collect();
        assert_eq!(used, listed);
    }
}
