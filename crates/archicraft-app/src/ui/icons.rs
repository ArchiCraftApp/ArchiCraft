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
//! hover, selection and the disabled state like the label beside it.

use egui::{Context, ImageSource, include_image};

pub const SELECT: ImageSource<'static> = include_image!("../../assets/icons/select.svg");
pub const WALL: ImageSource<'static> = include_image!("../../assets/icons/wall.svg");
pub const SLAB: ImageSource<'static> = include_image!("../../assets/icons/slab.svg");
pub const FIT: ImageSource<'static> = include_image!("../../assets/icons/fit.svg");
pub const ORBIT: ImageSource<'static> = include_image!("../../assets/icons/orbit.svg");
pub const PLAN_3D: ImageSource<'static> = include_image!("../../assets/icons/plan-3d.svg");
pub const UNDO: ImageSource<'static> = include_image!("../../assets/icons/undo.svg");
pub const REDO: ImageSource<'static> = include_image!("../../assets/icons/redo.svg");

/// Every icon.
pub const ALL: [ImageSource<'static>; 8] = [SELECT, WALL, SLAB, FIT, ORBIT, PLAN_3D, UNDO, REDO];

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
    /// a button tints with its text colour, at the toolbar's 16, the
    /// toolbox's 20 and twice that many pixels: each pixel is white with some
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
            for px in [16, 20, 40, 48] {
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
                // whole pixels; at 16 px it is one pixel and may straddle two.
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

    /// No two icons are the same drawing or share a texture name.
    #[test]
    fn icons_differ() {
        let sources: HashSet<&[u8]> = ALL.iter().map(source).collect();
        let uris: HashSet<&str> = ALL.iter().filter_map(ImageSource::uri).collect();
        assert_eq!((sources.len(), uris.len()), (ALL.len(), ALL.len()));
    }
}
