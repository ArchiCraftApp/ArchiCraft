//! Values typed during point input (INP-002, INP-003, INP-004, INP-005).
//!
//! The typed values form a field set: relative or absolute polar coordinates
//! (distance, angle), Cartesian coordinates (X, Y, Z) or, while an element is
//! stretched, its total length or length change. Each field keeps
//! its text and, once a value was converted into it, the exact `f64`, so
//! switching field sets does not round typed values (AS-04).

use archicraft_model::glam::DVec2;
use archicraft_model::units::{
    NumberFormat, ParseError, UnitSettings, format_angle, format_length, parse_angle, parse_length,
};

use super::consumer::TrackerMode;
use crate::tr;

/// One typed field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Distance,
    Angle,
    /// ΔX in relative, X in absolute coordinates.
    X,
    /// ΔY in relative, Y in absolute coordinates.
    Y,
    /// ΔZ in relative, Z in absolute coordinates. Display-only in plan
    /// views: planar consumers ignore it until the 3D working plane exists.
    Z,
    /// The total length of a stretched element (INP-002).
    Total,
    /// The length change of a stretched element (INP-002).
    Change,
}

impl Field {
    /// The field's tracker label: X, Y and Z are differences in relative
    /// coordinates (INP-002, INP-004).
    pub fn label(self, reference: Reference) -> String {
        let relative = reference == Reference::Relative;
        match self {
            Field::Distance => tr!("tracker-distance"),
            Field::Angle => tr!("tracker-angle"),
            Field::X if relative => tr!("tracker-dx"),
            Field::Y if relative => tr!("tracker-dy"),
            Field::Z if relative => tr!("tracker-dz"),
            Field::X => tr!("tracker-x"),
            Field::Y => tr!("tracker-y"),
            Field::Z => tr!("tracker-z"),
            Field::Total => tr!("tracker-total"),
            Field::Change => tr!("tracker-change"),
        }
    }
}

/// The text typed into a field and, after a conversion from another field
/// set, its exact value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Value {
    pub text: String,
    exact: Option<f64>,
}

impl Value {
    /// A converted value: shown rounded, kept exact.
    fn converted(value: f64, text: String) -> Self {
        Self {
            text,
            exact: Some(value),
        }
    }

    pub fn push_str(&mut self, s: &str) {
        self.exact = None;
        self.text.push_str(s);
    }

    pub fn pop(&mut self) -> Option<char> {
        self.exact = None;
        self.text.pop()
    }
}

/// The fields of one coordinate kind (INP-002, INP-004).
#[derive(Clone, Debug, PartialEq)]
pub enum FieldSet {
    Polar {
        distance: Value,
        angle: Value,
    },
    Cartesian {
        dx: Value,
        dy: Value,
        dz: Value,
    },
    /// The alternatives of a stretch: either value gives the point.
    Stretch {
        total: Value,
        change: Value,
    },
}

impl FieldSet {
    pub fn polar() -> Self {
        FieldSet::Polar {
            distance: Value::default(),
            angle: Value::default(),
        }
    }

    pub fn cartesian() -> Self {
        FieldSet::Cartesian {
            dx: Value::default(),
            dy: Value::default(),
            dz: Value::default(),
        }
    }

    pub fn stretch() -> Self {
        FieldSet::Stretch {
            total: Value::default(),
            change: Value::default(),
        }
    }

