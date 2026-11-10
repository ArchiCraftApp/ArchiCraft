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
//! Whenever a reader-visible shape changes, or the format stores a new kind,
//! bump [`FORMAT_VERSION`](schema::FORMAT_VERSION) and add a step to
//! [`MIGRATIONS`]; other optional additions need neither. Older files hold
//! no elements of a new kind, so its step adds only what every project of
//! the new format has, such as the template's text styles of format 3, or
//! nothing, as for spaces: it exists so that a reader of the older format
//! flags the new files as newer (IFX-062) instead of skipping what it cannot
//! read.
//!
//! A step works on [`Value`] trees, so a file opened with raw attribute
//! values ([`crate::validate::parse_raw`]) is converted to trees before its
//! first step: a migrated file is held as a tree of the whole file, with the
//! memory that takes (NFR-SEC-003), until it has been read.

use std::collections::HashSet;

use archicraft_model::KindTag;
use archicraft_model::template::template_text_styles;
use serde_json::Value;

use super::kinds::annotations;
use super::schema;
use super::{relations, writer};
use crate::container::{AttributeValue, IfcxFile};
use crate::message::Message;
use crate::validate::Problem;

/// One step of the migration chain: upgrades a file from format `from` to
/// format `from + 1`.
#[derive(Debug)]
pub struct Migration {
    pub from: u32,
    /// The message key of what the step changes, for the load report; it
    /// belongs in [`crate::message::ALL_KEYS`] (NFR-L10N-001).
    pub description: &'static str,
    /// Rewrites the file in place; reports what it could not convert.
    pub apply: fn(&mut IfcxFile, &mut Vec<Problem>),
}

/// The migration chain up to [`FORMAT_VERSION`](schema::FORMAT_VERSION), one
/// step per earlier format, in order.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        from: 1,
        description: "ifcx-migration-adds-spaces",
        apply: adds_spaces,
    },
    Migration {
        from: 2,
        description: "ifcx-migration-adds-texts",
        apply: adds_texts,
    },
];

/// Format 2 stores spaces (SPC-001) and, for a node that shows another name
/// than its element's, the element's own `name` in `archicraft::element`.
/// A format 1 file has neither, so nothing changes.
fn adds_spaces(_: &mut IfcxFile, _: &mut Vec<Problem>) {}

/// Format 3 stores texts as annotation entries and the project's text
/// styles as `textStyles` of `archicraft::project` (ANN-007). A format 2
/// file has no texts, and its project gets the template's text styles, as
/// every project from the template has them; a project that has a
/// `textStyles` member keeps it.
fn adds_texts(file: &mut IfcxFile, _: &mut Vec<Problem>) {
    let styles: Vec<Value> = template_text_styles().iter().map(writer::text_style_record).collect();
    for node in &mut file.data {
        if let Some(project) = node.attributes.get_mut(schema::PROJECT).and_then(Value::as_object_mut) {
            project
                .entry("textStyles")
                .or_insert_with(|| Value::Array(styles.clone()));
        }
    }
}

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

