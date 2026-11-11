//! The one renderer of declared element parameters (UI-002, UI-003,
//! ELM-001): every kind lists its fields as [`FieldSpec`]s in its
//! descriptor, [`COMMON_FIELDS`] those of the element envelope, and this
//! module draws them for the properties strip and the settings dialog
//! alike, for the selected elements and for the tool defaults. It knows no
//! kind: a field's widget follows its [`FieldType`], its label and options
//! come from the spec, and a committed value goes through the spec's
//! setter into the working copies, which the panel then applies as one
//! command.

use archicraft_model::fields::{COMMON_FIELDS, FieldSpec, FieldType, FieldValue, OptionLabel, fields_of};
use archicraft_model::message::Message;
use archicraft_model::units::{NumberFormat, UnitSettings};
use archicraft_model::{Element, IfcClass, KindTag, Project};
use egui::{Id, Ui};

use super::properties::Layout;
use super::widgets::{self, Edited, common};
use crate::tr;

/// Field rendering context: the working copies and what the rows need.
#[derive(Debug)]
pub struct Fields<'a> {
    pub layout: Layout,
    /// The salt of the widget ids; each field adds its key, so that focus
    /// and typed text survive frames.
    pub id: Id,
    pub units: UnitSettings,
    pub nf: NumberFormat,
    pub project: &'a Project,
    /// The editable selected elements, or the prototype of the tool
    /// defaults.
    pub targets: &'a mut Vec<Element>,
    /// True while editing selected elements, false for the tool defaults.
    pub selection: bool,
    /// The hints of empty text fields, by field key, such as the suggested
    /// number of a space (SPC-001).
    pub hints: Vec<(&'static str, String)>,
    /// Set when a field committed a value to the targets.
    pub changed: bool,
    pub error: Option<String>,
}

