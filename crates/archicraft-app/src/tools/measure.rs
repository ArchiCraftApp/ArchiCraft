//! Measuring in plan views (MEA-001): a point-input consumer that takes the
//! points of a distance polyline or of an angle and reports their values in
//! working units. It never returns a command, so nothing in the model
//! changes and there is no undo step.

use archicraft_model::glam::DVec2;
use archicraft_model::tolerance::coincident;
use archicraft_model::units::{NumberFormat, UnitSettings, format_angle, format_length};
use archicraft_model::{Project, StoreyId};

use crate::input::{Expect, InputConsumer, KeyHint, Outcome, Preview, Resolved, StepSpec};
use crate::tr;

/// What the measure tool measures (MEA-001). The 3D views add the height
/// difference (archicraft-3g7.2) and the area mode follows (archicraft-gfv).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MeasureMode {
    /// A polyline: the total along it and its last segment.
    #[default]
    Distance,
    /// Three points: the vertex, then a point on each leg.
    Angle,
}

impl MeasureMode {
    pub const ALL: [MeasureMode; 2] = [MeasureMode::Distance, MeasureMode::Angle];

    /// The label of the mode's button.
    pub fn label(self) -> String {
        match self {
            MeasureMode::Distance => tr!("tool-measure-mode-distance"),
            MeasureMode::Angle => tr!("tool-measure-mode-angle"),
        }
    }
}

/// The values of a measurement in metres and radians, unrounded
/// (NFR-ACC-005): only the readout formats them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measurement {
    /// A polyline of two or more points.
    Distance {
        /// The length along the whole polyline.
        total: f64,
        /// The length of the last segment.
        segment: f64,
        /// The last segment's ΔX and ΔY.
        delta: DVec2,
        /// The last segment's direction from the X axis.
        direction: f64,
    },
    /// The angle between the two legs, 0 to π.
    Angle(f64),
}

/// The values of the polyline through `points`, if it has a segment.
pub fn distance(points: &[DVec2]) -> Option<Measurement> {
    let [.., a, b] = points else { return None };
    let delta = *b - *a;
    Some(Measurement::Distance {
        total: points.windows(2).map(|w| w[0].distance(w[1])).sum(),
        segment: delta.length(),
        delta,
        direction: delta.y.atan2(delta.x),
    })
}

/// The angle at `vertex` between the legs towards `a` and `b`, from
/// atan2(|a × b|, a · b), which stays accurate near 0 and 180 degrees.
pub fn angle(vertex: DVec2, a: DVec2, b: DVec2) -> f64 {
    let (u, v) = (a - vertex, b - vertex);
    u.perp_dot(v).abs().atan2(u.dot(v))
}

/// The measure tool (MEA-001): the points picked so far, which stay shown
/// once the measurement is finished until Esc, a tool switch or the first
/// click of the next measurement.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeasureTool {
    pub mode: MeasureMode,
    points: Vec<DVec2>,
    finished: bool,
    /// The storey the measurement was picked on.
    pub storey: Option<StoreyId>,
}

impl MeasureTool {
    /// The points picked so far.
    pub fn points(&self) -> &[DVec2] {
        &self.points
    }

    /// Whether a measurement is being picked: it has points and is not
    /// finished.
    pub fn in_progress(&self) -> bool {
        !self.points.is_empty() && !self.finished
    }

    /// Switches the mode, clearing the measurement.
    pub fn set_mode(&mut self, mode: MeasureMode) {
        self.mode = mode;
        self.clear();
    }

    /// Removes the measurement.
    pub fn clear(&mut self) {
        self.points.clear();
        self.finished = false;
        self.storey = None;
    }

    /// The values of the picked points, once they make a measurement.
    pub fn measurement(&self) -> Option<Measurement> {
        match (self.mode, &self.points[..]) {
            (MeasureMode::Distance, points) => distance(points),
            (MeasureMode::Angle, &[vertex, a, b]) => Some(Measurement::Angle(angle(vertex, a, b))),
            (MeasureMode::Angle, _) => None,
        }
    }

