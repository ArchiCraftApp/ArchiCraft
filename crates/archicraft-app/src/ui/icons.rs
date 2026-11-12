//! The icons of the toolbox and toolbar buttons (UI-001). They are
//! ArchiCraft's own drawings: open icon sets have no BIM tools, and nothing
//! may come from the reference product (docs/technology-choices.md, risk 5;
//! NFR-LEG-002).
//!
//! Each is an SVG in `assets/icons` on a 24 × 24 grid with a free border of
//! one unit: outlines 1.5 units wide with round ends and corners, the
//! modelling tools in oblique projection with the depth at 45° (the cube of
//! Fit, Orbit and Plan/3D too), so the icon of a new tool can follow them.
//! They are white only: a button multiplies them by its text colour
//! (`Button::image_tint_follows_text_color`), so an icon follows the theme,
//! hover, selection and the disabled state like the label beside it. The
//! project browser's rows tint their icons the same way (VIEW-003).

use archicraft_model::project::Projection;
use egui::{Context, ImageSource, include_image};

pub const SELECT: ImageSource<'static> = include_image!("../../assets/icons/select.svg");
pub const WALL: ImageSource<'static> = include_image!("../../assets/icons/wall.svg");
pub const SLAB: ImageSource<'static> = include_image!("../../assets/icons/slab.svg");
pub const SPACE: ImageSource<'static> = include_image!("../../assets/icons/space.svg");
pub const TEXT: ImageSource<'static> = include_image!("../../assets/icons/text.svg");
pub const COLUMN: ImageSource<'static> = include_image!("../../assets/icons/column.svg");
pub const FIT: ImageSource<'static> = include_image!("../../assets/icons/fit.svg");
pub const ORBIT: ImageSource<'static> = include_image!("../../assets/icons/orbit.svg");
pub const PLAN_3D: ImageSource<'static> = include_image!("../../assets/icons/plan-3d.svg");
pub const UNDO: ImageSource<'static> = include_image!("../../assets/icons/undo.svg");
pub const REDO: ImageSource<'static> = include_image!("../../assets/icons/redo.svg");
pub const SAVE: ImageSource<'static> = include_image!("../../assets/icons/save.svg");
pub const GROUP: ImageSource<'static> = include_image!("../../assets/icons/group.svg");
pub const UNGROUP: ImageSource<'static> = include_image!("../../assets/icons/ungroup.svg");
pub const MEASURE: ImageSource<'static> = include_image!("../../assets/icons/measure.svg");
/// The eyedropper of parameter pick-up and the syringe of inject (ELM-003,
/// the glossary's terms).
pub const PICK_UP: ImageSource<'static> = include_image!("../../assets/icons/pick-up.svg");
pub const INJECT: ImageSource<'static> = include_image!("../../assets/icons/inject.svg");
/// A sheet with a folded corner and a floor plan on it: a storey in the
/// project browser, each storey being a plan sheet (VIEW-003).
pub const PLAN_SHEET: ImageSource<'static> = include_image!("../../assets/icons/plan-sheet.svg");
/// A camera with its lens: a 3D view in perspective, and a saved camera
/// (VIEW-003, VIEW-020).
pub const CAMERA: ImageSource<'static> = include_image!("../../assets/icons/camera.svg");
/// A tall, steep box drawn in parallel projection: a 3D view in parallel
/// projection (VIEW-003, VIEW-020).
pub const PARALLEL_BOX: ImageSource<'static> = include_image!("../../assets/icons/parallel-box.svg");
/// A window outline with its right column marked off and lined: the
/// project browser beside the views, which the tab row's toggle shows and
/// hides (UI-004).
pub const SIDEBAR_RIGHT: ImageSource<'static> = include_image!("../../assets/icons/sidebar-right.svg");

/// Every icon.
pub const ALL: [ImageSource<'static>; 21] = [
    SELECT,
    WALL,
    SLAB,
    SPACE,
    TEXT,
    COLUMN,
    FIT,
    ORBIT,
    PLAN_3D,
    UNDO,
    REDO,
    SAVE,
    GROUP,
    UNGROUP,
    MEASURE,
    PICK_UP,
    INJECT,
    PLAN_SHEET,
    CAMERA,
    PARALLEL_BOX,
    SIDEBAR_RIGHT,
];

/// The icon of a 3D view, or of a saved camera, by the projection it shows
/// (VIEW-003, VIEW-020): the camera for perspective, the parallel box for
/// parallel projection.
pub fn view_icon(projection: Projection) -> ImageSource<'static> {
    match projection {
        Projection::Perspective => CAMERA,
        Projection::Parallel => PARALLEL_BOX,
    }
}

