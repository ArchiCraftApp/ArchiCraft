//! THIRD-PARTY.md carries the notices of every component the ArchiCraft binaries contain
//! (NFR-LEG-001, archicraft-vczo.1). scripts/third-party.sh generates it with cargo-about from
//! Cargo.lock, together with third-party/not-shipped.txt, the packages the binaries do not
//! contain; these tests fail when Cargo.lock changed without the script being run again.

use std::collections::BTreeSet;

const LOCK: &str = include_str!("../../../Cargo.lock");
const NOTICES: &str = include_str!("../../../THIRD-PARTY.md");
const NOT_SHIPPED: &str = include_str!("../../../third-party/not-shipped.txt");

/// "name version" of every registry package in `lock`; the workspace's own crates have no
/// source.
fn lock_packages(lock: &str) -> BTreeSet<String> {
    lock.split("[[package]]")
        .skip(1)
        .filter(|block| block.contains("\nsource = \"registry+"))
        .map(|block| {
            let field = |key: &str| {
                block
                    .lines()
                    .find_map(|line| line.strip_prefix(key))
                    .map(|value| value.trim_matches('"').to_owned())
                    .unwrap_or_else(|| panic!("no {key} in {block}"))
            };
            format!("{} {}", field("name = "), field("version = "))
        })
        .collect()
}

/// "name version" of every crate of the crate list of `notices` ("- name version: licence").
fn listed(notices: &str) -> BTreeSet<String> {
    let crates = notices
        .split_once("\n## Crates\n")
        .and_then(|(_, rest)| rest.split_once("\n## "))
        .map(|(list, _)| list)
        .expect("a ## Crates section");
    crates
        .lines()
        .filter_map(|line| line.strip_prefix("- "))
        .map(|entry| entry.split_once(": ").map_or(entry, |(krate, _)| krate).to_owned())
        .collect()
}

