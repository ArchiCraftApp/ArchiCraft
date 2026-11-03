//! The wall tool's geometry methods (WAL-001).

/// Geometry methods of the wall tool (WAL-001).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum WallMode {
    Single,
    #[default]
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

    /// The stable code in the id of the mode's command,
    /// `tool.wall.mode.<code>` (UI-017).
    pub fn code(self) -> &'static str {
        match self {
            WallMode::Single => "single",
            WallMode::Chain => "chain",
            WallMode::Rectangle => "rectangle",
            WallMode::RotatedRectangle => "rotated-rectangle",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WallMode::Single => "Single",
            WallMode::Chain => "Chain",
            WallMode::Rectangle => "Rectangle",
            WallMode::RotatedRectangle => "Rotated rectangle",
        }
    }
}