    /// The lines between the picked points: the polyline, or the legs of
    /// the angle from the vertex.
    pub fn lines(&self) -> Vec<Vec<DVec2>> {
        match (self.mode, &self.points[..]) {
            (_, [] | [_]) => Vec::new(),
            (MeasureMode::Distance, points) => vec![points.to_vec()],
            (MeasureMode::Angle, &[vertex, a]) => vec![vec![vertex, a]],
            (MeasureMode::Angle, &[vertex, a, b, ..]) => vec![vec![a, vertex, b]],
        }
    }

    /// The readout of the status line: each value with its label, formatted
    /// in the project units and the user's number format (MEA-001,
    /// NFR-ACC-005). The labels of ΔX, ΔY and the direction are the
    /// tracker's.
    pub fn readout(&self, units: &UnitSettings, nf: &NumberFormat) -> Vec<(String, String)> {
        let length = |v: f64| format_length(v, units, nf, true);
        let angle = |v: f64| format_angle(v, units, nf, true);
        match self.measurement() {
            None => Vec::new(),
            Some(Measurement::Distance {
                total,
                segment,
                delta,
                direction,
            }) => vec![
                (tr!("tool-measure-total"), length(total)),
                (tr!("tool-measure-segment"), length(segment)),
                (tr!("tracker-dx"), length(delta.x)),
                (tr!("tracker-dy"), length(delta.y)),
                (tr!("tracker-angle"), angle(direction)),
            ],
            Some(Measurement::Angle(a)) => vec![(tr!("tool-measure-angle"), angle(a))],
        }
    }
}

impl InputConsumer for MeasureTool {
    fn step(&self) -> Option<StepSpec> {
        let n = if self.finished { 0 } else { self.points.len() };
        let (prompt, repeat) = match (self.mode, n) {
            (MeasureMode::Distance, 0) => (tr!("prompt-measure-distance-first"), false),
            (MeasureMode::Distance, _) => (tr!("prompt-measure-distance-next"), true),
            (MeasureMode::Angle, 0) => (tr!("prompt-measure-angle-vertex"), false),
            (MeasureMode::Angle, 1) => (tr!("prompt-measure-angle-first-leg"), false),
            (MeasureMode::Angle, _) => (tr!("prompt-measure-angle-second-leg"), false),
        };
        let mut hints = Vec::new();
        if repeat {
            hints.push(KeyHint::new("Enter", tr!("prompt-measure-key-finish")));
        }
        if n > 0 {
            hints.push(KeyHint::new("Backspace", tr!("prompt-measure-key-remove-last")));
        }
        if !self.points.is_empty() {
            hints.push(KeyHint::new("Esc", tr!("prompt-measure-key-clear")));
        }
        Some(StepSpec {
            hints,
            repeat,
            ..StepSpec::new(prompt, Expect::Point)
        })
    }

    /// The last point of a distance, the vertex of an angle: typed values
    /// are relative to it.
    fn reference_point(&self) -> Option<DVec2> {
        if !self.in_progress() {
            return None;
        }
        match self.mode {
            MeasureMode::Distance => self.points.last().copied(),
            MeasureMode::Angle => self.points.first().copied(),
        }
    }

    /// The line from the reference point to the cursor while a measurement
    /// is in progress; the picked lines are drawn by the plan view.
    fn preview(&self, _project: &Project, resolved: &Resolved) -> Preview {
        Preview {
            outlines: self
                .reference_point()
                .map(|from| (vec![from, resolved.point], false))
                .into_iter()
                .collect(),
            ..Preview::default()
        }
    }

