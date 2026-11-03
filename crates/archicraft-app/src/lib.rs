//! The ArchiCraft desktop application: a native window with plan and 3D
//! views, built with eframe/egui and wgpu (technology choices R8).

pub mod app;
pub mod commands;
pub mod document;
pub mod i18n;
pub mod input;
pub mod logging;
#[cfg(feature = "perf")]
pub mod perf;
pub mod persistence;
pub mod tools;
pub mod ui;
pub mod views;
pub mod workspace;

pub use app::ArchiCraftApp;

/// The application icon (`assets/ArchiCraft_Logo.png`, a copy of the logo in
/// the repository root that the Linux and Windows packages take for their
/// icons), shown in the About window and, through [`viewport_icon`], on the
/// window and the Dock.
pub fn icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/ArchiCraft_Logo.png"))
        .inspect_err(|err| tracing::warn!(%err, "cannot decode the application icon"))
        .ok()
}

/// The icon main.rs gives eframe for the window and the Dock. Without one
/// eframe shows its own logo, so this is the logo, except on macOS in an app
/// bundle that brings its own icon (see `dock_icon`).
pub fn viewport_icon() -> Option<egui::IconData> {
    #[cfg(target_os = "macos")]
    if let Ok(exe) = std::env::current_exe() {
        return dock_icon(&exe);
    }
    icon()
}

/// The icon for the Dock when `exe` runs. eframe draws the icon it is given
/// into the Dock as it is (`NSApplication.setApplicationIconImage`), over the
/// bundle's own icon: the logo, a full-bleed square, then fills the whole tile
/// and looks larger than the other icons, which macOS masks and fits into it
/// (from `Assets.car` on macOS 26 and newer). So an app bundle
/// (`<name>.app/Contents/MacOS/<exe>`) whose `Info.plist` names an icon
/// (`CFBundleIconName` or `CFBundleIconFile`) gets the empty icon, which
/// eframe leaves unset; everything else (`cargo run`) gets the logo.
#[cfg(any(target_os = "macos", test))]
fn dock_icon(exe: &std::path::Path) -> Option<egui::IconData> {
    let contents = exe
        .parent()
        .filter(|dir| dir.ends_with("MacOS"))
        .and_then(std::path::Path::parent)
        .filter(|dir| dir.ends_with("Contents"))
        .filter(|dir| dir.parent().and_then(std::path::Path::extension) == Some("app".as_ref()));
    // A text search finds the keys in an XML plist as package-macos.sh writes
    // it and in a binary one, which keeps its keys as ASCII strings.
    let names_an_icon = contents
        .and_then(|dir| std::fs::read(dir.join("Info.plist")).ok())
        .is_some_and(|plist| {
            [&b"CFBundleIconName"[..], b"CFBundleIconFile"]
                .iter()
                .any(|key| plist.windows(key.len()).any(|bytes| bytes == *key))
        });
    if names_an_icon {
        Some(egui::IconData::default())
    } else {
        icon()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// The logo is as large as the largest macOS bundle icon
    /// (`icon_512x512@2x.png`, 1024 px), so packaging never upscales it.
    #[test]
    fn icon_decodes() {
        let icon = super::icon().unwrap();
        assert_eq!((icon.width, icon.height), (1024, 1024));
    }

    /// The window and Dock icon is the logo in the repository root, the one
    /// the packaging scripts take for the bundle icons.
    #[test]
    fn icon_is_the_repository_logo() {
        let logo = eframe::icon_data::from_png_bytes(include_bytes!("../../../ArchiCraft_Logo.png")).unwrap();
        let icon = super::icon().unwrap();
        assert_eq!((icon.width, icon.height), (logo.width, logo.height));
        assert!(icon.rgba == logo.rgba, "icon pixels differ from ArchiCraft_Logo.png");
    }

    /// An app bundle `ArchiCraft.app` in a fresh temporary folder named after
    /// `test`, with `plist` between the `<dict>` tags of its `Info.plist`.
    /// Returns its executable's path.
    fn bundle(test: &str, plist: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("archicraft-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let contents = root.join("ArchiCraft.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n{plist}\n</dict>\n</plist>\n"),
        )
        .unwrap();
        let exe = contents.join("MacOS/archicraft");
        std::fs::write(&exe, b"").unwrap();
        exe
    }

    /// The app bundle around `exe` (from `bundle`), removed.
    fn remove_bundle(exe: &Path) {
        let root = exe.ancestors().nth(4).unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    /// From a bundle that names its icon (as scripts/package-macos.sh writes
    /// it), eframe gets the empty icon and leaves the bundle's in the Dock.
    #[test]
    fn a_bundle_with_an_icon_keeps_it_in_the_dock() {
        for (test, plist) in [
            (
                "both-keys",
                "    <key>CFBundleIconFile</key><string>AppIcon</string>\n    <key>CFBundleIconName</key><string>AppIcon</string>",
            ),
            ("icon-name", "    <key>CFBundleIconName</key><string>AppIcon</string>"),
            ("icon-file", "    <key>CFBundleIconFile</key><string>AppIcon</string>"),
        ] {
            let exe = bundle(test, plist);
            assert_eq!(super::dock_icon(&exe), Some(egui::IconData::default()), "{test}");
            remove_bundle(&exe);
        }
    }

    /// A bundle that names no icon has none of its own: the Dock shows the logo.
    #[test]
    fn a_bundle_without_an_icon_gets_the_logo() {
        let exe = bundle(
            "no-icon",
            "    <key>CFBundleExecutable</key><string>archicraft</string>",
        );
        assert_eq!(super::dock_icon(&exe), super::icon());
        remove_bundle(&exe);
    }

    /// Outside a bundle (`cargo run`) there is no icon but the logo.
    #[test]
    fn an_unbundled_executable_gets_the_logo() {
        let exe = Path::new("target/debug/archicraft");
        assert_eq!(super::dock_icon(exe), super::icon());
        assert!(super::icon().is_some_and(|icon| !icon.is_empty()));
    }
}
