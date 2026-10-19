//! The ArchiCraft desktop application: a native window with plan and 3D
//! views, built with eframe/egui and wgpu (technology choices R8).

pub mod app;
pub mod document;
pub mod i18n;
pub mod logging;
pub mod persistence;
pub mod ui;
pub mod views;
pub mod workspace;

pub use app::ArchiCraftApp;

/// The application icon (`assets/icon.png`, also the macOS bundle icon) for
/// the window and the Dock. Without it eframe shows its own logo, even when
/// started from the app bundle.
pub fn icon() -> Option<egui::IconData> {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .inspect_err(|err| tracing::warn!(%err, "cannot decode the application icon"))
        .ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn icon_decodes() {
        let icon = super::icon().unwrap();
        assert_eq!((icon.width, icon.height), (1024, 1024));
    }
}
