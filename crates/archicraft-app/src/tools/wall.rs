//! The wall tool's geometry methods (WAL-001).

/// Geometry methods of the wall tool (WAL-001).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WallMode {
    Single,
    Chain,
    Rectangle,
    RotatedRectangle,
}

impl WallMode {
    pub const ALL: [WallMode; 4] = [
        WallMode::Single,
        WallMode::Chain,
        WallMode::Rectangle,
        WallMode::RotatedRectangle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WallMode::Single => "Single",
            WallMode::Chain => "Chain",
            WallMode::Rectangle => "Rectangle",
            WallMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}