/// Installs the loader that rasterises the icons for the screen's pixels
/// (NFR-PORT-005). It does nothing once installed, so the application calls
/// it every frame and a test harness needs no setup.
pub fn install_loader(ctx: &Context) {
    egui_extras::install_image_loaders(ctx);
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use egui::ColorImage;
    use egui::load::{Bytes, SizeHint};

    use super::*;
    use crate::ui::command_widgets::ICON;

    /// The SVG source of `icon`.
    fn source(icon: &ImageSource<'static>) -> &'static [u8] {
        match icon {
            ImageSource::Bytes {
                bytes: Bytes::Static(bytes),
                ..
            } => bytes,
            _ => panic!("{icon:?} is not built in"),
        }
    }

    /// `icon` rasterised as egui's loader does it, `px` pixels square.
    fn raster(icon: &ImageSource<'static>, px: u32) -> ColorImage {
        let size = SizeHint::Size {
            width: px,
            height: px,
            maintain_aspect_ratio: true,
        };
        egui_extras::image::load_svg_bytes_with_size(source(icon), size, &Default::default())
            .unwrap_or_else(|e| panic!("{:?}: {e}", icon.uri()))
    }

    /// UI-001: every icon is a 24-unit square SVG of white paint only, which
    /// a button tints with its text colour, at the one icon side of toolbar
    /// and toolbox ([`ICON`], 28 pt), twice that many pixels (200 %
    /// scaling) and 48 pixels: each pixel is white with some
    /// alpha (premultiplied, so r = g = b = a), the lines cover 5 to 40 % of
    /// the square (drawn, and outlines rather than filled shapes), at twice
    /// the size some pixels are opaque, and the border of one unit stays
    /// empty. Nothing in an icon comes from outside it: no text (which needs
    /// fonts), no embedded or linked images.
    #[test]
    fn icons_are_white_drawings_inside_their_grid() {
        for icon in ALL {
            let name = icon.uri().unwrap_or_default();
            let text = std::str::from_utf8(source(&icon)).unwrap();
            assert!(text.contains(r#"viewBox="0 0 24 24""#), "{name}");
            for banned in ["<text", "<image", "href", "<style", "<script"] {
                assert!(!text.contains(banned), "{name} contains {banned}");
            }
            for px in [ICON as u32, 2 * ICON as u32, 48] {
                let image = raster(&icon, px);
                assert_eq!(image.size, [px as usize; 2], "{name}");
                assert!(
                    image.pixels.iter().all(|p| [p.r(), p.g(), p.b()] == [p.a(); 3]),
                    "{name} at {px} px has a pixel that is not white"
                );
                let ink =
                    image.pixels.iter().map(|p| f64::from(p.a())).sum::<f64>() / (255.0 * image.pixels.len() as f64);
                assert!((0.05..=0.4).contains(&ink), "{name} at {px} px covers {ink:.2}");
                // A 1.5-unit line is 2.5 pixels wide at 40 px, so it covers
                // whole pixels; at 28 px it is under two and may straddle three.
                assert!(
                    px < 40 || image.pixels.iter().any(|p| p.a() == 255),
                    "{name} at {px} px has no opaque pixel"
                );
            }
            // One unit of the grid is two pixels at 48 px.
            let image = raster(&icon, 48);
            for y in 0..48 {
                for x in 0..48 {
                    if x < 2 || y < 2 || x >= 46 || y >= 46 {
                        assert_eq!(image[(x, y)].a(), 0, "{name} paints the border at ({x}, {y})");
                    }
                }
            }
        }
    }

    /// PRJ-003, UI-011: Save shows a floppy disk, not the download arrow it
    /// was mistaken for: at 48 px (two pixels a grid unit) the outline has
    /// ink in three corners and one cut corner at the top right, the shutter
    /// hangs from the top edge, the label stands on the bottom edge, and the
    /// middle column between them, where the arrow's shaft ran, is empty.
    #[test]
    fn save_icon_is_a_floppy_disk() {
        let image = raster(&SAVE, 48);
        // The alpha of the pixel at grid point (u, v).
        let ink = |u: f64, v: f64| image[((u * 2.0) as usize, (v * 2.0) as usize)].a();
        for (u, v) in [(3.5, 3.5), (3.5, 20.5), (20.5, 20.5)] {
            assert!(ink(u, v) > 128, "the outline misses the corner ({u}, {v})");
        }
        assert_eq!(ink(20.5, 3.5), 0, "the top right corner is not cut");
        assert!(ink(18.5, 5.5) > 128, "the cut corner has no diagonal");
        for (u, v) in [(7.5, 6.0), (15.5, 6.0), (11.5, 8.5)] {
            assert!(ink(u, v) > 128, "the shutter misses ({u}, {v})");
        }
        for (u, v) in [(7.0, 17.0), (17.0, 17.0), (12.0, 14.0)] {
            assert!(ink(u, v) > 128, "the label misses ({u}, {v})");
        }
        for v in [10.0, 11.0, 12.0] {
            assert_eq!(ink(12.0, v), 0, "a line runs down the middle at {v}, like an arrow");
        }
    }

    /// No two icons are the same drawing or share a texture name.
    #[test]
    fn icons_differ() {
        let sources: HashSet<&[u8]> = ALL.iter().map(source).collect();
        let uris: HashSet<&str> = ALL.iter().filter_map(ImageSource::uri).collect();
        assert_eq!((sources.len(), uris.len()), (ALL.len(), ALL.len()));
    }
}