    /// The fields in Tab order.
    pub fn fields(&self) -> &'static [Field] {
        match self {
            FieldSet::Polar { .. } => &[Field::Distance, Field::Angle],
            FieldSet::Cartesian { .. } => &[Field::X, Field::Y, Field::Z],
            FieldSet::Stretch { .. } => &[Field::Total, Field::Change],
        }
    }

    pub fn get(&self, field: Field) -> Option<&Value> {
        match (self, field) {
            (FieldSet::Polar { distance, .. }, Field::Distance) => Some(distance),
            (FieldSet::Polar { angle, .. }, Field::Angle) => Some(angle),
            (FieldSet::Cartesian { dx, .. }, Field::X) => Some(dx),
            (FieldSet::Cartesian { dy, .. }, Field::Y) => Some(dy),
            (FieldSet::Cartesian { dz, .. }, Field::Z) => Some(dz),
            (FieldSet::Stretch { total, .. }, Field::Total) => Some(total),
            (FieldSet::Stretch { change, .. }, Field::Change) => Some(change),
            _ => None,
        }
    }

    fn get_mut(&mut self, field: Field) -> Option<&mut Value> {
        match (self, field) {
            (FieldSet::Polar { distance, .. }, Field::Distance) => Some(distance),
            (FieldSet::Polar { angle, .. }, Field::Angle) => Some(angle),
            (FieldSet::Cartesian { dx, .. }, Field::X) => Some(dx),
            (FieldSet::Cartesian { dy, .. }, Field::Y) => Some(dy),
            (FieldSet::Cartesian { dz, .. }, Field::Z) => Some(dz),
            (FieldSet::Stretch { total, .. }, Field::Total) => Some(total),
            (FieldSet::Stretch { change, .. }, Field::Change) => Some(change),
            _ => None,
        }
    }
}

/// Where absolute coordinates are measured from (INP-004).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Project,
    /// The user origin (INP-014).
    User,
    /// A temporarily marked snap point.
    Marked,
}

/// What typed coordinates are measured from (INP-004).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// From the origin of the step: the previous point, the centre of a
    /// rotation.
    Relative,
    Absolute(Origin),
}

/// Values typed during point input (INP-003, INP-004).
#[derive(Clone, Debug, PartialEq)]
pub struct TypedInput {
    pub set: FieldSet,
    pub reference: Reference,
    /// The field the keyboard edits.
    pub field: Field,
    /// A key chose the set, the reference or the field, so the step's
    /// defaults no longer replace them while the step stays.
    chosen: bool,
    /// The fields were made for the first point of a step sequence.
    first: bool,
}

impl Default for TypedInput {
    fn default() -> Self {
        Self::for_step(true, false, TrackerMode::Relative)
    }
}

impl TypedInput {
    /// The fields a step starts with: a stretch takes the field of its
    /// tracker mode, the first point of a step sequence absolute X and Y
    /// from the project origin, later points a relative distance and angle;
    /// an angle step types the angle first.
    pub fn for_step(first: bool, angle_first: bool, tracker: TrackerMode) -> Self {
        let stretch = tracker.stretch().is_some();
        let (set, reference, field) = if stretch && matches!(tracker, TrackerMode::TotalLength { .. }) {
            (FieldSet::stretch(), Reference::Relative, Field::Total)
        } else if stretch {
            (FieldSet::stretch(), Reference::Relative, Field::Change)
        } else if first {
            (FieldSet::cartesian(), Reference::Absolute(Origin::Project), Field::X)
        } else if angle_first {
            (FieldSet::polar(), Reference::Relative, Field::Angle)
        } else {
            (FieldSet::polar(), Reference::Relative, Field::Distance)
        };
        Self {
            set,
            reference,
            field,
            chosen: false,
            first,
        }
    }

    /// Takes the step's defaults unless a value is typed or a key chose the
    /// fields for this kind of step. Removing the last point (Backspace)
    /// brings back the first point's absolute X and Y; a stretch step and
    /// any other step never share their fields.
    pub fn follow_step(&mut self, first: bool, angle_first: bool, tracker: TrackerMode) {
        let stretch = matches!(self.set, FieldSet::Stretch { .. });
        if self.is_empty() && (!self.chosen || self.first != first || stretch != tracker.stretch().is_some()) {
            *self = Self::for_step(first, angle_first, tracker);
        }
    }

    /// Whether no field holds text.
    pub fn is_empty(&self) -> bool {
        self.set.fields().iter().all(|f| self.text(*f).is_empty())
    }

    /// The text of `field`, empty when the set has no such field.
    pub fn text(&self, field: Field) -> &str {
        self.set.get(field).map_or("", |v| v.text.as_str())
    }

    /// Appends typed characters to the active field.
    pub fn push_str(&mut self, s: &str) {
        if let Some(value) = self.set.get_mut(self.field) {
            value.push_str(s);
        }
    }

    /// Removes the last typed character: from the active field, or from the
    /// last field before it that holds text, which becomes active.
    pub fn pop(&mut self) -> bool {
        let fields = self.set.fields();
        let start = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        for i in 0..fields.len() {
            let field = fields[(start + fields.len() - i) % fields.len()];
            if let Some(value) = self.set.get_mut(field)
                && value.pop().is_some()
            {
                self.field = field;
                return true;
            }
        }
        false
    }

