//! The surface a view offers to point input (INP-001, INP-015).
//!
//! Point input works in the 2D coordinates of a working plane. A view maps
//! screen pixels onto that plane; the plane's frame maps plane coordinates
//! into the model.

use archicraft_model::glam::{DVec2, DVec3};

/// A working plane in the model: an origin and two orthonormal axes. Plan
/// views use the storey elevation as origin and the model X and Y axes; the
/// 3D working plane of INP-015 fills origin and axes from a horizontal,
/// vertical, three-point or face plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SurfaceFrame {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
}

impl SurfaceFrame {
    /// The frame of a plan view at `elevation` (metres).
    pub fn plan(elevation: f64) -> Self {
        Self {
            origin: DVec3::new(0.0, 0.0, elevation),
            x: DVec3::X,
            y: DVec3::Y,
        }
    }

    /// The model point of a point on the plane.
    pub fn to_model(&self, p: DVec2) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y
    }

    /// The plane coordinates of a model point, projected onto the plane.
    pub fn to_plane(&self, p: DVec3) -> DVec2 {
        let d = p - self.origin;
        DVec2::new(d.dot(self.x), d.dot(self.y))
    }
}

/// A 2D frame on a working plane: an origin and the rotation of its X axis
/// from the plane's X axis, in radians. The user origin is one (INP-014);
/// the default is the plane's own origin and axes.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frame2d {
    pub origin: DVec2,
    pub rotation: f64,
}

impl Frame2d {
    /// Whether this is not the default frame: a user origin is placed
    /// (INP-014).
    pub fn is_placed(&self) -> bool {
        *self != Frame2d::default()
    }

    /// The coordinates of a plane point in this frame:
    /// R(−rotation)·(p − origin).
    pub fn to_local(&self, p: DVec2) -> DVec2 {
        DVec2::from_angle(-self.rotation).rotate(p - self.origin)
    }

    /// The plane point of coordinates in this frame, the inverse of
    /// [`Frame2d::to_local`].
    pub fn from_local(&self, p: DVec2) -> DVec2 {
        self.origin + DVec2::from_angle(self.rotation).rotate(p)
    }
}

/// What a view offers point input: the mapping between screen pixels (+Y
/// down) and its working plane. Screen positions are plain `DVec2` pixels, so
/// point input stays independent of the windowing toolkit.
pub trait ViewSurface {
    /// The plane point under a screen position, if the pointer ray meets the
    /// plane.
    fn to_plane(&self, screen: DVec2) -> Option<DVec2>;
    /// The screen position of a plane point.
    fn to_screen(&self, plane: DVec2) -> DVec2;
    /// Model length of one screen pixel near the plane, which turns the snap
    /// radius in pixels into model units.
    fn model_per_pixel(&self) -> f64;
    /// The working plane.
    fn frame(&self) -> SurfaceFrame;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A surface looking straight at its plane: `scale` pixels per metre,
    /// `centre` the plane point in the middle of a 800 × 600 pixel view.
    struct StubSurface {
        frame: SurfaceFrame,
        centre: DVec2,
        scale: f64,
    }

    impl ViewSurface for StubSurface {
        fn to_plane(&self, screen: DVec2) -> Option<DVec2> {
            let d = screen - DVec2::new(400.0, 300.0);
            Some(self.centre + DVec2::new(d.x, -d.y) / self.scale)
        }

        fn to_screen(&self, plane: DVec2) -> DVec2 {
            let d = (plane - self.centre) * self.scale;
            DVec2::new(400.0 + d.x, 300.0 - d.y)
        }

        fn model_per_pixel(&self) -> f64 {
            1.0 / self.scale
        }

        fn frame(&self) -> SurfaceFrame {
            self.frame
        }
    }

    /// INP-014: a frame maps plane points to its coordinates and back within
    /// 1e-12; rotated by 90° at (10, 10), its X axis points along the plane's
    /// Y axis.
    #[test]
    fn a_frame_round_trips_plane_points() {
        let frames = [
            Frame2d::default(),
            Frame2d {
                origin: DVec2::new(10.0, 10.0),
                rotation: 90f64.to_radians(),
            },
            Frame2d {
                origin: DVec2::new(-3.25, 1234.5),
                rotation: -2.1,
            },
        ];
        for frame in frames {
            for p in [
                DVec2::ZERO,
                DVec2::new(1.0, 0.0),
                DVec2::new(-7.5, 3.125),
                DVec2::new(1e3, -2e3),
            ] {
                assert!(frame.from_local(frame.to_local(p)).distance(p) < 1e-12, "{frame:?} {p}");
                assert!(frame.to_local(frame.from_local(p)).distance(p) < 1e-12, "{frame:?} {p}");
            }
        }
        let rotated = frames[1];
        assert!(rotated.from_local(DVec2::X).distance(DVec2::new(10.0, 11.0)) < 1e-12);
        assert!(rotated.to_local(DVec2::new(10.0, 11.0)).distance(DVec2::X) < 1e-12);
        assert_eq!(Frame2d::default().to_local(DVec2::new(3.0, 4.0)), DVec2::new(3.0, 4.0));
    }

    /// INP-001: a surface on a plane elevated to z = 3 and rotated by 30°
    /// round-trips screen → plane → model and back.
    #[test]
    fn an_elevated_rotated_surface_round_trips() {
        let angle = 30f64.to_radians();
        let surface = StubSurface {
            frame: SurfaceFrame {
                origin: DVec3::new(2.0, -1.0, 3.0),
                x: DVec3::new(angle.cos(), angle.sin(), 0.0),
                y: DVec3::new(-angle.sin(), angle.cos(), 0.0),
            },
            centre: DVec2::new(1.5, 2.5),
            scale: 40.0,
        };
        assert!((surface.model_per_pixel() - 0.025).abs() < 1e-12);
        for screen in [DVec2::new(0.0, 0.0), DVec2::new(123.25, 456.5), DVec2::new(799.0, 1.0)] {
            let plane = surface.to_plane(screen).unwrap();
            let model = surface.frame().to_model(plane);
            assert!((model.z - 3.0).abs() < 1e-9, "{model}");
            let back = surface.frame().to_plane(model);
            assert!(back.distance(plane) < 1e-9);
            assert!(surface.to_screen(back).distance(screen) < 1e-9);
        }
        // The plane's X axis runs at 30° in the model.
        let model = surface.frame().to_model(DVec2::X) - surface.frame().origin;
        assert!((model.y.atan2(model.x) - angle).abs() < 1e-12);
        assert_eq!(
            SurfaceFrame::plan(2.8).to_model(DVec2::new(1.0, 2.0)),
            DVec3::new(1.0, 2.0, 2.8)
        );
    }
}
