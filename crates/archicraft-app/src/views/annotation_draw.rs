//! Annotation text at paper size in 2D views (ANN-007, VIEW-013, DSP-019).
//!
//! A text's size is given in paper millimetres and resolved every frame
//! against the view's drawing scale and the camera's points per metre, so
//! zooming changes how large the text is on screen and never its paper
//! size. Text goes through the egui overlay, one galley per line, because
//! a tessellated path would need a font tessellator; the same helper serves
//! every text-bearing annotation (space stamps, text blocks, dimension
//! values, labels).

use std::sync::Arc;

use archicraft_model::glam::DVec2;
use archicraft_model::{AnchorPos, DrawingScale, TextAlign};
use egui::emath::{Rot2, TSTransform};
use egui::epaint::TextShape;
use egui::text::LayoutJob;
use egui::{Color32, Context, FontId, Galley, Painter, Pos2, Rect, Shape, Vec2};

use super::camera::Camera2d;

/// Text smaller than this on screen, in points, is not laid out: it could
/// not be read, and skipping it keeps zoomed-out plans fast.
pub const MIN_TEXT_PT: f64 = 1.0;

/// The largest font size rasterised, in points. Larger text is laid out at
/// this size and scaled up, so that zooming far in never asks the font
/// atlas for glyphs wider than it is.
const MAX_RASTER_PT: f32 = 256.0;

/// A block of annotation text in a plan (ANN-007).
#[derive(Clone, Debug, PartialEq)]
pub struct TextBlock {
    /// The text; each `\n` starts a new line.
    pub content: String,
    /// The text size in paper millimetres at the view's drawing scale.
    pub size_mm: f64,
    /// The distance between consecutive lines as a multiple of the size.
    pub line_spacing: f64,
    pub align: TextAlign,
    /// The model point the block is placed at.
    pub anchor: DVec2,
    /// The point of the block that sits on `anchor`.
    pub anchor_pos: AnchorPos,
    /// The reading direction in radians counter-clockwise from the model's
    /// +X axis. Text never reads upside down (see [`upright`]).
    pub angle: f64,
}

/// One line of a laid-out text: its galley and the screen point of the
/// galley's top left corner.
#[derive(Clone, Debug)]
pub struct LaidOutLine {
    pub pos: Pos2,
    pub galley: Arc<Galley>,
}

/// A text laid out on screen for one frame.
#[derive(Clone, Debug)]
pub struct LaidOutText {
    /// The text size on screen in points. Every line's glyphs are this
    /// size; above [`MAX_RASTER_PT`] the galleys are laid out at that font
    /// size and scaled up to it.
    pub size: f32,
    pub lines: Vec<LaidOutLine>,
    /// The rotation on screen in radians clockwise, as epaint takes it.
    pub angle: f32,
    /// The corners of the block on screen in reading orientation: top left,
    /// top right, bottom right, bottom left.
    pub quad: [Pos2; 4],
    /// The block's width and height in points, before rotation.
    pub extent: Vec2,
}

impl LaidOutText {
    /// Whether the screen point lies inside the rotated block.
    pub fn hit(&self, pos: Pos2) -> bool {
        let local = Rot2::from_angle(self.angle).inverse() * (pos - self.quad[0]);
        (0.0..=self.extent.x).contains(&local.x) && (0.0..=self.extent.y).contains(&local.y)
    }
}

/// The reading direction a text is drawn at: never upside down, vertical
/// text reading from bottom to top (the model's rule, ANN-007).
pub use archicraft_model::text::upright;

