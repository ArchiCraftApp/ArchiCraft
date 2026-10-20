//! The model returns message keys, not English text (NFR-L10N-001), and stays
//! free of user-interface and localization dependencies (NFR-MNT-002).

#![allow(clippy::unwrap_used)]

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use archicraft_model::command::{JournalEntry, StoreyElements, replay};
use archicraft_model::glam::DVec2;
use archicraft_model::kinds::KINDS;
use archicraft_model::message::{ALL_KEYS, ARG_NAMES};
use archicraft_model::template::default_metric_project;
use archicraft_model::{Arg, Command, History, KindTag, Layer, Message, Project, Storey};
use common::{ground, wall};

/// NFR-MNT-002, NFR-L10N-001: the catalogue lives in the application; the
/// model depends on no UI or localization crate.
#[test]
fn the_model_has_no_ui_or_localization_dependency() {
    let manifest = include_str!("../Cargo.toml");
    for line in manifest.lines().map(str::trim) {
        let name = line.split(['=', '.', ' ']).next().unwrap_or_default();
        for forbidden in ["egui", "eframe", "fluent", "unic-langid", "accesskit", "archicraft-app"] {
            assert!(
                !name.starts_with(forbidden) && !line.contains(&format!("\"{forbidden}")),
                "archicraft-model must not depend on {forbidden}: {line}"
            );
        }
    }
    assert!(manifest.contains("[dependencies]"), "the manifest was read");
}

/// Every command variant, in declaration order. [`variant`] has no wildcard
/// arm, so a new variant does not compile until it is listed here and built
/// in [`one_of_each`].
const VARIANTS: &[&str] = &[
    "AddElements",
    "DeleteElements",
    "UpdateElements",
    "MoveElements",
    "InsertStorey",
    "DeleteStorey",
    "RenameStorey",
    "SetStoreyElevation",
    "SetStoreyHeight",
    "SetStoreyCutPlane",
    "AddLayer",
    "UpdateLayer",
    "DeleteLayer",
    "AddMaterial",
    "UpdateMaterial",
    "DeleteMaterial",
    "SetToolDefaults",
    "SetSettings",
    "SetInfo",
    "SetSpatialNames",
    "SetLocation",
];

fn variant(command: &Command) -> &'static str {
    match command {
        Command::AddElements(_) => "AddElements",
        Command::DeleteElements(_) => "DeleteElements",
        Command::UpdateElements(_) => "UpdateElements",
        Command::MoveElements { .. } => "MoveElements",
        Command::InsertStorey { .. } => "InsertStorey",
        Command::DeleteStorey { .. } => "DeleteStorey",
        Command::RenameStorey { .. } => "RenameStorey",
        Command::SetStoreyElevation { .. } => "SetStoreyElevation",
        Command::SetStoreyHeight { .. } => "SetStoreyHeight",
        Command::SetStoreyCutPlane { .. } => "SetStoreyCutPlane",
        Command::AddLayer(_) => "AddLayer",
        Command::UpdateLayer(_) => "UpdateLayer",
        Command::DeleteLayer { .. } => "DeleteLayer",
        Command::AddMaterial(_) => "AddMaterial",
        Command::UpdateMaterial(_) => "UpdateMaterial",
        Command::DeleteMaterial { .. } => "DeleteMaterial",
        Command::SetToolDefaults(_) => "SetToolDefaults",
        Command::SetSettings(_) => "SetSettings",
        Command::SetInfo(_) => "SetInfo",
        Command::SetSpatialNames { .. } => "SetSpatialNames",
        Command::SetLocation(_) => "SetLocation",
    }
}

/// At least one command of every variant, including both label forms of
/// `AddElements`.
fn one_of_each(p: &Project) -> Vec<Command> {
    let g = ground(p);
    let w = wall(p, (0.0, 0.0), (5.0, 0.0));
    let layer = p.layers[0].clone();
    let material = p.materials[0].clone();
    vec![
        Command::AddElements(vec![w.clone()]),
        Command::AddElements(vec![w.clone(), wall(p, (0.0, 1.0), (5.0, 1.0))]),
        Command::DeleteElements(vec![w.id]),
        Command::UpdateElements(vec![w.clone()]),
        Command::MoveElements {
            ids: vec![w.id, w.id],
            delta: DVec2::X,
        },
        Command::InsertStorey {
            reference: g,
            above: true,
            storey: Storey::new("02", 6.0, 3.0),
        },
        Command::DeleteStorey {
            storey: g,
            elements: StoreyElements::Delete,
        },
        Command::RenameStorey {
            storey: g,
            name: "EG".into(),
        },
        Command::SetStoreyElevation {
            storey: g,
            elevation: 0.1,
            shift_above: true,
        },
        Command::SetStoreyHeight {
            storey: g,
            height: 3.2,
            shift_above: true,
        },
        Command::SetStoreyCutPlane {
            storey: g,
            height: Some(1.2),
        },
        Command::AddLayer(Layer::new("New")),
        Command::UpdateLayer(layer.clone()),
        Command::DeleteLayer {
            layer: layer.id,
            reassign_to: p.layers[1].id,
        },
        Command::AddMaterial(material.clone()),
        Command::UpdateMaterial(material.clone()),
        Command::DeleteMaterial {
            material: material.id,
            replacement: p.materials[1].id,
        },
        Command::SetToolDefaults(p.tool_defaults.clone()),
        Command::SetSettings(p.settings.clone()),
        Command::SetInfo(p.info.clone()),
        Command::SetSpatialNames {
            site: "S".into(),
            building: "B".into(),
        },
        Command::SetLocation(p.site.location),
    ]
}