fn not_shipped(list: &str) -> BTreeSet<String> {
    list.lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What keeps the notices from matching the lock file: packages new in `lock`, packages the
/// lists name that `lock` no longer has, and packages on both lists.
fn drift(lock: &str, notices: &str, not_shipped_list: &str) -> [Vec<String>; 3] {
    let lock = lock_packages(lock);
    let (listed, not_shipped) = (listed(notices), not_shipped(not_shipped_list));
    let known: BTreeSet<String> = listed.union(&not_shipped).cloned().collect();
    [
        lock.difference(&known).cloned().collect(),
        known.difference(&lock).cloned().collect(),
        listed.intersection(&not_shipped).cloned().collect(),
    ]
}

#[test]
fn third_party_notices_match_cargo_lock() {
    let [added, gone, both] = drift(LOCK, NOTICES, NOT_SHIPPED);
    assert!(
        added.is_empty() && gone.is_empty() && both.is_empty(),
        "THIRD-PARTY.md does not match Cargo.lock: run scripts/third-party.sh\n\
         new in Cargo.lock: {added:?}\nno longer in Cargo.lock: {gone:?}\n\
         both listed and not shipped: {both:?}"
    );
}

/// A package added to the lock file, or one whose version changed, is reported until the
/// notices are generated again.
#[test]
fn a_changed_lock_file_is_reported() {
    let package = |name: &str, version: &str| {
        format!("[[package]]\nname = \"{name}\"\nversion = \"{version}\"\nsource = \"registry+https://example\"\n")
    };
    let notices = "intro\n## Crates\n\n- a 1.0.0: MIT\n\n## Licence texts\n";
    let lock = package("a", "1.0.0") + &package("b", "2.0.0");
    let none = Vec::<String>::new;
    assert_eq!(
        drift(&lock, notices, "# not shipped\nb 2.0.0\n"),
        [none(), none(), none()]
    );
    let lock = package("a", "1.1.0") + &package("b", "2.0.0") + &package("c", "0.1.0");
    assert_eq!(
        drift(&lock, notices, "b 2.0.0\n"),
        [
            vec!["a 1.1.0".to_owned(), "c 0.1.0".to_owned()],
            vec!["a 1.0.0".to_owned()],
            none()
        ]
    );
}

/// The notices open with ArchiCraft's own licence and the open distribution model, cover the
/// crates of all three release platforms, egui's fonts, the Rust standard library and the
/// bundled IFCX schema files, carry the crates' own copyright lines (no bare licence template),
/// and leave out the test tools.
#[test]
fn third_party_notices_cover_what_ships() {
    assert!(
        NOTICES
            .starts_with("# Third-party licences\n\nArchiCraft is licensed under the Creative Commons Attribution 4.0"),
        "{}",
        &NOTICES[..200]
    );
    let names: BTreeSet<String> = listed(NOTICES)
        .iter()
        .filter_map(|krate| krate.split_once(' ').map(|(name, _)| name.to_owned()))
        .collect();
    // Windows, macOS and Linux only crates, and the GUI stack.
    for name in [
        "windows-sys",
        "objc2-metal",
        "wayland-client",
        "x11rb",
        "egui",
        "wgpu",
        "winit",
    ] {
        assert!(names.contains(name), "{name} is not listed");
    }
    for name in ["egui_kittest", "criterion"] {
        assert!(!names.contains(name), "{name} ships only with the tests");
    }
    for text in [
        "(SRS O-5)",
        "SIL OPEN FONT LICENSE Version 1.1",
        "UBUNTU FONT LICENCE Version 1.0",
        "BITSTREAM VERA LICENSE",
        "Copyright 2018 Source Foundry Authors",
        "Copyright (c) 2014 John Slegers",
        "## Rust standard library",
        "`ifc@v5a.ifcx`, `prop@v5a.ifcx`,",
        "`ifc-mat@v1.0.0.ifcx` and `usd@v1.ifcx`",
        "at commit 36010c0e1069043f964a7dff8367e3e910255ebb. The source states no licence for them",
        // Crates whose own licence file cargo-about cannot read, or that ship none.
        "Copyright (c) 2018, the respective contributors",
        "Copyright (c) 2006-2009 Graydon Hoare",
        "Copyright (c) Steven Sheldon",
        "Copyright (c) 2024 iShape-Rust",
        "blur.xml: Copyright 2015 Martin Gräßlin",
    ] {
        assert!(NOTICES.contains(text), "THIRD-PARTY.md lacks {text:?}");
    }
    for placeholder in ["<copyright holders>", "<owner>", "<copyright notice>"] {
        assert!(
            !NOTICES.contains(placeholder),
            "a bare licence template is left: {placeholder}"
        );
    }
}

/// Every notice of third-party/supplements.toml reaches THIRD-PARTY.md with its own source
/// line, also where its text equals another notice (dispatch's composed notice has the text
/// of the objc2 notice taken from the repository).
#[test]
fn supplied_notices_keep_their_source() {
    let sources: Vec<&str> = include_str!("../../../third-party/supplements.toml")
        .lines()
        .filter_map(|line| line.strip_prefix("source = \""))
        .filter_map(|line| line.strip_suffix('"'))
        .collect();
    assert_eq!(sources.len(), 5, "{sources:?}");
    for source in sources {
        assert_eq!(
            NOTICES.matches(&format!("\n{source}\n")).count(),
            1,
            "THIRD-PARTY.md shows {source:?} not once"
        );
    }
    let dispatch = NOTICES
        .split("\n### ")
        .find(|entry| entry.contains("Used by: dispatch 0.2.0\n"))
        .expect("an entry for dispatch 0.2.0");
    assert!(
        dispatch.contains("Composed: the package states the MIT licence"),
        "{dispatch}"
    );
}

/// Each package carries THIRD-PARTY.md next to LICENSE (NFR-LEG-001).
#[test]
fn the_packages_ship_the_notices() {
    for (script, source) in [
        ("package-linux.sh", include_str!("../../../scripts/package-linux.sh")),
        ("package-macos.sh", include_str!("../../../scripts/package-macos.sh")),
        (
            "package-windows.sh",
            include_str!("../../../scripts/package-windows.sh"),
        ),
    ] {
        let copies: Vec<&str> = source
            .lines()
            .map(str::trim)
            .filter(|line| line.starts_with("cp LICENSE ") || line.starts_with("cp THIRD-PARTY.md "))
            .collect();
        assert_eq!(copies.len(), 2, "{script}: {copies:?}");
        // The folder of the destination, the last argument, such as "$stage.
        let folder = |line: &str| {
            line.split_whitespace()
                .last()
                .and_then(|target| target.rsplit_once('/'))
                .map(|(dir, _)| dir.to_owned())
        };
        assert_eq!(folder(copies[0]), folder(copies[1]), "{script}: {copies:?}");
        // Windows opens .txt files in a text editor, so the zip names them that way.
        if script == "package-windows.sh" {
            assert!(
                copies.iter().any(|line| line.ends_with("/THIRD-PARTY.txt\"")),
                "{copies:?}"
            );
            assert!(copies.iter().any(|line| line.ends_with("/LICENSE.txt\"")), "{copies:?}");
        }
    }
}