    /// Picks a point. The first click after a finished measurement, or one
    /// on another storey, starts a new one; a click on the previous point
    /// (the second click of a double-click) is the same point, also on the
    /// point that finished an angle, which keeps the angle.
    fn commit(&mut self, _project: &Project, storey: StoreyId, point: DVec2) -> Outcome {
        let on_last = self.points.last().is_some_and(|&p| coincident(p, point));
        if self.finished && on_last && self.storey == Some(storey) {
            return Outcome::Continue;
        }
        if self.finished || self.storey.is_some_and(|s| s != storey) {
            self.clear();
        }
        if self.points.last().is_some_and(|&p| coincident(p, point)) {
            return Outcome::Continue;
        }
        self.storey = Some(storey);
        self.points.push(point);
        if self.mode == MeasureMode::Angle && self.points.len() == 3 {
            self.finished = true;
        }
        Outcome::Continue
    }

    /// Esc clears the measurement, in progress or finished.
    fn cancel_step(&mut self) -> bool {
        let had = !self.points.is_empty();
        self.clear();
        had
    }

    /// Removes the last point of a measurement in progress.
    fn backspace(&mut self) -> bool {
        if !self.in_progress() {
            return false;
        }
        self.points.pop();
        if self.points.is_empty() {
            self.storey = None;
        }
        true
    }

    /// Enter or a double-click finishes a distance of two or more points,
    /// as it ends a wall chain.
    fn finish(&mut self, _project: &Project) -> Outcome {
        if self.mode == MeasureMode::Distance && self.in_progress() && self.points.len() >= 2 {
            self.finished = true;
        }
        Outcome::Continue
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use archicraft_model::template::default_metric_project;
    use archicraft_model::units::LengthUnit;

    use super::*;
    use crate::i18n::{self, Localizer};

    /// Picks `points` on the ground storey of `project`.
    fn pick(project: &Project, tool: &mut MeasureTool, points: &[(f64, f64)]) {
        let storey = project.storeys.ground();
        for &(x, y) in points {
            assert_eq!(
                tool.commit(project, storey, DVec2::new(x, y)),
                Outcome::Continue,
                "measuring executes nothing"
            );
        }
    }

    fn readout(tool: &MeasureTool, units: &UnitSettings, nf: &NumberFormat) -> Vec<(String, String)> {
        i18n::with(Rc::new(Localizer::english()), || tool.readout(units, nf))
    }

    fn strings(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|&(l, v)| (l.to_owned(), v.to_owned())).collect()
    }

    /// MEA-001, NFR-ACC-005: a distance is cumulative along the picked
    /// points; the last segment has its own length, ΔX, ΔY and direction.
    /// The values are exact before formatting.
    #[test]
    fn a_distance_is_cumulative_along_the_points() {
        let project = default_metric_project();
        let units = project.settings.units.clone();
        let nf = NumberFormat::default();
        let mut tool = MeasureTool::default();
        pick(&project, &mut tool, &[(0.0, 0.0), (3.0, 4.0)]);
        let Some(Measurement::Distance {
            total,
            segment,
            delta,
            direction,
        }) = tool.measurement()
        else {
            panic!("two points measure a distance")
        };
        assert!((total - 5.0).abs() < 1e-12 && (segment - 5.0).abs() < 1e-12);
        assert!(delta.distance(DVec2::new(3.0, 4.0)) < 1e-12);
        assert!((direction - 4f64.atan2(3.0)).abs() < 1e-12);
        assert_eq!(
            readout(&tool, &units, &nf),
            strings(&[
                ("Total", "5.000 m"),
                ("Segment", "5.000 m"),
                ("ΔX", "3.000 m"),
                ("ΔY", "4.000 m"),
                ("a", "53.13°"),
            ])
        );

        pick(&project, &mut tool, &[(3.0, 10.0)]);
        let Some(Measurement::Distance { total, segment, .. }) = tool.measurement() else {
            panic!()
        };
        assert!((total - 11.0).abs() < 1e-12 && (segment - 6.0).abs() < 1e-12);
        let shown = readout(&tool, &units, &nf);
        assert_eq!(shown[..2], strings(&[("Total", "11.000 m"), ("Segment", "6.000 m")]));
    }

