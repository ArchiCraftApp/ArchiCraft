//! Values typed during point input (INP-003, INP-004).

/// Which typed value the keyboard currently edits (INP-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Field {
    #[default]
    Distance,
    Angle,
}

/// Values typed during point input, relative to the previous point in polar
/// coordinates (INP-004).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TypedInput {
    pub field: Field,
    pub distance: String,
    pub angle: String,
}

impl TypedInput {
    pub fn is_empty(&self) -> bool {
        self.distance.is_empty() && self.angle.is_empty()
    }

    pub fn active_text(&mut self) -> &mut String {
        match self.field {
            Field::Distance => &mut self.distance,
            Field::Angle => &mut self.angle,
        }
    }

    pub fn clear(&mut self) {
        *self = TypedInput::default();
    }
}
