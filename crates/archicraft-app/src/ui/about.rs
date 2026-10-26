//! The content of the About window (archicraft-pb3f2): the app icon, the
//! name with the version and the release stage, what ArchiCraft is, the
//! copyright, the website and the licence notice. Its text comes from the
//! message catalogue (NFR-L10N-001).

use egui::{Color32, ColorImage, Context, Image, RichText, TextureHandle, TextureOptions, Ui, Vec2};

use crate::tr;

/// The project's website.
const WEBSITE: &str = "https://archicraft.app";
/// The licence ArchiCraft is published under (LICENSE, README).
const LICENCE: &str = "https://creativecommons.org/licenses/by/4.0/";
/// The source code, the material the licence covers.
const SOURCE: &str = "https://github.com/ArchiCraftApp/ArchiCraft";

/// The width of the About window, in points; longer lines wrap.
pub const WIDTH: f32 = 440.0;
/// The side of the app icon, in points: large enough to read its pencils.
pub const ICON_SIZE: f32 = 128.0;
/// The space above the icon and around the licence notice, in points.
const PARAGRAPH: f32 = 16.0;

/// The app icon as a texture with as many pixels as it covers on screen,
/// made for one pixels-per-point value.
pub struct Icon {
    /// `None` when the bundled logo did not decode (logged by
    /// [`crate::icon`]); then the window shows no icon.
    texture: Option<TextureHandle>,
    pixels_per_point: f32,
}

impl std::fmt::Debug for Icon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Icon")
            .field("size", &self.texture.as_ref().map(TextureHandle::size))
            .field("pixels_per_point", &self.pixels_per_point)
            .finish()
    }
}

impl Icon {
    /// The icon texture for `pixels_per_point`, from `cached` when it was
    /// made for the same value, otherwise made again (another monitor, a
    /// changed zoom), so the icon stays sharp.
    pub fn texture<'a>(
        cached: &'a mut Option<Icon>,
        ctx: &Context,
        pixels_per_point: f32,
    ) -> Option<&'a TextureHandle> {
        if cached
            .as_ref()
            .is_none_or(|icon| icon.pixels_per_point != pixels_per_point)
        {
            // Rounded down a little, so 128 pt at 1.5 is 192 pixels, not 193.
            let side = (ICON_SIZE * pixels_per_point - 0.01).ceil().max(1.0) as usize;
            let texture = crate::icon().map(|logo| {
                let size = [logo.width as usize, logo.height as usize];
                let to = [side.min(size[0]), side.min(size[1])];
                ctx.load_texture("about-icon", scaled(&logo.rgba, size, to), TextureOptions::LINEAR)
            });
            *cached = Some(Icon {
                texture,
                pixels_per_point,
            });
        }
        cached.as_ref().and_then(|icon| icon.texture.as_ref())
    }
}

/// The unmultiplied RGBA image `rgba` of `size` pixels scaled to `to`
/// pixels, no larger. Each pixel is the mean of the source pixels it
/// covers, weighted by area, in premultiplied alpha, so the transparent
/// corners do not darken the edges. egui's renderer builds no mipmaps, so
/// a texture much larger than its place on screen would look jagged.
fn scaled(rgba: &[u8], size: [usize; 2], to: [usize; 2]) -> ColorImage {
    if to == size {
        return ColorImage::from_rgba_unmultiplied(size, rgba);
    }
    let [width, height] = size;
    let source: Vec<[f32; 4]> = rgba
        .chunks_exact(4)
        .map(|p| {
            Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
                .to_array()
                .map(f32::from)
        })
        .collect();
    let columns = spans(width, to[0]);
    let rows = spans(height, to[1]);
    let weighted = |pixels: &mut dyn Iterator<Item = ([f32; 4], f32)>| {
        pixels.fold([0.0; 4], |sum, (pixel, share)| {
            std::array::from_fn(|c| sum[c] + pixel[c] * share)
        })
    };
    // Rows first: every source row narrowed to the target width.
    let narrow: Vec<[f32; 4]> = (0..height)
        .flat_map(|y| {
            let source = &source;
            columns
                .iter()
                .map(move |span| weighted(&mut span.iter().map(|&(x, share)| (source[y * width + x], share))))
        })
        .collect();
    let pixels = rows
        .iter()
        .flat_map(|span| {
            let narrow = &narrow;
            (0..to[0]).map(move |x| {
                let [r, g, b, a] = weighted(&mut span.iter().map(|&(y, share)| (narrow[y * to[0] + x], share)))
                    .map(|channel| channel.round().clamp(0.0, 255.0) as u8);
                Color32::from_rgba_premultiplied(r, g, b, a)
            })
        })
        .collect();
    ColorImage::new(to, pixels)
}

/// For each of `to` cells laid over `from` cells, the cells it covers with
/// the share of its area each takes; the shares of a cell add up to 1.
fn spans(from: usize, to: usize) -> Vec<Vec<(usize, f32)>> {
    let scale = from as f32 / to as f32;
    (0..to)
        .map(|cell| {
            let start = cell as f32 * scale;
            let end = start + scale;
            (start.floor() as usize..(end.ceil() as usize).min(from))
                .map(|i| (i, (end.min(i as f32 + 1.0) - start.max(i as f32)) / scale))
                .filter(|&(_, share)| share > 0.0)
                .collect()
        })
        .collect()
}

