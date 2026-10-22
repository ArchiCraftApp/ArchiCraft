//! Migration of files written by earlier ArchiCraft format versions
//! (IFX-062, IFX-091).
//!
//! The `formatVersion` in `archicraft::project` versions the ArchiCraft
//! schema ([`FORMAT_VERSION`](schema::FORMAT_VERSION)). The reader upgrades a
//! file of an earlier format step by step before it validates and reads it:
//! each [`Migration`] turns format `from` into `from + 1` on the parsed
//! container, so a step needs none of the old Rust types and old shapes never
//! reach the domain model or geometry (NFR-MNT-004). After the last step the
//! file declares the current `archicraft::` schemas, so validation checks the
//! migrated data against the shapes the reader expects.
//!
//! No migration is registered yet: format 1 is the only format ever written.
//! Whenever a reader-visible shape changes, bump
//! [`FORMAT_VERSION`](schema::FORMAT_VERSION) and add a step to
//! [`MIGRATIONS`]; optional additions need neither.
//!
//! A step works on [`Value`] trees, so a file opened with raw attribute
//! values ([`crate::validate::parse_raw`]) is converted to trees before its
//! first step: a migrated file is held as a tree of the whole file, with the
//! memory that takes (NFR-SEC-003), until it has been read.

use serde_json::Value;

use super::schema;
use crate::container::{AttributeValue, IfcxFile};
use crate::validate::Problem;

/// One step of the migration chain: upgrades a file from format `from` to
/// format `from + 1`.
#[derive(Debug)]
pub struct Migration {
    pub from: u32,
    /// What the step changes, for the load report.
    pub description: &'static str,
    /// Rewrites the file in place; reports what it could not convert.
    pub apply: fn(&mut IfcxFile, &mut Vec<Problem>),
}

/// The migration chain up to [`FORMAT_VERSION`](schema::FORMAT_VERSION), one
/// step per earlier format, in order.
pub const MIGRATIONS: &[Migration] = &[];

/// The format version of the file and the path of its project node: the last
/// entry with `archicraft::project` wins, as when the file is composed. Of
/// raw text, only `formatVersion` is parsed, not the materials, layers and
/// views of the project.
fn format_version<A: AttributeValue>(file: &IfcxFile<A>) -> Option<(String, u32)> {
    let (node, version) = file.data.iter().rev().find_map(|n| {
        let version = n.attributes.get(schema::PROJECT)?.field("formatVersion").ok()??;
        Some((n, version))
    })?;
    let version = version?.as_str()?.parse().ok()?;
    Some((node.path.clone(), version))
}

/// Upgrades `file` from its format version to `target` with `migrations`,
/// reporting each step as an info problem “migrated from format n to format
/// n+1: …”. Returns the migrated file, held as [`Value`] trees, and the
/// format the file was written in
/// ([`ReadOutcome::migrated_from`](super::reader::ReadOutcome::migrated_from)),
/// or `None` when no step ran: a file without a readable format version, or
/// at `target` or newer, is read as it is, without a copy; the reader reports
/// those.
pub(crate) fn upgrade<A: AttributeValue>(
    file: &IfcxFile<A>,
    migrations: &[Migration],
    target: u32,
    problems: &mut Vec<Problem>,
) -> Option<(IfcxFile, u32)> {
    let (path, from) = format_version(file)?;
    let mut version = from;
    let mut migrated: Option<IfcxFile> = None;
    while version < target {
        let Some(step) = migrations.iter().find(|m| m.from == version) else {
            problems.push(Problem::warning(
                Some(&path),
                format!("no migration from ArchiCraft format {version} is known; the file is read as it is"),
            ));
            break;
        };
        let tree = match &mut migrated {
            Some(tree) => tree,
            None => match file.to_values() {
                Ok(tree) => migrated.insert(tree),
                // Unreachable for a file from `parse_raw`, which has
                // checked that every attribute value parses.
                Err(e) => {
                    problems.push(Problem::error(
                        Some(&path),
                        format!(
                            "the file cannot be migrated from ArchiCraft format {version}: {e}; it is read as it is"
                        ),
                    ));
                    return None;
                }
            },
        };
        (step.apply)(tree, problems);
        problems.push(Problem::info(
            Some(&path),
            format!(
                "migrated from format {version} to format {}: {}",
                version + 1,
                step.description
            ),
        ));
        version += 1;
    }
    let mut file = migrated?;
    for node in &mut file.data {
        if let Some(project) = node.attributes.get_mut(schema::PROJECT).and_then(Value::as_object_mut) {
            project.insert("formatVersion".into(), version.to_string().into());
        }
    }
    file.schemas.retain(|id, _| !id.starts_with("archicraft::"));
    file.schemas.extend(schema::declarations());
    Some((file, from))
}