    /// Moves to the next field (Tab).
    pub fn next_field(&mut self) {
        let fields = self.set.fields();
        let i = fields.iter().position(|f| *f == self.field).unwrap_or(0);
        self.field = fields[(i + 1) % fields.len()];
        self.chosen = true;
    }

    pub fn clear(&mut self) {
        *self = TypedInput::default();
    }

    /// The value of `field`: `None` while it is empty, its exact value
    /// after a conversion, otherwise its parsed text (INP-005).
    pub fn value(&self, field: Field, units: &UnitSettings, nf: &NumberFormat) -> Option<Result<f64, ParseError>> {
        let value = self.set.get(field).filter(|v| !v.text.is_empty())?;
        Some(match (value.exact, field) {
            (Some(exact), _) => Ok(exact),
            (None, Field::Angle) => parse_angle(&value.text, units.angle, nf),
            (None, _) => parse_length(&value.text, units.length, nf),
        })
    }

    /// Switches between polar and Cartesian fields (INP-004). Typed values
    /// are converted exactly, an untyped planar field taking its value from
    /// `cursor`, the cursor's offset from the base point, as a resolved
    /// point does; `base_angle` is the direction a polar angle is measured
    /// from. With nothing typed the new fields start empty; without a
    /// cursor to complete a field from, the set stays. ΔZ has no polar field
    /// and is dropped. A stretch has neither and keeps its fields.
    pub fn switch_set(&mut self, base_angle: f64, cursor: Option<DVec2>, units: &UnitSettings, nf: &NumberFormat) {
        let parsed = |f| self.value(f, units, nf).and_then(Result::ok);
        let typed = |a: Option<f64>, b: Option<f64>| a.is_some() || b.is_some();
        let length = |v: f64| Value::converted(v, format_length(v, units, nf, false));
        self.set = match self.set {
            FieldSet::Polar { .. } => match (parsed(Field::Distance), parsed(Field::Angle)) {
                (d, a) if typed(d, a) => {
                    let d = d.or(cursor.map(DVec2::length));
                    let a = a.map(|a| base_angle + a).or(cursor.map(DVec2::to_angle));
                    let Some(v) = d.zip(a).map(|(d, a)| DVec2::from_angle(a) * d) else {
                        return;
                    };
                    FieldSet::Cartesian {
                        dx: length(v.x),
                        dy: length(v.y),
                        dz: Value::default(),
                    }
                }
                _ => FieldSet::cartesian(),
            },
            FieldSet::Cartesian { .. } => match (parsed(Field::X), parsed(Field::Y)) {
                (x, y) if typed(x, y) => {
                    let x = x.or(cursor.map(|c| c.x));
                    let y = y.or(cursor.map(|c| c.y));
                    let Some(v) = x.zip(y).map(|(x, y)| DVec2::new(x, y)) else {
                        return;
                    };
                    let a = v.to_angle() - base_angle;
                    FieldSet::Polar {
                        distance: length(v.length()),
                        angle: Value::converted(a, format_angle(a, units, nf, false)),
                    }
                }
                _ => FieldSet::polar(),
            },
            FieldSet::Stretch { .. } => return,
        };
        self.field = self.set.fields()[0];
        self.chosen = true;
    }

    /// Switches between relative and absolute coordinates (INP-004). The
    /// typed text stays and is read in the new reference. A stretch length
    /// has no reference.
    pub fn switch_reference(&mut self) {
        if let FieldSet::Stretch { .. } = self.set {
            return;
        }
        self.reference = match self.reference {
            Reference::Relative => Reference::Absolute(Origin::Project),
            Reference::Absolute(_) => Reference::Relative,
        };
        self.chosen = true;
    }
}

/// Whether `c` starts a typed value when nothing is typed yet (INP-003): a
/// digit, a sign or the decimal separator of the number format
/// (NFR-L10N-004). Every other key keeps its single-key shortcut.
pub fn starts_value(c: char, nf: &NumberFormat) -> bool {
    c.is_ascii_digit() || matches!(c, '+' | '-' | '−') || c == nf.decimal_separator
}