impl Fields<'_> {
    /// The fields shown for the targets, in display order: the fields of
    /// the targets' kinds that any target has, then the common fields that
    /// every target has with the same options. A key two kinds share is one
    /// field, read and written through each target's own spec. The strip
    /// leaves out dialog-only fields, the tool defaults the fields only
    /// placed elements have.
    pub fn visible(&self) -> Vec<&'static FieldSpec> {
        let mut specs: Vec<&'static FieldSpec> = Vec::new();
        for e in self.targets.iter() {
            for spec in e.kind.descriptor().fields {
                if !specs.iter().any(|s| s.key == spec.key) && (spec.get)(e, self.project).is_some() {
                    specs.push(spec);
                }
            }
        }
        specs.extend(COMMON_FIELDS.iter().filter(|spec| self.all_have(spec)));
        specs.retain(|spec| (self.selection || !spec.placed_only) && (self.layout == Layout::Grid || spec.in_strip));
        specs
    }

    /// The value of the field `spec` of `element`, through the element's own
    /// spec of that key; `None` if the spec hides it.
    fn value(&self, spec: &'static FieldSpec, element: &Element) -> Option<FieldValue> {
        (spec_of(element, spec).get)(element, self.project)
    }

    /// Every target has the common field and, for a choice, the same
    /// options. The options of the common fields depend only on the
    /// project, the kind and the IFC class (`COMMON_FIELDS`), so they are
    /// built once per kind and class.
    fn all_have(&self, spec: &FieldSpec) -> bool {
        if !self.targets.iter().all(|e| (spec.get)(e, self.project).is_some()) {
            return false;
        }
        let Some(options) = spec.options else {
            return true;
        };
        let mut seen: Vec<(KindTag, IfcClass)> = Vec::new();
        let mut first: Option<Vec<FieldValue>> = None;
        for e in self.targets.iter() {
            if seen.contains(&(e.tag(), e.ifc_class)) {
                continue;
            }
            seen.push((e.tag(), e.ifc_class));
            let values: Vec<FieldValue> = options(self.project, e).into_iter().map(|(v, _)| v).collect();
            match &first {
                None => first = Some(values),
                Some(first) if *first != values => return false,
                Some(_) => {}
            }
        }
        true
    }

    /// The width of a widget that is `strip` wide in the strip.
    pub fn width(&self, strip: f32) -> f32 {
        match self.layout {
            Layout::Strip => strip,
            Layout::Grid => 180.0,
        }
    }

    /// A row of `label` and what `add` adds; the label names the widget
    /// whose id `add` returns, for screen readers.
    pub fn row(&mut self, ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui, &mut Self) -> Option<Id>) {
        let label = ui.label(label);
        if let Some(field) = add(ui, self) {
            ui.ctx().accesskit_node_builder(field, |node| {
                node.push_labelled_by(label.id.accesskit_id());
            });
        }
        match self.layout {
            Layout::Strip => ui.add_space(8.0),
            Layout::Grid => ui.end_row(),
        }
    }

    /// The rows of `specs`, each over the targets that have the field.
    pub fn show(&mut self, ui: &mut Ui, specs: &[&'static FieldSpec]) {
        for spec in specs {
            self.field(ui, spec);
        }
    }

    fn field(&mut self, ui: &mut Ui, spec: &'static FieldSpec) {
        let values: Vec<FieldValue> = self.targets.iter().filter_map(|e| self.value(spec, e)).collect();
        if values.is_empty() {
            return;
        }
        let label = match spec.defaults_label {
            Some(key) if !self.selection => tr!(key),
            _ => tr!(spec.label),
        };
        let id = self.id.with(spec.key);
        let hint = self
            .hints
            .iter()
            .find(|(key, _)| *key == spec.key)
            .map_or_else(String::new, |(_, hint)| hint.clone());
        self.row(ui, &label, |ui, f| {
            let (edited, labelled) = match spec.field_type {
                FieldType::Length | FieldType::Elevation => {
                    let value = common(values.iter().filter_map(as_length));
                    let edited = widgets::length(ui, id, value, &f.units, &f.nf, f.width(64.0));
                    (map(edited, FieldValue::Length), Some(id))
                }
                FieldType::Angle => {
                    let value = common(values.iter().filter_map(as_angle));
                    let edited = widgets::angle(ui, id, value, &f.units, &f.nf, f.width(64.0));
                    (map(edited, FieldValue::Angle), Some(id))
                }
                FieldType::Count => {
                    let value = common(values.iter().filter_map(as_count)).map(|n| f.nf.format_decimal(n as f64, 0));
                    let edited = widgets::hinted_text(ui, id, value.as_deref(), &hint, f.width(64.0));
                    (parse_count(edited, &f.nf), Some(id))
                }
                FieldType::Bool => {
                    let value = common(values.iter().map(|v| matches!(v, FieldValue::Bool(true))));
                    let mut checked = value.unwrap_or(false);
                    let response = ui.add(egui::Checkbox::without_text(&mut checked).indeterminate(value.is_none()));
                    let edited = if response.changed() {
                        Edited::Changed(FieldValue::Bool(checked))
                    } else {
                        Edited::Unchanged
                    };
                    (edited, Some(response.id))
                }
                FieldType::Choice => {
                    let options = f.options(spec);
                    let value = common(values.iter());
                    let (response, picked) = widgets::choice_response(ui, id, value, &options, f.width(110.0));
                    (picked.map_or(Edited::Unchanged, Edited::Changed), Some(response.id))
                }
                // COL-001: nine toggles in a 3 × 3 grid and the further
                // anchors; each names itself, so the row label stays apart.
                FieldType::Anchor => {
                    let options = f.options(spec);
                    let picked = widgets::anchor_grid(ui, id, common(values.iter()), &options);
                    (picked.map_or(Edited::Unchanged, Edited::Changed), None)
                }
                FieldType::Remark => {
                    let text = common(values.iter().filter_map(as_code))
                        .map(|key| f.option_label(OptionLabel::Message(Message::new(key))));
                    ui.label(egui::RichText::new(text.unwrap_or_default()).weak());
                    (Edited::Unchanged, None)
                }
                FieldType::Text => {
                    let value = common(values.iter().filter_map(as_text));
                    let edited = widgets::hinted_text(ui, id, value, &hint, f.width(90.0));
                    (map(edited, FieldValue::Text), Some(id))
                }
                FieldType::MultilineText => {
                    let value = common(values.iter().filter_map(as_text));
                    let (_, edited) = widgets::multiline_text(ui, id, value, f.width(140.0));
                    (map(edited, FieldValue::Text), Some(id))
                }
                FieldType::OptionalSize => {
                    let value = common(values.iter().filter_map(|v| match v {
                        FieldValue::OptionalSize(size) => Some(*size),
                        _ => None,
                    }));
                    let (_, edited) = widgets::optional_positive(ui, id, value, &hint, &f.nf, f.width(64.0));
                    (map(edited, FieldValue::OptionalSize), Some(id))
                }
            };
            f.commit(spec, edited);
            labelled
        });
    }

    /// The options of a choice field with their labels, from the first
    /// target that has the field, built once per frame.
    fn options(&self, spec: &'static FieldSpec) -> Vec<(FieldValue, String)> {
        let first = self.targets.iter().find(|e| self.value(spec, e).is_some());
        match (first.and_then(|e| spec_of(e, spec).options), first) {
            (Some(options), Some(first)) => options(self.project, first)
                .into_iter()
                .map(|(value, label)| (value, self.option_label(label)))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Writes a changed value into every target that has the field, or
    /// keeps the error of an invalid one.
    fn commit(&mut self, spec: &'static FieldSpec, edited: Edited<FieldValue>) {
        match edited {
            Edited::Changed(value) => {
                let project = self.project;
                for e in self.targets.iter_mut() {
                    let own = spec_of(e, spec);
                    if (own.get)(e, project).is_some() {
                        (own.set)(e, &value, project);
                    }
                }
                self.changed = true;
            }
            Edited::Invalid(e) => self.error = Some(e),
            Edited::Unchanged => {}
        }
    }

    fn option_label(&self, label: OptionLabel) -> String {
        match label {
            OptionLabel::Message(message) => crate::i18n::model::render(&message, &self.units, &self.nf),
            OptionLabel::Name(name) => name,
        }
    }
}

/// `element`'s own spec of the field `spec` stands for: its kind's or the
/// envelope's of the same key, or `spec` itself when it declares none.
fn spec_of(element: &Element, spec: &'static FieldSpec) -> &'static FieldSpec {
    fields_of(element).find(|s| s.key == spec.key).unwrap_or(spec)
}

fn map<T>(edited: Edited<T>, value: impl FnOnce(T) -> FieldValue) -> Edited<FieldValue> {
    match edited {
        Edited::Changed(v) => Edited::Changed(value(v)),
        Edited::Invalid(e) => Edited::Invalid(e),
        Edited::Unchanged => Edited::Unchanged,
    }
}

/// A typed whole number, read with the user's number format.
fn parse_count(edited: Edited<String>, nf: &NumberFormat) -> Edited<FieldValue> {
    match edited {
        Edited::Changed(text) => match nf.parse_decimal(&text) {
            Some(n) if n.fract() == 0.0 && n.abs() < i64::MAX as f64 => Edited::Changed(FieldValue::Count(n as i64)),
            _ => Edited::Invalid(tr!("dlg-properties-error-whole-number")),
        },
        Edited::Invalid(e) => Edited::Invalid(e),
        Edited::Unchanged => Edited::Unchanged,
    }
}

fn as_length(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Length(v) => Some(*v),
        _ => None,
    }
}

fn as_angle(value: &FieldValue) -> Option<f64> {
    match value {
        FieldValue::Angle(v) => Some(*v),
        _ => None,
    }
}

fn as_count(value: &FieldValue) -> Option<i64> {
    match value {
        FieldValue::Count(n) => Some(*n),
        _ => None,
    }
}

fn as_code(value: &FieldValue) -> Option<&'static str> {
    match value {
        FieldValue::Code(code) => Some(code),
        _ => None,
    }
}

fn as_text(value: &FieldValue) -> Option<&str> {
    match value {
        FieldValue::Text(v) => Some(v),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use archicraft_model::Project;
    use archicraft_model::fields::Section;
    use archicraft_model::glam::DVec2;
    use archicraft_model::template::default_metric_project;
    use egui::accesskit::{Role, Toggled};
    use egui_kittest::Harness;
    use egui_kittest::kittest::{By, NodeT, Queryable};

    use super::*;

    /// A flag field over the envelope's lock, for the Bool widget.
    const FLAG: FieldSpec = FieldSpec::new(
        "flag",
        "model-field-x",
        Section::Geometry,
        FieldType::Bool,
        |e, _| Some(FieldValue::Bool(e.locked)),
        |e, v, _| {
            if let FieldValue::Bool(b) = v {
                e.locked = *b;
            }
        },
    );

    /// A count field over the name's length, for the Count widget.
    const COUNT: FieldSpec = FieldSpec::new(
        "count",
        "model-field-y",
        Section::Geometry,
        FieldType::Count,
        |e, _| Some(FieldValue::Count(e.name.len() as i64)),
        |e, v, _| {
            if let FieldValue::Count(n) = v {
                e.name = "x".repeat(usize::try_from(*n).unwrap_or(0));
            }
        },
    );

    struct State {
        project: Project,
        targets: Vec<Element>,
        error: Option<String>,
    }

    /// The strip rows of [`FLAG`] and [`COUNT`] over two walls, the second
    /// locked.
    fn harness() -> Harness<'static, State> {
        let project = default_metric_project();
        let storey = project.storeys.ground();
        let first = project.new_wall(storey, DVec2::ZERO, DVec2::X, 0);
        let mut second = project.new_wall(storey, DVec2::Y, DVec2::ONE, 1);
        second.locked = true;
        let state = State {
            project,
            targets: vec![first, second],
            error: None,
        };
        Harness::new_ui_state(
            |ui, state: &mut State| {
                let mut f = Fields {
                    layout: Layout::Strip,
                    id: Id::new("fields-test"),
                    units: state.project.settings.units.clone(),
                    nf: NumberFormat::default(),
                    project: &state.project,
                    targets: &mut state.targets,
                    selection: true,
                    hints: Vec::new(),
                    changed: false,
                    error: None,
                };
                ui.horizontal(|ui| f.show(ui, &[&FLAG, &COUNT]));
                if f.error.is_some() {
                    state.error = f.error;
                }
            },
            state,
        )
    }

    /// UI-002: a flag the targets differ in is an indeterminate checkbox,
    /// and checking it sets the flag on every target.
    #[test]
    fn a_mixed_flag_is_checked_for_every_target() {
        let mut harness = harness();
        harness.run();
        let flag = || By::new().role(Role::CheckBox);
        assert_eq!(harness.get(flag()).accesskit_node().toggled(), Some(Toggled::Mixed));
        harness.get(flag()).click();
        harness.run();
        assert!(harness.state().targets.iter().all(|e| e.locked));
    }

    /// UI-002: a whole number typed into a count field goes to every target;
    /// a fraction is rejected with a message.
    #[test]
    fn a_count_takes_whole_numbers_only() {
        let mut harness = harness();
        harness.run();
        let field = || By::new().role(Role::TextInput).label("Y");
        assert_eq!(harness.get(field()).value().as_deref(), Some("5"));
        for (typed, names) in [("3", "xxx"), ("2.5", "xxx")] {
            harness.get(field()).click();
            harness.run();
            harness.get(field()).type_text(typed);
            harness.key_press(egui::Key::Enter);
            harness.run();
            assert!(harness.state().targets.iter().all(|e| e.name == names), "{typed}");
        }
        assert_eq!(
            harness.state().error.as_deref(),
            Some(tr!("dlg-properties-error-whole-number").as_str())
        );
    }

    /// UI-002: a count is read with the number format and must be whole.
    #[test]
    fn parse_count_reads_whole_numbers() {
        let nf = NumberFormat::default();
        let parsed = |text: &str| parse_count(Edited::Changed(text.to_owned()), &nf);
        assert_eq!(parsed("12"), Edited::Changed(FieldValue::Count(12)));
        assert_eq!(parsed("-2"), Edited::Changed(FieldValue::Count(-2)));
        for bad in ["1.5", "x", ""] {
            assert!(matches!(parsed(bad), Edited::Invalid(_)), "{bad}");
        }
    }
}