/// The version as people read it: major.minor, with the patch only when it
/// is not 0 ("0.1" for 0.1.0, "0.1.2"); a version with a pre-release or
/// build part stays whole.
pub fn version_text(version: &str) -> String {
    let parts: Vec<&str> = version.split('.').collect();
    match parts.as_slice() {
        [major, minor, "0"] if [major, minor].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) => {
            format!("{major}.{minor}")
        }
        _ => version.to_owned(),
    }
}

/// The content of the About window, centred: the icon with space above it,
/// the name with the version and the release stage, what ArchiCraft is, the
/// copyright and the website; after a gap the licence notice with what a
/// CC BY 4.0 notice names (section 3(a)(1): the licence and its URI, the
/// disclaimer of warranties, a link to the material); last, dimmed, the
/// file format written and the privacy note (NFR-SEC-001, NFR-SEC-002).
pub fn contents(ui: &mut Ui, icon: Option<&TextureHandle>) {
    ui.vertical_centered(|ui| {
        ui.add_space(PARAGRAPH);
        if let Some(texture) = icon {
            ui.add(Image::new((texture.id(), Vec2::splat(ICON_SIZE))).alt_text(tr!("dlg-about-icon")));
            ui.add_space(PARAGRAPH / 2.0);
        }
        ui.heading(tr!(
            "dlg-about-heading",
            version = version_text(env!("CARGO_PKG_VERSION"))
        ));
        ui.label(tr!("dlg-about-summary"));
        ui.label(tr!("dlg-about-copyright"));
        ui.hyperlink(WEBSITE);
        ui.add_space(PARAGRAPH);
        ui.label(tr!("dlg-about-licence"));
        ui.hyperlink(LICENCE);
        ui.label(tr!("dlg-about-warranty"));
        ui.hyperlink_to(tr!("dlg-about-source", url = SOURCE), SOURCE);
        ui.add_space(PARAGRAPH);
        let format = tr!("dlg-about-format", version = archicraft_ifcx::WRITTEN_VERSION);
        ui.label(RichText::new(format).weak());
        ui.label(RichText::new(tr!("dlg-about-privacy")).weak());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_text_drops_a_zero_patch() {
        assert_eq!(version_text("0.1.0"), "0.1");
        assert_eq!(version_text("10.20.0"), "10.20");
        assert_eq!(version_text("0.1.2"), "0.1.2");
        assert_eq!(version_text("1.0.0-rc.1"), "1.0.0-rc.1");
        assert_eq!(version_text("1.0.0+build.7"), "1.0.0+build.7");
    }

    #[test]
    fn spans_share_each_cell_by_area() {
        assert_eq!(spans(4, 2), [vec![(0, 0.5), (1, 0.5)], vec![(2, 0.5), (3, 0.5)]]);
        let thirds = spans(3, 2);
        assert_eq!(thirds.len(), 2);
        let [first, second] = [&thirds[0], &thirds[1]];
        assert_eq!(first.iter().map(|s| s.0).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(second.iter().map(|s| s.0).collect::<Vec<_>>(), [1, 2]);
        assert!((first[0].1 - 2.0 / 3.0).abs() < 1e-6 && (first[1].1 - 1.0 / 3.0).abs() < 1e-6);
        for span in spans(1024, 192) {
            let total: f32 = span.iter().map(|s| s.1).sum();
            assert!((total - 1.0).abs() < 1e-5, "{span:?}");
        }
    }

    /// Averaging in premultiplied alpha: an opaque red pixel next to a
    /// transparent one gives half-transparent red, not a darker red.
    #[test]
    fn scaling_averages_premultiplied_pixels() {
        let image = scaled(&[255, 0, 0, 255, 0, 0, 0, 0], [2, 1], [1, 1]);
        assert_eq!(image.size, [1, 1]);
        assert_eq!(image.pixels[0], Color32::from_rgba_premultiplied(128, 0, 0, 128));
        let blue = [0, 64, 255, 255].repeat(16);
        let image = scaled(&blue, [4, 4], [2, 2]);
        assert!(
            image.pixels.iter().all(|&p| p == Color32::from_rgb(0, 64, 255)),
            "{image:?}"
        );
        // Black, white, black into two pixels: each takes 2/3 black, 1/3 white.
        let stripes = [[0, 0, 0, 255], [255, 255, 255, 255], [0, 0, 0, 255]].concat();
        let image = scaled(&stripes, [3, 1], [2, 1]);
        assert_eq!(image.pixels, [Color32::from_gray(85), Color32::from_gray(85)]);
    }

    /// The icon texture covers 128 pt with one texel per pixel at the
    /// current scale, made again when the scale changes, never larger than
    /// the logo (archicraft-pb3f2).
    #[test]
    fn the_icon_texture_follows_pixels_per_point() {
        let ctx = Context::default();
        let mut cached = None;
        let first = Icon::texture(&mut cached, &ctx, 1.0).unwrap().clone();
        assert_eq!(first.size(), [128, 128]);
        assert_eq!(Icon::texture(&mut cached, &ctx, 1.0).unwrap().id(), first.id());
        let sharper = Icon::texture(&mut cached, &ctx, 1.5).unwrap().clone();
        assert_eq!(sharper.size(), [192, 192]);
        assert_ne!(sharper.id(), first.id());
        assert_eq!(Icon::texture(&mut cached, &ctx, 2.0).unwrap().size(), [256, 256]);
        assert_eq!(Icon::texture(&mut cached, &ctx, 10.0).unwrap().size(), [1024, 1024]);
    }
}