/// NFR-L10N-001: every command label is a known key with known arguments.
#[test]
fn every_command_label_is_a_known_key() {
    let p = default_metric_project();
    let commands = one_of_each(&p);
    let built: BTreeSet<&str> = commands.iter().map(variant).collect();
    assert_eq!(built, VARIANTS.iter().copied().collect::<BTreeSet<_>>());
    for command in &commands {
        let label = command.label();
        assert!(ALL_KEYS.contains(&label.key()), "{label} of {}", variant(command));
        for (name, _) in label.args() {
            assert!(ARG_NAMES.contains(name), "{label}: argument {name}");
        }
    }
}

/// NFR-L10N-001: labels name the element kind and the count as arguments, so
/// translators get whole sentences.
#[test]
fn labels_carry_the_kind_and_the_count_as_arguments() {
    let p = default_metric_project();
    let single = Command::AddElements(vec![wall(&p, (0.0, 0.0), (5.0, 0.0))]).label();
    assert_eq!(
        single,
        Message::new("model-label-create-kind").kind("kind", KindTag::Wall)
    );
    let ids = vec![wall(&p, (0.0, 0.0), (5.0, 0.0)).id; 3];
    let delete = Command::DeleteElements(ids).label();
    assert_eq!(delete.key(), "model-label-delete-elements");
    assert_eq!(delete.arg("count"), Some(&Arg::Count(3)));
    assert_eq!(delete.to_string(), "model-label-delete-elements(count=3)");
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

/// The `"model-…"` string literals of `source`.
fn model_keys(source: &str) -> Vec<String> {
    source
        .match_indices("\"model-")
        .filter_map(|(at, _)| {
            let rest = &source[at + 1..];
            rest.find('"').map(|end| rest[..end].to_owned())
        })
        .collect()
}

/// NFR-L10N-001: the source scan finds every key the model sources write
/// (`Message::new("model-…")`, the kind descriptors' keys) in ALL_KEYS, and
/// every listed key is used somewhere, so the catalogue test in the
/// application covers exactly what the model emits.
#[test]
fn every_key_in_the_sources_is_listed_in_all_keys() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut used = BTreeSet::new();
    for file in &files {
        if file.ends_with("message.rs") {
            continue;
        }
        let source = std::fs::read_to_string(file).unwrap();
        for key in model_keys(&source) {
            assert!(
                ALL_KEYS.contains(&key.as_str()),
                "{key} in {} is missing from message::ALL_KEYS",
                file.display()
            );
            used.insert(key);
        }
    }
    let unused: Vec<&&str> = ALL_KEYS.iter().filter(|k| !used.contains(**k)).collect();
    assert!(unused.is_empty(), "keys in ALL_KEYS that no source uses: {unused:?}");
    assert!(used.contains("model-label-create-kind"), "the scan reached command.rs");
    for d in KINDS {
        assert!(
            ALL_KEYS.contains(&d.label_key) && ALL_KEYS.contains(&d.noun_key),
            "{}",
            d.code
        );
    }
}

#[test]
fn the_scanner_finds_model_keys() {
    let source = r#"Message::new("model-label-new-layer"); let x = "model-kind-wall"; "other""#;
    assert_eq!(model_keys(source), ["model-label-new-layer", "model-kind-wall"]);
}

/// NFR-MNT-001: the journal, with its message arguments, survives a serde
/// round trip unchanged, and replay still reproduces the session.
#[test]
fn the_journal_round_trips_through_serde() {
    let mut p = default_metric_project();
    let start = p.clone();
    let mut h = History::default();
    let g = ground(&p);
    let w = wall(&p, (0.0, 0.0), (5.0, 0.0));
    let id = w.id;
    h.execute(&mut p, Command::AddElements(vec![w])).unwrap();
    h.execute(
        &mut p,
        Command::MoveElements {
            ids: vec![id],
            delta: DVec2::Y,
        },
    )
    .unwrap();
    h.execute(
        &mut p,
        Command::RenameStorey {
            storey: g,
            name: "EG".into(),
        },
    )
    .unwrap();
    h.undo(&mut p).unwrap();
    h.redo(&mut p).unwrap();

    let mut journal: Vec<JournalEntry> = h.journal().cloned().collect();
    assert_eq!(journal.len(), 5);
    // Every argument type, nested too.
    journal.push(JournalEntry::Executed(
        Message::new("model-label-create-elements")
            .count("count", 2)
            .text("kind", "A “b”")
            .length("kind", 0.365)
            .angle("kind", -0.5)
            .kind("kind", KindTag::Slab)
            .nested(
                "kind",
                Message::new("model-label-create-kind").kind("kind", KindTag::Wall),
            ),
    ));
    let json = serde_json::to_string(&journal).unwrap();
    let read: Vec<JournalEntry> = serde_json::from_str(&json).unwrap();
    assert_eq!(read, journal);

    let mut replayed = start.clone();
    let commands: Vec<Command> = h.applied_commands().cloned().collect();
    let replayed_history = replay(&mut replayed, &commands).unwrap();
    replayed.revision = p.revision;
    assert_eq!(replayed, p);
    assert_eq!(replayed_history.journal().count(), 3);
}