/// Lays `text` out for a view at the drawing scale `scale` seen through
/// `camera` in `rect` (ANN-007, VIEW-013, DSP-019).
///
/// The font size is the paper size at the scale times the camera's points
/// per metre. Lines are `size × line_spacing` apart and aligned left,
/// centred or right within the block, and the block is placed so that its
/// anchor position lies on the anchor's screen point however many lines it
/// has, rotated about that point. Returns `None` for text smaller than
/// [`MIN_TEXT_PT`] on screen.
///
/// Alignment and anchor position are independent: alignment places each
/// line within the block's width (the widest line), the anchor position's
/// column places the block on the anchor. So the lines' left edges, centres
/// or right edges lie on the anchor's x only when the column matches the
/// alignment (`Left` with a `*Left` position, `Center` with `*Center` or
/// `Center`, `Right` with `*Right`); centred lines with a `BottomLeft`
/// position are centred within a block whose left edge is on the anchor.
pub fn layout_text(
    ctx: &Context,
    text: &TextBlock,
    scale: DrawingScale,
    camera: &Camera2d,
    rect: Rect,
) -> Option<LaidOutText> {
    let size = scale.paper_mm_to_model(text.size_mm) * camera.scale;
    if !size.is_finite() || size < MIN_TEXT_PT {
        return None;
    }
    let size = size as f32;
    let raster = size.min(MAX_RASTER_PT);
    let galleys: Vec<Arc<Galley>> = text
        .content
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let job = LayoutJob::simple_singleline(line.to_owned(), FontId::proportional(raster), Color32::PLACEHOLDER);
            let galley = ctx.fonts_mut(|fonts| fonts.layout_job(job));
            if raster < size {
                let mut shape = TextShape::new(Pos2::ZERO, galley, Color32::PLACEHOLDER);
                shape.transform(TSTransform::from_scaling(size / raster));
                shape.galley
            } else {
                galley
            }
        })
        .collect();

    let pitch = size * text.line_spacing as f32;
    let row_height = galleys[0].size().y;
    let width = galleys.iter().map(|g| g.rect.width()).fold(0.0, f32::max);
    let height = pitch * (galleys.len() - 1) as f32 + row_height;
    let extent = Vec2::new(width, height);
    let anchor_local = Vec2::new(
        width * text.anchor_pos.x_fraction() as f32,
        height * text.anchor_pos.y_fraction() as f32,
    );

    // Screen +Y points down, so a counter-clockwise model angle is a
    // clockwise screen angle of the opposite sign.
    let angle = -upright(text.angle) as f32;
    let rot = Rot2::from_angle(angle);
    let anchor = camera.to_screen(rect, text.anchor);
    let place = |local: Vec2| anchor + rot * (local - anchor_local);

    let lines = galleys
        .into_iter()
        .enumerate()
        .map(|(i, galley)| {
            let free = width - galley.rect.width();
            let x = match text.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => free / 2.0,
                TextAlign::Right => free,
            };
            LaidOutLine {
                pos: place(Vec2::new(x, pitch * i as f32)),
                galley,
            }
        })
        .collect();
    let quad = [
        place(Vec2::ZERO),
        place(Vec2::new(width, 0.0)),
        place(extent),
        place(Vec2::new(0.0, height)),
    ];
    Some(LaidOutText {
        size,
        lines,
        angle,
        quad,
        extent,
    })
}

/// The shapes that draw a laid-out text in `color`: one text shape per
/// line, rotated by the text's angle.
pub fn text_shapes(text: &LaidOutText, color: Color32) -> Vec<Shape> {
    text.lines
        .iter()
        .map(|line| {
            TextShape::new(line.pos, line.galley.clone(), color)
                .with_angle(text.angle)
                .into()
        })
        .collect()
}