    /// MEA-001: the angle at the vertex between its legs, exact before
    /// formatting, also near 0 and 180 degrees; a double-click on the third
    /// point keeps it.
    #[test]
    fn an_angle_takes_the_vertex_and_a_point_on_each_leg() {
        let project = default_metric_project();
        let units = project.settings.units.clone();
        let mut tool = MeasureTool::default();
        tool.set_mode(MeasureMode::Angle);
        pick(&project, &mut tool, &[(0.0, 0.0), (1.0, 0.0)]);
        assert_eq!(tool.measurement(), None);
        pick(&project, &mut tool, &[(0.0, 1.0)]);
        let Some(Measurement::Angle(a)) = tool.measurement() else {
            panic!("three points measure an angle")
        };
        assert!((a - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(
            readout(&tool, &units, &NumberFormat::default()),
            strings(&[("Angle", "90.00°")])
        );
        assert_eq!(tool.step().unwrap().prompt, tr!("prompt-measure-angle-vertex"));
        // A double-click on the third point: its second click is the same
        // point and keeps the angle.
        pick(&project, &mut tool, &[(0.0, 1.0)]);
        assert_eq!(tool.measurement(), Some(Measurement::Angle(a)), "the angle stays");
        assert_eq!(tool.points().len(), 3);

        let tiny = 1e-9;
        assert!((angle(DVec2::ZERO, DVec2::X, DVec2::new(1.0, tiny)) - tiny).abs() < 1e-20);
        let flat = angle(DVec2::ZERO, DVec2::X, DVec2::new(-1.0, tiny));
        assert!((flat - (std::f64::consts::PI - tiny)).abs() < 1e-15);
    }

    /// MEA-001, NFR-ACC-005: an imperial project reads feet and inches, a
    /// comma-decimal preference commas; only the display rounds.
    #[test]
    fn the_readout_follows_the_units_and_the_number_format() {
        let project = default_metric_project();
        let mut units = project.settings.units.clone();
        let comma = NumberFormat {
            decimal_separator: ',',
            grouping: None,
        };
        let mut tool = MeasureTool::default();
        pick(&project, &mut tool, &[(0.0, 0.0), (3.0, 4.0)]);
        let shown = readout(&tool, &units, &comma);
        assert_eq!(shown[0].1, "5,000 m");
        assert_eq!(shown[4].1, "53,13°");

        units.length = LengthUnit::FeetInches;
        let shown = readout(&tool, &units, &comma);
        assert_eq!(shown[0].1, format_length(5.0, &units, &comma, true));
        assert_eq!(shown[0].1, "16'-4 7/8\"");
    }

    /// MEA-001: Enter finishes a distance, which stays until the first click
    /// of the next measurement; Esc clears it, in progress or finished;
    /// Backspace removes the last point of one in progress.
    #[test]
    fn a_finished_measurement_stays_until_the_next_click_or_esc() {
        let project = default_metric_project();
        let mut tool = MeasureTool::default();
        pick(&project, &mut tool, &[(0.0, 0.0), (3.0, 4.0), (3.0, 4.0)]);
        assert_eq!(tool.points().len(), 2, "a double-click is one point");
        assert!(tool.step().unwrap().repeat);
        assert_eq!(tool.finish(&project), Outcome::Continue);
        assert!(!tool.in_progress());
        assert_eq!(tool.reference_point(), None);
        assert_eq!(tool.lines(), [vec![DVec2::ZERO, DVec2::new(3.0, 4.0)]]);
        assert!(!tool.backspace(), "a finished measurement has no step to take back");

        pick(&project, &mut tool, &[(9.0, 9.0)]);
        assert_eq!(tool.points(), [DVec2::new(9.0, 9.0)], "the next click starts anew");
        pick(&project, &mut tool, &[(9.0, 10.0)]);
        assert!(tool.backspace());
        assert_eq!(tool.points().len(), 1);
        assert!(tool.cancel_step());
        assert!(tool.points().is_empty() && tool.lines().is_empty());
        assert!(!tool.cancel_step(), "nothing left to clear");
    }
}
