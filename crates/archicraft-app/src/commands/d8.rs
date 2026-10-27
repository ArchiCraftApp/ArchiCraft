//! The default keymap of Appendix D.8 (UI-008, HW-003): one entry per
//! command the SRS requires a default shortcut for, with the bindings
//! reserved for it. Most of these commands arrive with later epics; their
//! entry names the epic, and the binding stays reserved so nothing else takes
//! it. Once the epic registers the command, the entry names its id and the
//! tests require the command's default bindings to be exactly the reserved
//! ones. docs/keymap.md is generated from this table and the registry
//! ([`markdown`]).

use egui::Key;
use egui::os::OperatingSystem;

use super::{Binding, CommandId, Registry, Scope, edit, os_reserved, tools, view};

/// The four groups of Appendix D.8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    FileEdit,
    Transform,
    Tool,
    View,
}

impl Group {
    fn title(self) -> &'static str {
        match self {
            Group::FileEdit => "File and edit",
            Group::Transform => "Transforms and reshaping",
            Group::Tool => "Tools",
            Group::View => "Views",
        }
    }
}

/// Who provides the command of a D.8 entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The registered command.
    Registered(CommandId),
    /// Not registered yet: the key of the roadmap epic (docs/roadmap.md)
    /// that adds it.
    Pending { owner_epic: &'static str },
}

/// One command of Appendix D.8.
#[derive(Clone, Copy, Debug)]
pub struct Item {
    /// The command's name as Appendix D.8 spells it ("next/previous tab" is
    /// two entries).
    pub srs: &'static str,
    pub group: Group,
    /// The reserved default bindings; the first is shown in menus.
    pub bindings: &'static [Binding],
    pub scope: Scope,
    pub owner: Owner,
}

const fn item(srs: &'static str, group: Group, bindings: &'static [Binding], scope: Scope, owner: Owner) -> Item {
    Item {
        srs,
        group,
        bindings,
        scope,
        owner,
    }
}

const fn registered(id: CommandId) -> Owner {
    Owner::Registered(id)
}

const fn pending(owner_epic: &'static str) -> Owner {
    Owner::Pending { owner_epic }
}

/// A registered command whose module exports no constant for its id.
const fn command(id: &'static str) -> Owner {
    Owner::Registered(CommandId::new(id))
}

use Group::*;
use Scope::{Canvas, Global, TextSafe};

