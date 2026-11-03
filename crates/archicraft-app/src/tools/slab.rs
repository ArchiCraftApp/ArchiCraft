//! The slab tool's geometry methods (SLB-001).

/// Geometry methods of the slab tool (SLB-001).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SlabMode {
    #[default]
    Polygon,
    Rectangle,
    RotatedRectangle,
}

impl SlabMode {
    pub const ALL: [SlabMode; 3] = [SlabMode::Polygon, SlabMode::Rectangle, SlabMode::RotatedRectangle];

    /// The stable code in the id of the mode's command,
    /// `tool.slab.mode.<code>` (UI-017).
    pub fn code(self) -> &'static str {
        match self {
            SlabMode::Polygon => "polygon",
            SlabMode::Rectangle => "rectangle",
            SlabMode::RotatedRectangle => "rotated-rectangle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SlabMode::Polygon => "Polygon",
            SlabMode::Rectangle => "Rectangle",
            SlabMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}
