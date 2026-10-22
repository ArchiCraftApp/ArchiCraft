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
/// the repository root that the packaging scripts take for the bundle icons)
/// for the window and the Dock. Without it eframe shows its own logo, even when
/// started from the app bundle.
pub fn icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/ArchiCraft_Logo.png"))
        .inspect_err(|err| tracing::warn!(%err, "cannot decode the application icon"))
        .ok()
}

#[cfg(test)]
mod tests {
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
}