/// The commands of Appendix D.8 in its order, with their reserved bindings:
/// Primary letters for file, edit and transform commands, single letters
/// (with Shift or Alt for a second command on the same letter) for tools and
/// view toggles, and nothing an operating system reserves
/// ([`os_reserved`]). docs/keymap.md gives the reasons.
pub const ITEMS: [Item; 38] = [
    item(
        "save",
        FileEdit,
        &[Binding::primary(Key::S)],
        TextSafe,
        command("file.save"),
    ),
    item(
        "undo",
        FileEdit,
        &[Binding::primary(Key::Z)],
        Global,
        registered(edit::UNDO),
    ),
    item(
        "redo",
        FileEdit,
        &[Binding::primary(Key::Z).shift(), Binding::primary(Key::Y)],
        Global,
        registered(edit::REDO),
    ),
    item(
        "cut",
        FileEdit,
        &[Binding::primary(Key::X)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "copy",
        FileEdit,
        &[Binding::primary(Key::C)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "paste",
        FileEdit,
        &[Binding::primary(Key::V)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "delete",
        FileEdit,
        &[Binding::key(Key::Delete), Binding::key(Key::Backspace)],
        Canvas,
        command("edit.delete"),
    ),
    item(
        "select all",
        FileEdit,
        &[Binding::primary(Key::A)],
        Global,
        command("edit.select-all"),
    ),
    // The palette itself is archicraft-fw0 of this epic.
    item(
        "command palette",
        FileEdit,
        &[Binding::primary(Key::P).shift()],
        TextSafe,
        pending("commands-and-l10n"),
    ),
    item(
        "move",
        Transform,
        &[Binding::primary(Key::D)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "copy-move",
        Transform,
        &[Binding::primary(Key::D).shift()],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "rotate",
        Transform,
        &[Binding::primary(Key::E)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "mirror",
        Transform,
        &[Binding::primary(Key::E).shift()],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "vertical move",
        Transform,
        &[Binding::primary(Key::U)],
        Global,
        pending("selection-transforms"),
    ),
    item(
        "array",
        Transform,
        &[Binding::primary(Key::A).shift()],
        Global,
        pending("drafting-trim-split"),
    ),
    item(
        "trim",
        Transform,
        &[Binding::primary(Key::K).shift()],
        Global,
        pending("drafting-trim-split"),
    ),
    item(
        "split",
        Transform,
        &[Binding::primary(Key::K)],
        Global,
        pending("drafting-trim-split"),
    ),
    item(
        "offset",
        Transform,
        &[Binding::primary(Key::O).shift()],
        Global,
        pending("edit-reshape-palette"),
    ),
    item(
        "measure",
        Tool,
        &[Binding::key(Key::M)],
        Canvas,
        pending("3d-input-navigation"),
    ),
    item("wall", Tool, &[Binding::key(Key::W)], Canvas, registered(tools::WALL)),
    item("slab", Tool, &[Binding::key(Key::S)], Canvas, registered(tools::SLAB)),
    item(
        "column",
        Tool,
        &[Binding::key(Key::C)],
        Canvas,
        pending("profiles-columns-beams"),
    ),
    item(
        "beam",
        Tool,
        &[Binding::key(Key::B)],
        Canvas,
        pending("profiles-columns-beams"),
    ),
    // R cycles the wall reference line (Global), so the roof takes G.
    item("roof", Tool, &[Binding::key(Key::G)], Canvas, pending("roofs")),
    item(
        "opening",
        Tool,
        &[Binding::key(Key::D).shift()],
        Canvas,
        pending("host-openings"),
    ),
    item(
        "door",
        Tool,
        &[Binding::key(Key::D)],
        Canvas,
        pending("doors-windows-core"),
    ),
    // W is the wall, so the window takes N.
    item(
        "window",
        Tool,
        &[Binding::key(Key::N)],
        Canvas,
        pending("doors-windows-core"),
    ),
    item(
        "stair",
        Tool,
        &[Binding::key(Key::S).shift()],
        Canvas,
        pending("stair-solver-tool"),
    ),
    item(
        "free-form solid",
        Tool,
        &[Binding::key(Key::M).shift()],
        Canvas,
        pending("free-form-solids"),
    ),
    // F3 needs Fn on a Mac keyboard, where it is a media key: Primary+3
    // works without.
    item(
        "switch between plan and 3D",
        View,
        &[Binding::key(Key::F3), Binding::primary(Key::Num3)],
        Canvas,
        registered(view::SWITCH_PLAN_3D),
    ),
    item(
        "next tab",
        View,
        &[Binding::ctrl(Key::Tab)],
        Canvas,
        command("view.next-tab"),
    ),
    item(
        "previous tab",
        View,
        &[Binding::ctrl(Key::Tab).shift()],
        Global,
        command("view.previous-tab"),
    ),
    item(
        "fit to window",
        View,
        &[Binding::key(Key::F)],
        Canvas,
        registered(view::FIT_WINDOW),
    ),
    item(
        "zoom to selection",
        View,
        &[Binding::key(Key::F).shift()],
        Global,
        command("view.zoom-selection"),
    ),
    item("orbit", View, &[Binding::key(Key::O)], Canvas, registered(view::ORBIT)),
    item(
        "show all",
        View,
        &[Binding::key(Key::H).alt()],
        Canvas,
        pending("3d-view-restrictions"),
    ),
    item(
        "hide selection",
        View,
        &[Binding::key(Key::H)],
        Canvas,
        pending("3d-view-restrictions"),
    ),
    item(
        "isolate selection",
        View,
        &[Binding::key(Key::H).shift()],
        Canvas,
        pending("3d-view-restrictions"),
    ),
];

/// The D.8 entry of the registered command `id`, if it is one.
pub fn item_for(id: CommandId) -> Option<&'static Item> {
    ITEMS.iter().find(|item| item.owner == Owner::Registered(id))
}

/// The scope in the words of docs/keymap.md.
fn scope_text(scope: Scope) -> &'static str {
    match scope {
        Scope::TextSafe => "always",
        Scope::Global => "not while typing",
        Scope::Canvas => "not while typing or drawing",
        Scope::PointInput => "only while drawing",
    }
}

/// The bindings on `os`, separated by commas.
fn keys(bindings: &[Binding], os: OperatingSystem) -> String {
    bindings
        .iter()
        .map(|b| format!("`{}`", b.display(os)))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The generated part of docs/keymap.md: the D.8 table, the other default
/// shortcuts of the registry with their labels in the current language, and
/// the OS-reserved tables, with macOS and Windows/Linux spelled by
/// [`Binding::display`].
pub fn markdown() -> String {
    let (mac, other) = (OperatingSystem::Mac, OperatingSystem::Windows);
    let mut out = String::new();
    out.push_str("## Appendix D.8 commands\n\n");
    out.push_str("| Command | Group | macOS | Windows and Linux | Active | Command or owner epic |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for item in &ITEMS {
        let owner = match item.owner {
            Owner::Registered(id) => format!("`{id}`"),
            Owner::Pending { owner_epic } => format!("pending: `{owner_epic}`"),
        };
        let mut name = item.srs.to_owned();
        name[..1].make_ascii_uppercase();
        out.push_str(&format!(
            "| {name} | {} | {} | {} | {} | {owner} |\n",
            item.group.title(),
            keys(item.bindings, mac),
            keys(item.bindings, other),
            scope_text(item.scope),
        ));
    }
    out.push_str("\n## Other default shortcuts\n\n");
    out.push_str("| Command | macOS | Windows and Linux | Active | Command |\n");
    out.push_str("|---|---|---|---|---|\n");
    for spec in Registry::get().specs() {
        if spec.default_bindings.is_empty() || item_for(spec.id).is_some() {
            continue;
        }
        out.push_str(&format!(
            "| {} | {} | {} | {}{} | `{}` |\n",
            spec.label(),
            keys(&spec.default_bindings, mac),
            keys(&spec.default_bindings, other),
            scope_text(spec.scope),
            if spec.local { ", matched by the plan view" } else { "" },
            spec.id,
        ));
    }
    out.push_str("\n## Reserved by the operating system\n");
    for (os, title) in os_reserved::PLATFORMS
        .into_iter()
        .zip(["macOS", "Windows", "Linux (GNOME and KDE)"])
    {
        out.push_str(&format!("\n### {title}\n\n| Keys | Used for |\n|---|---|\n"));
        for reserved in os_reserved::reserved(os) {
            out.push_str(&format!("| `{}` | {} |\n", reserved.display(os), reserved.purpose));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::super::{Category, Registry};
    use super::*;
    use crate::i18n::Localizer;

    /// A file of the repository, relative to the workspace root.
    fn repo_file(path: &str) -> String {
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path);
        std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
    }

    /// The commands of Appendix D.8 of docs/SRS.md in their order, as the
    /// bullets name them: "measure, and each element tool (wall, …)" lists
    /// measure and each tool, "next/previous tab" is next tab and previous tab.
    fn srs_d8_commands() -> Vec<String> {
        let srs = repo_file("docs/SRS.md");
        let start = srs.find("### D.8 ").expect("Appendix D.8 in the SRS");
        let section = &srs[start..];
        let section = &section[..section[4..].find("\n### ").map_or(section.len(), |e| e + 4)];
        let mut commands = Vec::new();
        for line in section.lines().filter(|l| l.starts_with("- **")) {
            let (_, list) = line.split_once(":**").expect("a D.8 bullet with a group name");
            let list = list
                .trim()
                .trim_end_matches(['.', ';'])
                .replace(", and each element tool (", ", ")
                .replace(')', "");
            for name in list.split(", ") {
                match name.split_once('/') {
                    Some((first, rest)) => {
                        let (second, noun) = rest.split_once(' ').expect("a/b noun");
                        commands.push(format!("{first} {noun}"));
                        commands.push(format!("{second} {noun}"));
                    }
                    None => commands.push(name.to_owned()),
                }
            }
        }
        commands
    }

    /// UI-008: every command of Appendix D.8 has exactly one entry in the D.8
    /// table, in the order of the SRS.
    #[test]
    fn every_appendix_d8_command_appears_exactly_once() {
        let srs = srs_d8_commands();
        assert_eq!(srs.len(), 38, "{srs:?}");
        assert!(srs.contains(&"previous tab".to_owned()) && srs.contains(&"free-form solid".to_owned()));
        let table: Vec<&str> = ITEMS.iter().map(|item| item.srs).collect();
        assert_eq!(table, srs);
        let mut ids = HashSet::new();
        for item in &ITEMS {
            if let Owner::Registered(id) = item.owner {
                assert!(ids.insert(id), "{id} is the command of two D.8 entries");
            }
        }
    }

    /// The epic keys of the epic table in docs/roadmap.md.
    fn roadmap_epic_keys() -> HashSet<String> {
        repo_file("docs/roadmap.md")
            .lines()
            .filter_map(|line| {
                let mut cells = line.split('|').map(str::trim);
                let number = cells.nth(1)?;
                let key = cells.next()?;
                (number.parse::<u32>().is_ok() && key.starts_with('`') && key.ends_with('`'))
                    .then(|| key.trim_matches('`').to_owned())
            })
            .collect()
    }

    /// UI-008, HW-003: a registered D.8 command has exactly the reserved
    /// default bindings, in the reserved scope; a pending one names the
    /// roadmap epic that registers it.
    #[test]
    fn appendix_d8_commands_have_their_reserved_bindings() {
        let epics = roadmap_epic_keys();
        assert!(epics.len() >= 40 && epics.contains("selection-transforms"), "{epics:?}");
        for item in &ITEMS {
            assert!(!item.bindings.is_empty(), "{}", item.srs);
            match item.owner {
                Owner::Registered(id) => {
                    let spec = Registry::get().spec(id.as_str()).expect("registered");
                    assert_eq!(spec.default_bindings, item.bindings, "{id}");
                    assert_eq!(spec.scope, item.scope, "{id}");
                }
                Owner::Pending { owner_epic } => {
                    assert!(
                        epics.contains(owner_epic),
                        "{}: {owner_epic} is no roadmap epic",
                        item.srs
                    );
                }
            }
        }
    }

    /// Every binding the keymap holds: the reserved bindings of the D.8 table
    /// and the default bindings of the registered commands that are not in it,
    /// each with its scope and its owner's name.
    fn keymap_bindings() -> Vec<(Binding, Scope, String)> {
        let mut all: Vec<(Binding, Scope, String)> = ITEMS
            .iter()
            .flat_map(|item| {
                item.bindings
                    .iter()
                    .map(|&b| (b, item.scope, format!("D.8 {}", item.srs)))
            })
            .collect();
        for spec in Registry::get().specs() {
            if item_for(spec.id).is_none() {
                all.extend(
                    spec.default_bindings
                        .iter()
                        .map(|&b| (b, spec.scope, spec.id.to_string())),
                );
            }
        }
        all
    }

    /// UI-008: no two owners share a binding in scopes that can be active at
    /// once, among the reserved D.8 bindings and the registered commands alike,
    /// so a later epic can register its D.8 command without a conflict.
    #[test]
    fn reserved_bindings_are_unique_within_overlapping_scopes() {
        let all = keymap_bindings();
        for (i, (a, scope_a, owner_a)) in all.iter().enumerate() {
            for (b, scope_b, owner_b) in &all[i + 1..] {
                assert!(
                    !(a == b && scope_a.overlaps(*scope_b)),
                    "{owner_a} and {owner_b} share {a:?}"
                );
            }
        }
    }

    /// UI-008: no reserved D.8 binding and no default binding is a combination
    /// an operating system reserves, on any of the three platforms, whichever
    /// host runs the test.
    #[test]
    fn no_binding_is_reserved_by_an_operating_system() {
        for (binding, _, owner) in keymap_bindings() {
            if let Some((os, reserved)) = os_reserved::reserved_by(binding) {
                panic!(
                    "{owner}: {} is reserved on {os:?} ({})",
                    binding.display(os),
                    reserved.purpose
                );
            }
        }
    }

    /// UI-008: the reserved D.8 bindings follow the keymap's conventions:
    /// letters of file, edit and transform commands with Primary; letters
    /// without Primary or Ctrl only for tools and view toggles, among the
    /// registered commands too.
    #[test]
    fn the_keymap_follows_its_conventions() {
        for item in &ITEMS {
            for b in item.bindings.iter().filter(|b| b.is_letter()) {
                let plain = !b.primary && !b.ctrl;
                match item.group {
                    Group::FileEdit | Group::Transform => {
                        assert!(b.primary, "{}: {b:?} without Primary", item.srs)
                    }
                    Group::Tool | Group::View => {
                        assert!(plain, "{}: {b:?} is not a single letter", item.srs)
                    }
                }
            }
        }
        for spec in Registry::get().specs() {
            for b in spec
                .default_bindings
                .iter()
                .filter(|b| b.is_letter() && !b.primary && !b.ctrl)
            {
                assert!(
                    matches!(spec.category, Category::Design | Category::View),
                    "{}: single letter {b:?}",
                    spec.id
                );
            }
        }
    }

    /// UI-008: docs/keymap.md shows the default keymap as the registry and the
    /// D.8 table have it. With `ARCHICRAFT_BLESS=1` the generated part is
    /// rewritten.
    #[test]
    fn keymap_doc_matches_the_registry() {
        const BEGIN: &str = "<!-- BEGIN GENERATED by keymap_doc_matches_the_registry -->\n";
        const END: &str = "<!-- END GENERATED -->\n";
        let english = std::rc::Rc::new(Localizer::english());
        let generated = crate::i18n::with(english, markdown);
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/keymap.md");
        let doc = repo_file("docs/keymap.md").replace("\r\n", "\n");
        let start = doc.find(BEGIN).expect("the begin marker") + BEGIN.len();
        let end = doc.find(END).expect("the end marker");
        if std::env::var_os("ARCHICRAFT_BLESS").is_some() {
            std::fs::write(&path, format!("{}{generated}{}", &doc[..start], &doc[end..])).unwrap();
            return;
        }
        assert!(
            doc[start..end] == generated,
            "docs/keymap.md differs from the registry; if intended, regenerate with \
             ARCHICRAFT_BLESS=1 cargo test -p archicraft-app keymap_doc"
        );
    }
}
