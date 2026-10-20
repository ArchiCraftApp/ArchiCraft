//! The slab tool's geometry methods (SLB-001).

/// Geometry methods of the slab tool (SLB-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlabMode {
    Polygon,
    Rectangle,
    RotatedRectangle,
}

impl SlabMode {
    pub const ALL: [SlabMode; 3] = [SlabMode::Polygon, SlabMode::Rectangle, SlabMode::RotatedRectangle];

    pub fn label(self) -> &'static str {
        match self {
            SlabMode::Polygon => "Polygon",
            SlabMode::Rectangle => "Rectangle",
            SlabMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}