/// A format version as a message count.
fn count(version: u32) -> usize {
    usize::try_from(version).unwrap_or(usize::MAX)
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
                Message::new("ifcx-migration-unknown").count("version", count(version)),
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
                        Message::new("ifcx-migration-failed")
                            .count("version", count(version))
                            .text("detail", e.to_string()),
                    ));
                    return None;
                }
            },
        };
        (step.apply)(tree, problems);
        problems.push(Problem::info(
            Some(&path),
            Message::new("ifcx-migrated")
                .count("from", count(version))
                .count("to", count(version + 1))
                .nested("change", Message::new(step.description)),
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
    // The declarations follow from the content (IFX-061): the kinds the
    // file stores and, when a node carries relations, `archicraft::relations`.
    file.schemas
        .extend(schema::declarations(&present(&file), relations::present_in(&file)));
    Some((file, from))
}

/// The kinds the migrated file stores, as [`super::kinds::present`] finds
/// them in a project: a defaults slot, a non-zero counter or an element of
/// the kind, a node or an annotation entry, in registry order. The migrated
/// file declares these, like a file the writer writes (IFX-061).
fn present(file: &IfcxFile) -> Vec<KindTag> {
    let mut codes: HashSet<&str> = HashSet::new();
    for node in &file.data {
        if let Some(project) = node.attributes.get(schema::PROJECT) {
            let slots = project["toolDefaults"].as_object().into_iter().flatten();
            codes.extend(slots.map(|(code, _)| code.as_str()));
            let counters = project["counters"].as_object().into_iter().flatten();
            codes.extend(
                counters
                    .filter(|(_, n)| n.as_i64().is_some_and(|n| n != 0))
                    .map(|(code, _)| code.as_str()),
            );
        }
        if let Some(kind) = node.attributes.get(schema::ELEMENT).and_then(|e| e["kind"].as_str()) {
            codes.insert(kind);
        }
        let entries = node.attributes.get(annotations::ATTRIBUTE).and_then(Value::as_array);
        codes.extend(entries.into_iter().flatten().filter_map(|entry| entry["kind"].as_str()));
    }
    KindTag::ALL
        .iter()
        .copied()
        .filter(|kind| codes.contains(kind.code()))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::value::RawValue;

    use archicraft_model::samples;

    use super::*;
    use crate::alpha::reader;
    use crate::alpha::schema::FORMAT_VERSION;
    use crate::message::ALL_KEYS;
    use crate::validate::{self, Severity};

    /// A file written at format 1 (tests/formats/, never regenerated).
    const FORMAT_1: &[u8] = include_bytes!("../../tests/formats/format-1.ifcx");

    /// A file written at format 2, the format that added spaces.
    const FORMAT_2: &[u8] = include_bytes!("../../tests/formats/format-2.ifcx");

    const DESCRIPTION: &str = "bsi::ifc::prop::Description";

    /// The format after this release's, which the synthetic steps reach.
    const NEXT: u32 = FORMAT_VERSION + 1;

    fn format_1() -> IfcxFile {
        validate::parse(FORMAT_1).unwrap()
    }

    /// `project` saved as this release writes it.
    fn saved(project: &archicraft_model::Project) -> Vec<u8> {
        let geometry = archicraft_geometry::ModelGeometry::evaluate(project);
        let options = crate::WriteOptions {
            author: "Test".into(),
            timestamp: "2026-11-09T12:00:00Z".into(),
            data_version: 1,
        };
        let (bytes, warnings) = crate::save(project, &geometry, &options, false);
        assert!(warnings.is_empty(), "{warnings:?}");
        bytes
    }

    /// The two-storey shell as this release writes it, at
    /// [`FORMAT_VERSION`], the file the synthetic steps upgrade.
    fn current() -> IfcxFile {
        validate::parse(&saved(&samples::two_storey_shell())).unwrap()
    }

    /// The same file as the open path parses it, with raw attribute values.
    fn current_raw(bytes: &[u8]) -> IfcxFile<&RawValue> {
        validate::parse_raw(bytes).unwrap()
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

    /// The synthetic steps of the test registry, after the registered
    /// chain: the next format describes the project, the one after it
    /// extends the description.
    fn first(file: &mut IfcxFile, _: &mut Vec<Problem>) {
        describe(file, "described by the first step");
    }

    fn second(file: &mut IfcxFile, _: &mut Vec<Problem>) {
        describe(file, "then by the second step");
    }

    const TEST_MIGRATIONS: &[Migration] = &[
        // Out of order on purpose: steps are found by their format.
        Migration {
            from: NEXT,
            description: "test-description-extended",
            apply: second,
        },
        Migration {
            from: FORMAT_VERSION,
            description: "test-description-added",
            apply: first,
        },
    ];

    /// The messages of `severity` in their key form.
    fn messages(problems: &[Problem], severity: Severity) -> Vec<String> {
        problems
            .iter()
            .filter(|p| p.severity == severity)
            .map(|p| p.message.to_string())
            .collect()
    }

    /// The info of the step from `from` with `change`, in its key form.
    fn migrated(from: u32, change: &str) -> String {
        format!("ifcx-migrated(from={from}, to={}, change={change})", from + 1)
    }

    /// IFX-091, IFX-062: a file at this release's format is upgraded by a
    /// step to the next format, the migrated file validates, the step's
    /// change is in the loaded project and the migration is reported as
    /// info.
    #[test]
    fn a_synthetic_step_upgrades_a_file_and_its_result_validates() {
        let outcome = reader::read_with(&current(), TEST_MIGRATIONS, NEXT).unwrap();
        assert_eq!(outcome.project.info.description, "described by the first step");
        assert_eq!(
            messages(&outcome.problems, Severity::Info),
            [migrated(FORMAT_VERSION, "test-description-added")]
        );
        assert_eq!(outcome.problems.len(), 1, "{:#?}", outcome.problems);
        assert!(!outcome.newer_format);
        assert_eq!(outcome.source_version, "ifcx_alpha");
        assert_eq!(outcome.migrated_from, Some(FORMAT_VERSION));
        assert_eq!(outcome.project.elements.len(), 12);

        let mut problems = Vec::new();
        let (file, from) = upgrade(&current(), TEST_MIGRATIONS, NEXT, &mut problems).unwrap();
        assert_eq!(from, FORMAT_VERSION);
        assert_eq!(format_version(&file).unwrap().1, NEXT);
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
    }

    /// NFR-SEC-003, IFX-091: a file opened with raw attribute values is
    /// converted to trees for its steps and read as the same file parsed
    /// into trees is.
    #[test]
    fn a_raw_file_is_migrated_like_a_tree() {
        let bytes = saved(&samples::two_storey_shell());
        let raw = current_raw(&bytes);
        assert_eq!(raw.to_values().unwrap(), current());
        for target in [NEXT, NEXT + 1] {
            let from_raw = reader::read_with(&raw, TEST_MIGRATIONS, target).unwrap();
            let from_tree = reader::read_with(&current(), TEST_MIGRATIONS, target).unwrap();
            assert_eq!(from_raw.project, from_tree.project);
            assert_eq!(from_raw.problems, from_tree.problems);
            assert_eq!(from_raw.migrated_from, Some(FORMAT_VERSION));
        }
        let mut problems = Vec::new();
        let (file, from) = upgrade(&raw, TEST_MIGRATIONS, NEXT, &mut problems).unwrap();
        assert_eq!(from, FORMAT_VERSION);
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
    }

    /// The steps of a chain run in format order, each reported.
    #[test]
    fn a_chain_runs_its_steps_in_format_order() {
        let outcome = reader::read_with(&current(), TEST_MIGRATIONS, NEXT + 1).unwrap();
        assert_eq!(
            outcome.project.info.description,
            "described by the first step; then by the second step"
        );
        assert_eq!(
            messages(&outcome.problems, Severity::Info),
            [
                migrated(FORMAT_VERSION, "test-description-added"),
                migrated(NEXT, "test-description-extended"),
            ]
        );
        assert_eq!(outcome.problems.len(), 2, "{:#?}", outcome.problems);
        assert_eq!(outcome.migrated_from, Some(FORMAT_VERSION));
    }

    /// After migrating, the file declares the current `archicraft::`
    /// schemas: an old declaration that the migrated data no longer matches
    /// does not fail validation.
    #[test]
    fn a_migrated_file_declares_the_current_schemas() {
        let slab = "archicraft::slab";
        let mut old = current();
        old.schemas.shift_remove(slab);
        old.schemas.insert(
            "archicraft::retired".into(),
            serde_json::json!({"value": {"dataType": "String"}}),
        );
        assert!(!validate::validate(&old).is_empty());
        let (file, _) = upgrade(&old, TEST_MIGRATIONS, NEXT, &mut Vec::new()).unwrap();
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
        assert!(!file.schemas.contains_key("archicraft::retired"));
        assert_eq!(
            file.schemas.get(slab),
            schema::declarations(KindTag::ALL, false).get(slab)
        );
        assert!(
            !file.schemas.contains_key(relations::ATTRIBUTE),
            "no relations in the file"
        );
        // The file's kinds and no others, however many codecs this build has:
        // walls, slabs and the space and text tools' slots.
        let declared: Vec<&String> = file
            .schemas
            .keys()
            .filter(|id| id.starts_with("archicraft::"))
            .collect();
        assert_eq!(
            declared,
            [
                "archicraft::project",
                "archicraft::storey",
                "archicraft::element",
                "archicraft::wall",
                slab,
                "archicraft::space",
                annotations::ATTRIBUTE,
            ]
        );
    }

    /// SRS 3.3, IFX-062: a migrated file keeps declaring
    /// `archicraft::relations` when one of its nodes carries relations, so
    /// the relations of a file written at the current format survive the
    /// next migration and validate afterwards.
    #[test]
    fn a_migrated_file_with_relations_keeps_declaring_them() {
        let mut old = current();
        let elements: Vec<String> = old
            .data
            .iter()
            .filter(|n| n.attributes.contains_key(schema::ELEMENT))
            .map(|n| n.path.clone())
            .collect();
        assert!(elements.len() >= 2, "{elements:?}");
        let (first, second) = (&elements[0], &elements[1]);
        let node = old.data.iter_mut().find(|n| &n.path == first).unwrap();
        node.attributes.insert(
            relations::ATTRIBUTE.into(),
            serde_json::json!([{"kind": "host", "target": second}]),
        );
        old.schemas.extend(
            schema::declarations(&[], true)
                .into_iter()
                .filter(|(id, _)| id == relations::ATTRIBUTE),
        );
        assert!(validate::validate(&old).is_empty(), "{:#?}", validate::validate(&old));
        let (file, _) = upgrade(&old, TEST_MIGRATIONS, NEXT, &mut Vec::new()).unwrap();
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
        assert_eq!(
            file.schemas.get(relations::ATTRIBUTE),
            schema::declarations(&[], true).get(relations::ATTRIBUTE)
        );
    }

    /// A migrated file declares the kinds it stores, however the kind got
    /// there: a non-zero counter of the test kind, which no sample has,
    /// declares its attribute, its kind option and its optional counter, and
    /// the file validates.
    #[cfg(feature = "test-kinds")]
    #[test]
    fn a_migrated_file_declares_a_kind_its_counters_name() {
        let mut old = current();
        let project = old
            .data
            .iter_mut()
            .find_map(|node| node.attributes.get_mut(schema::PROJECT))
            .unwrap();
        project["counters"]["dummy"] = 2.into();
        let (file, _) = upgrade(&old, TEST_MIGRATIONS, NEXT, &mut Vec::new()).unwrap();
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
        let declared = &file.schemas["archicraft::dummy"]["value"]["objectRestrictions"]["values"];
        assert!(declared["origin"].is_object(), "{declared}");
        let project = &file.schemas[schema::PROJECT]["value"]["objectRestrictions"]["values"];
        assert_eq!(
            project["counters"]["objectRestrictions"]["values"]["dummy"],
            serde_json::json!({"dataType": "Integer", "optional": true})
        );
        let element = &file.schemas[schema::ELEMENT]["value"]["objectRestrictions"]["values"];
        assert_eq!(
            element["kind"]["enumRestrictions"]["options"],
            serde_json::json!(["wall", "slab", "space", "dummy"])
        );
    }

    /// ANN-010, IFX-062: a migrated file declares the members of every kind
    /// its annotation entries hold, the entry's kind found from the entry
    /// itself without a counter, so the annotations of a file written at the
    /// current format survive the next migration: the file validates,
    /// declares no attribute of the note's own and the note loads.
    #[cfg(feature = "test-kinds")]
    #[test]
    fn a_migrated_file_with_annotations_keeps_declaring_them() {
        let mut old = current();
        let project = old.data.iter().find_map(|n| n.attributes.get(schema::PROJECT)).unwrap();
        let layer = project["layers"][0]["id"].clone();
        let storey = old
            .data
            .iter_mut()
            .find(|n| n.attributes.contains_key(schema::STOREY))
            .unwrap();
        let id = uuid::Uuid::new_v4();
        let entry = serde_json::json!({
            "id": id.to_string(),
            "name": "NT-001",
            "ifcClass": "IfcAnnotation",
            "kind": "note",
            "layer": layer,
            "locked": false,
            "position": "undefined",
            "note": {"view": {"type": "plan", "storey": storey.path}, "at": [1.0, 2.0], "text": "Hall"},
        });
        storey
            .attributes
            .insert(annotations::ATTRIBUTE.into(), serde_json::json!([entry]));
        let (file, _) = upgrade(&old, TEST_MIGRATIONS, NEXT, &mut Vec::new()).unwrap();
        assert!(validate::validate(&file).is_empty(), "{:#?}", validate::validate(&file));
        let declared = &file.schemas[annotations::ATTRIBUTE]["value"]["arrayRestrictions"]["value"];
        assert!(
            declared["objectRestrictions"]["values"]["note"].is_object(),
            "{declared}"
        );
        assert!(!file.schemas.contains_key("archicraft::note"));

        let outcome = reader::read_with(&old, TEST_MIGRATIONS, NEXT).unwrap();
        let note = outcome
            .project
            .element(archicraft_model::ElementId::from_uuid(id))
            .unwrap();
        assert_eq!(note.name, "NT-001");
        assert_eq!(outcome.project.elements.len(), 13);
    }

    /// A file at the target format is read without copying or reporting,
    /// with the test chain as with the registered one: `upgrade` returns no
    /// migrated copy, so the reader reads the file it was given.
    #[test]
    fn a_file_at_the_target_format_is_left_as_it_is() {
        let mut problems = Vec::new();
        assert_eq!(
            upgrade(&current(), TEST_MIGRATIONS, FORMAT_VERSION, &mut problems),
            None
        );
        assert!(problems.is_empty());
        let bytes = saved(&samples::two_storey_shell());
        let file = validate::parse(&bytes).unwrap();
        assert_eq!(upgrade(&file, MIGRATIONS, FORMAT_VERSION, &mut problems), None);
        assert!(problems.is_empty());
        let raw = current_raw(&bytes);
        assert!(upgrade(&raw, MIGRATIONS, FORMAT_VERSION, &mut problems).is_none());
        assert!(problems.is_empty());
        assert_eq!(reader::read(&raw).unwrap().migrated_from, None);
    }

    /// IFX-062, SPC-001: spaces came with one step that changes nothing,
    /// because files of the formats before it hold no spaces; it exists so
    /// that a reader of those formats sees a file with spaces as newer.
    #[test]
    fn the_spaces_step_changes_nothing() {
        let steps: Vec<&Migration> = MIGRATIONS
            .iter()
            .filter(|m| m.description == "ifcx-migration-adds-spaces")
            .collect();
        assert_eq!(steps.len(), 1, "{MIGRATIONS:?}");
        let mut file = format_1();
        let mut problems = Vec::new();
        (steps[0].apply)(&mut file, &mut problems);
        assert_eq!(file, format_1());
        assert!(problems.is_empty(), "{problems:#?}");
    }

    /// IFX-062, ANN-007: texts came with the step from format 2, which gives
    /// the project of an older file the template's text styles, written as
    /// the writer writes them, and changes nothing else; a project that
    /// somehow has text styles keeps them.
    #[test]
    fn the_texts_step_adds_the_template_text_styles() {
        let step = MIGRATIONS.iter().find(|m| m.from == 2).unwrap();
        assert_eq!(step.description, "ifcx-migration-adds-texts");
        let original = validate::parse(FORMAT_2).unwrap();
        let mut file = original.clone();
        let mut problems = Vec::new();
        (step.apply)(&mut file, &mut problems);
        assert!(problems.is_empty(), "{problems:#?}");
        let project = file
            .data
            .iter_mut()
            .find_map(|n| n.attributes.get_mut(schema::PROJECT))
            .unwrap();
        let expected: Vec<Value> = template_text_styles().iter().map(writer::text_style_record).collect();
        assert_eq!(project["textStyles"], Value::Array(expected));
        project.as_object_mut().unwrap().remove("textStyles");
        assert_eq!(file, original, "nothing else changes");

        let mut kept = validate::parse(FORMAT_2).unwrap();
        let project = kept
            .data
            .iter_mut()
            .find_map(|n| n.attributes.get_mut(schema::PROJECT))
            .unwrap();
        project["textStyles"] = serde_json::json!([]);
        let before = kept.clone();
        (step.apply)(&mut kept, &mut problems);
        assert_eq!(kept, before);
    }

    /// IFX-062, SPC-001, ANN-007: a reader of an earlier format, the chain
    /// up to that format, flags a file with the kind the next format added
    /// as written by a newer version, so it is not overwritten without
    /// confirmation: a reader of format 1 a file with spaces, a reader of
    /// format 2 one with texts. This release reads both as current.
    #[test]
    fn an_older_reader_flags_a_file_of_a_later_format_as_newer() {
        for (project, format) in [
            (samples::two_storey_shell_with_spaces(), 1),
            (samples::annotation_sample(), 2),
        ] {
            let bytes = saved(&project);
            let file = validate::parse_raw(&bytes).unwrap();
            let older = &MIGRATIONS[..usize::try_from(format - 1).unwrap()];
            let outcome = reader::read_with(&file, older, format).unwrap();
            assert!(outcome.newer_format, "{}", project.info.name);
            let version = usize::try_from(FORMAT_VERSION).unwrap();
            assert!(
                messages(&outcome.problems, Severity::Warning)
                    .contains(&format!("ifcx-newer-format(version={version})")),
                "{:#?}",
                outcome.problems
            );
            let current = reader::read(&file).unwrap();
            assert!(!current.newer_format);
            assert!(current.problems.is_empty(), "{:#?}", current.problems);
        }
    }

    /// A format without a step is reported and read as it is; a file that
    /// no step could upgrade at all counts as not migrated.
    #[test]
    fn a_missing_step_is_reported() {
        let outcome = reader::read_with(&current(), &[], NEXT).unwrap();
        assert_eq!(
            messages(&outcome.problems, Severity::Warning),
            [format!("ifcx-migration-unknown(version={FORMAT_VERSION})")]
        );
        assert_eq!(outcome.migrated_from, None);

        let mut problems = Vec::new();
        let (file, from) = upgrade(&current(), &TEST_MIGRATIONS[1..], NEXT + 1, &mut problems).unwrap();
        assert_eq!(from, FORMAT_VERSION);
        assert_eq!(
            messages(&problems, Severity::Warning),
            [format!("ifcx-migration-unknown(version={NEXT})")]
        );
        assert_eq!(format_version(&file).unwrap().1, NEXT);
    }

    /// The registered chain has exactly one step for every format before
    /// [`FORMAT_VERSION`] and none beyond it.
    #[test]
    fn the_registered_migrations_form_a_chain_to_the_current_format() {
        let mut froms: Vec<u32> = MIGRATIONS.iter().map(|m| m.from).collect();
        froms.sort_unstable();
        let earlier: Vec<u32> = (1..).take_while(|v| *v < FORMAT_VERSION).collect();
        assert_eq!(froms, earlier);
        // NFR-L10N-001: the load report renders each step's description.
        assert!(MIGRATIONS.iter().all(|m| ALL_KEYS.contains(&m.description)));
    }
}
