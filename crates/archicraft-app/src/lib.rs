//! The ArchiCraft desktop application: a native window with plan and 3D
//! views, built with eframe/egui and wgpu (technology choices R8).

pub mod app;
pub mod document;
pub mod logging;
pub mod persistence;
pub mod ui;
pub mod views;
pub mod workspace;

pub use app::ArchiCraftApp;