#[cfg(test)]
mod tests {
    use serde_json::value::RawValue;

    use super::*;
    use crate::alpha::reader;
    use crate::alpha::schema::FORMAT_VERSION;
    use crate::validate::{self, Severity};

    /// A file written at format 1 (tests/formats/, never regenerated).
    const FORMAT_1: &[u8] = include_bytes!("../../tests/formats/format-1.ifcx");

    const DESCRIPTION: &str = "bsi::ifc::prop::Description";

    fn format_1() -> IfcxFile {
        validate::parse(FORMAT_1).unwrap()
    }

    /// The file as the open path parses it, with raw attribute values.
    fn format_1_raw() -> IfcxFile<&'static RawValue> {
        validate::parse_raw(FORMAT_1).unwrap()
    }

    fn describe(file: &mut IfcxFile, text: &str) {
        for node in &mut file.data {
            if node.attributes.contains_key(schema::PROJECT) {
                let old = node
                    .attributes
                    .get(DESCRIPTION)
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let new = if old.is_empty() {
                    text.to_owned()
                } else {
                    format!("{old}; {text}")
                };
                node.attributes.insert(DESCRIPTION.into(), new.into());
            }
        }
    }

    /// The synthetic step of the test registry: format 2 describes the
    /// project.
    fn first(file: &mut IfcxFile, _: &mut Vec<Problem>) {
        describe(file, "described by the 1 to 2 step");
    }

    fn second(file: &mut IfcxFile, _: &mut Vec<Problem>) {
        describe(file, "then by the 2 to 3 step");
    }

    const TEST_MIGRATIONS: &[Migration] = &[
        // Out of order on purpose: steps are found by their format.
        Migration {
            from: 2,
            description: "the description is extended",
            apply: second,
        },
        Migration {
            from: 1,
            description: "the project gets a description",
            apply: first,
        },
    ];

    fn messages(problems: &[Problem], severity: Severity) -> Vec<&str> {
        problems
            .iter()
            .filter(|p| p.severity == severity)
            .map(|p| p.message.as_str())
            .collect()
    }

    /// IFX-091, IFX-062: a file at format 1 is upgraded by a v1 to v2 step,
    /// the migrated file validates, the step's change is in the loaded
    /// project and the migration is reported as info.
    #[test]
    fn a_synthetic_step_upgrades_format_1_and_its_result_validates() {
        let outcome = reader::read_with(&format_1(), TEST_MIGRATIONS, 2).unwrap();
        assert_eq!(outcome.project.info.description, "described by the 1 to 2 step");
        assert_eq!(
            messages(&outcome.problems, Severity::Info),
            ["migrated from format 1 to format 2: the project gets a description"]
        );
        assert_eq!(outcome.problems.len(), 1, "{:#?}", outcome.problems);
        assert!(!outcome.newer_format);
        assert_eq!(outcome.source_version, "ifcx_alpha");
        assert_eq!(outcome.migrated_from, Some(1));
        assert_eq!(outcome.project.elements.len(), 12);

        let mut problems = Vec::new();
        let (file, from) = upgrade(&format_1(), TEST_MIGRATIONS, 2, &mut problems).unwrap();
        assert_eq!(from, 1);
        assert_eq!(format_version(&file).unwrap().1, 2);
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
    }

    /// NFR-SEC-003, IFX-091: a file opened with raw attribute values is
    /// converted to trees for its steps and read as the same file parsed
    /// into trees is.
    #[test]
    fn a_raw_file_is_migrated_like_a_tree() {
        let raw = format_1_raw();
        assert_eq!(raw.to_values().unwrap(), format_1());
        for target in [2, 3] {
            let from_raw = reader::read_with(&raw, TEST_MIGRATIONS, target).unwrap();
            let from_tree = reader::read_with(&format_1(), TEST_MIGRATIONS, target).unwrap();
            assert_eq!(from_raw.project, from_tree.project);
            assert_eq!(from_raw.problems, from_tree.problems);
            assert_eq!(from_raw.migrated_from, Some(1));
        }
        let mut problems = Vec::new();
        let (file, from) = upgrade(&raw, TEST_MIGRATIONS, 2, &mut problems).unwrap();
        assert_eq!(from, 1);
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
    }

    /// The steps of a chain run in format order, each reported.
    #[test]
    fn a_chain_runs_its_steps_in_format_order() {
        let outcome = reader::read_with(&format_1(), TEST_MIGRATIONS, 3).unwrap();
        assert_eq!(
            outcome.project.info.description,
            "described by the 1 to 2 step; then by the 2 to 3 step"
        );
        assert_eq!(
            messages(&outcome.problems, Severity::Info),
            [
                "migrated from format 1 to format 2: the project gets a description",
                "migrated from format 2 to format 3: the description is extended",
            ]
        );
        assert_eq!(outcome.problems.len(), 2, "{:#?}", outcome.problems);
        assert_eq!(outcome.migrated_from, Some(1));
    }

    /// After migrating, the file declares the current `archicraft::`
    /// schemas: an old declaration that the migrated data no longer matches
    /// does not fail validation.
    #[test]
    fn a_migrated_file_declares_the_current_schemas() {
        let mut old = format_1();
        old.schemas.shift_remove(schema::SLAB);
        old.schemas.insert(
            "archicraft::retired".into(),
            serde_json::json!({"value": {"dataType": "String"}}),
        );
        assert!(!validate::validate(&old).is_empty());
        let (file, _) = upgrade(&old, TEST_MIGRATIONS, 2, &mut Vec::new()).unwrap();
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
        assert!(!file.schemas.contains_key("archicraft::retired"));
        assert_eq!(file.schemas.get(schema::SLAB), schema::declarations().get(schema::SLAB));
    }

    /// A file at the current format is read without copying or reporting,
    /// with the test chain as with the registered one: `upgrade` returns no
    /// migrated copy, so the reader reads the file it was given.
    #[test]
    fn a_file_at_the_target_format_is_left_as_it_is() {
        let file = format_1();
        let mut problems = Vec::new();
        assert_eq!(upgrade(&file, TEST_MIGRATIONS, 1, &mut problems), None);
        assert!(problems.is_empty());
        assert_eq!(upgrade(&file, MIGRATIONS, FORMAT_VERSION, &mut problems), None);
        assert!(problems.is_empty());
        let raw = format_1_raw();
        assert!(upgrade(&raw, MIGRATIONS, FORMAT_VERSION, &mut problems).is_none());
        assert!(problems.is_empty());
        assert_eq!(reader::read(&raw).unwrap().migrated_from, None);
    }

    /// A format without a step is reported and read as it is; a file that
    /// no step could upgrade at all counts as not migrated.
    #[test]
    fn a_missing_step_is_reported() {
        let outcome = reader::read_with(&format_1(), &[], 2).unwrap();
        assert_eq!(
            messages(&outcome.problems, Severity::Warning),
            ["no migration from ArchiCraft format 1 is known; the file is read as it is"]
        );
        assert_eq!(outcome.migrated_from, None);

        let mut problems = Vec::new();
        let (file, from) = upgrade(&format_1(), &TEST_MIGRATIONS[1..], 3, &mut problems).unwrap();
        assert_eq!(from, 1);
        assert_eq!(
            messages(&problems, Severity::Warning),
            ["no migration from ArchiCraft format 2 is known; the file is read as it is"]
        );
        assert_eq!(format_version(&file).unwrap().1, 2);
    }

    /// The registered chain has exactly one step for every format before
    /// [`FORMAT_VERSION`] and none beyond it.
    #[test]
    fn the_registered_migrations_form_a_chain_to_the_current_format() {
        let mut froms: Vec<u32> = MIGRATIONS.iter().map(|m| m.from).collect();
        froms.sort_unstable();
        let earlier: Vec<u32> = (1..).take_while(|v| *v < FORMAT_VERSION).collect();
        assert_eq!(froms, earlier);
    }
}