/// Draws a laid-out text in `color`.
pub fn paint(painter: &Painter, text: &LaidOutText, color: Color32) {
    painter.extend(text_shapes(text, color));
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use archicraft_model::ANGLE_TOLERANCE;

    use super::*;

    const RECT: Rect = Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(800.0, 600.0));

    fn context() -> Context {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        ctx
    }

    fn camera(scale: f64) -> Camera2d {
        Camera2d {
            center: DVec2::ZERO,
            scale,
        }
    }

    fn block(content: &str) -> TextBlock {
        TextBlock {
            content: content.to_owned(),
            size_mm: 2.5,
            line_spacing: 1.0,
            align: TextAlign::Left,
            anchor: DVec2::new(1.0, 0.5),
            anchor_pos: AnchorPos::BottomLeft,
            angle: 0.0,
        }
    }

    fn scale(n: u32) -> DrawingScale {
        DrawingScale::new(n).unwrap()
    }

    fn lay_out(ctx: &Context, text: &TextBlock, n: u32, points_per_metre: f64) -> LaidOutText {
        layout_text(ctx, text, scale(n), &camera(points_per_metre), RECT).expect("text is laid out")
    }

    fn font_size(line: &LaidOutLine) -> f32 {
        line.galley.job.sections[0].format.font_id.size
    }

    fn anchor_screen(text: &TextBlock, points_per_metre: f64) -> Pos2 {
        camera(points_per_metre).to_screen(RECT, text.anchor)
    }

    fn close(a: f32, b: f32, tolerance: f32) -> bool {
        (a - b).abs() <= tolerance
    }

    /// VIEW-013, DSP-019: the size is paper millimetres at the drawing
    /// scale; 2.5 mm at 1:100 seen at 100 pt/m is 25 pt, at 1:50 12.5 pt, and
    /// zooming changes the size on screen but never the paper size.
    #[test]
    fn text_size_is_paper_millimetres_at_the_drawing_scale() {
        let ctx = context();
        let text = block("Living");
        let at_100 = lay_out(&ctx, &text, 100, 100.0);
        assert_eq!(at_100.size, 25.0);
        assert_eq!(font_size(&at_100.lines[0]), 25.0);
        let at_50 = lay_out(&ctx, &text, 50, 100.0);
        assert_eq!(at_50.size, 12.5);
        assert_eq!(font_size(&at_50.lines[0]), 12.5);
        let zoomed = lay_out(&ctx, &text, 100, 200.0);
        assert_eq!(zoomed.size, 50.0);
        assert_eq!(font_size(&zoomed.lines[0]), 50.0);
        assert_eq!(text.size_mm, 2.5);
        assert_eq!(scale(100).model_to_paper_mm(f64::from(zoomed.size) / 200.0), 2.5);
    }

    /// ANN-007: consecutive lines are size × line spacing apart.
    #[test]
    fn lines_are_size_times_line_spacing_apart() {
        let ctx = context();
        for spacing in [1.0, 1.5] {
            let text = TextBlock {
                line_spacing: spacing,
                ..block("one\ntwo\nthree")
            };
            let laid = lay_out(&ctx, &text, 100, 100.0);
            assert_eq!(laid.lines.len(), 3);
            for pair in laid.lines.windows(2) {
                let gap = pair[1].pos.y - pair[0].pos.y;
                assert!(close(gap, 25.0 * spacing as f32, 0.01), "spacing {spacing}: gap {gap}");
            }
        }
    }

    /// ANN-007: at angle 0, left, centred and right alignment put each line's
    /// left edge, centre and right edge on the anchor's x. Each alignment is
    /// paired with the anchor position of the matching column (bottom left,
    /// bottom centre, bottom right): only then do the lines meet the anchor.
    #[test]
    fn alignment_lines_up_each_line_on_the_anchor() {
        let ctx = context();
        let cases = [
            (TextAlign::Left, AnchorPos::BottomLeft, 0.0),
            (TextAlign::Center, AnchorPos::BottomCenter, 0.5),
            (TextAlign::Right, AnchorPos::BottomRight, 1.0),
        ];
        for (align, anchor_pos, fraction) in cases {
            let text = TextBlock {
                align,
                anchor_pos,
                ..block("a\nmuch longer line\nmid line")
            };
            let laid = lay_out(&ctx, &text, 100, 100.0);
            let x = anchor_screen(&text, 100.0).x;
            for line in &laid.lines {
                let at = line.pos.x + line.galley.rect.width() * fraction;
                assert!(close(at, x, 0.5), "{align:?}: line at {at}, anchor at {x}");
            }
        }
    }

    /// ANN-007: the anchor stays on the point of the block its position
    /// names when the text grows from one line to three; top positions keep
    /// the first line in place, bottom positions the last.
    #[test]
    fn the_anchor_stays_fixed_when_the_text_grows() {
        let ctx = context();
        for anchor_pos in AnchorPos::ALL {
            let one = TextBlock {
                anchor_pos,
                ..block("first")
            };
            let three = TextBlock {
                content: "first\nsecond line\nthird".to_owned(),
                ..one.clone()
            };
            let anchor = anchor_screen(&one, 100.0);
            for text in [&one, &three] {
                let laid = lay_out(&ctx, text, 100, 100.0);
                let [top_left, _, bottom_right, _] = laid.quad;
                let named = Pos2::new(
                    top_left.x + (bottom_right.x - top_left.x) * anchor_pos.x_fraction() as f32,
                    top_left.y + (bottom_right.y - top_left.y) * anchor_pos.y_fraction() as f32,
                );
                assert!(named.distance(anchor) <= 0.5, "{anchor_pos:?}: {named:?} vs {anchor:?}");
            }
            let first_one = lay_out(&ctx, &one, 100, 100.0).lines[0].pos;
            let grown = lay_out(&ctx, &three, 100, 100.0);
            // The lines' heights stay put; a wider block may shift them
            // sideways when the anchor is not on its left edge.
            let kept = match anchor_pos.y_fraction() {
                0.0 => Some(&grown.lines[0]),
                1.0 => Some(&grown.lines[2]),
                _ => None,
            };
            if let Some(line) = kept {
                assert!(close(line.pos.y, first_one.y, 0.5), "{anchor_pos:?}");
                // Left-aligned lines on a left-column anchor keep their x.
                if anchor_pos.x_fraction() == 0.0 {
                    assert!(close(line.pos.x, first_one.x, 0.5), "{anchor_pos:?}");
                }
            }
        }
    }

    /// ANN-007: text never reads upside down: 170 degrees is drawn at -10
    /// degrees, 90 degrees reads bottom to top and so does -90 degrees (the
    /// angles themselves: `upright_never_reads_upside_down` in the model).
    #[test]
    fn text_is_turned_upright() {
        let ctx = context();
        let degrees = |d: f64| d.to_radians();
        let tilted = lay_out(
            &ctx,
            &TextBlock {
                angle: degrees(170.0),
                ..block("Living")
            },
            100,
            100.0,
        );
        assert!(
            (f64::from(tilted.angle) - degrees(10.0)).abs() < 1e-6,
            "clockwise on screen"
        );
        let [top_left, top_right, ..] = tilted.quad;
        assert!(top_right.x > top_left.x, "reads left to right");

        let vertical = lay_out(
            &ctx,
            &TextBlock {
                angle: FRAC_PI_2,
                ..block("Living")
            },
            100,
            100.0,
        );
        let [start, end, ..] = vertical.quad;
        assert!(close(start.x, end.x, 0.01));
        assert!(end.y < start.y, "reads bottom to top");
    }

    /// ANN-007: angles within ANGLE_TOLERANCE of -90 degrees, on either
    /// side, read bottom to top like -90 degrees itself, as do angles that
    /// close to 90 degrees.
    #[test]
    fn near_vertical_text_reads_bottom_to_top() {
        let ctx = context();
        let near = 0.5 * ANGLE_TOLERANCE;
        for angle in [-FRAC_PI_2 + near, -FRAC_PI_2 - near, FRAC_PI_2 + near, FRAC_PI_2 - near] {
            let laid = lay_out(
                &ctx,
                &TextBlock {
                    angle,
                    ..block("Living")
                },
                100,
                100.0,
            );
            let [start, end, ..] = laid.quad;
            assert!(end.y < start.y, "{angle}: reads bottom to top");
        }
    }

    /// ANN-007: hit() is true inside the rotated block and false outside.
    #[test]
    fn hit_follows_the_rotated_quad() {
        let ctx = context();
        let text = TextBlock {
            angle: 30f64.to_radians(),
            ..block("Living room")
        };
        let laid = lay_out(&ctx, &text, 100, 100.0);
        let [a, b, c, d] = laid.quad;
        let centre = Pos2::new((a.x + c.x) / 2.0, (a.y + c.y) / 2.0);
        assert!(laid.hit(centre));
        for corner in [a, b, c, d] {
            let inside = corner + (centre - corner) * 0.05;
            let outside = corner - (centre - corner) * 0.05;
            assert!(laid.hit(inside), "inside near {corner:?}");
            assert!(!laid.hit(outside), "outside near {corner:?}");
        }
        // Inside the axis-aligned bounds of the quad but outside the quad.
        let bounds = Rect::from_points(&laid.quad);
        assert!(!laid.hit(bounds.right_top() + Vec2::new(-1.0, 1.0)));
        assert!(!laid.hit(bounds.left_bottom() + Vec2::new(1.0, -1.0)));
    }

    /// ANN-007: text smaller than 1 pt on screen is not laid out and draws
    /// nothing.
    #[test]
    fn text_below_one_point_is_skipped() {
        let ctx = context();
        let text = block("Living");
        // 2.5 mm at 1:100 is 0.25 m; at 3 pt/m that is 0.75 pt.
        assert!(layout_text(&ctx, &text, scale(100), &camera(3.0), RECT).is_none());
        assert!(layout_text(&ctx, &text, scale(100), &camera(4.0), RECT).is_some());
    }

    /// Text far larger than any glyph the font atlas holds keeps its size on
    /// screen.
    #[test]
    fn very_large_text_keeps_its_size() {
        let ctx = context();
        let text = block("Living\nroom");
        let laid = lay_out(&ctx, &text, 100, 10_000.0);
        assert_eq!(laid.size, 2500.0);
        let reference = lay_out(&ctx, &text, 100, 100.0);
        let ratio = laid.extent.x / reference.extent.x;
        assert!(close(ratio, 100.0, 1.0), "width ratio {ratio}");
        let gap = laid.lines[1].pos.y - laid.lines[0].pos.y;
        assert!(close(gap, 2500.0, 0.01));
    }

    /// paint() draws one text shape per line at the text's angle.
    #[test]
    fn paint_draws_each_line_at_its_angle() {
        let ctx = context();
        let text = TextBlock {
            angle: FRAC_PI_2,
            ..block("one\ntwo")
        };
        let laid = lay_out(&ctx, &text, 100, 100.0);
        let shapes = text_shapes(&laid, Color32::BLACK);
        assert_eq!(shapes.len(), 2);
        for (shape, line) in shapes.iter().zip(&laid.lines) {
            let Shape::Text(shape) = shape else {
                panic!("not text: {shape:?}");
            };
            assert_eq!(shape.angle, laid.angle);
            assert_eq!(shape.pos, line.pos);
            assert_eq!(shape.fallback_color, Color32::BLACK);
        }
    }
}
