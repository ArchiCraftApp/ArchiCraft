//! The view-control bar of the active view (UI-001): zoom, fit, view
//! rotation, drawing scale, the active display settings and the restriction
//! indicator (VIEW-043), in that order of slots.
//!
//! What a slot holds depends on the pane type: a [`ViewBarProvider`] per
//! type fills the slots it has controls for and leaves the others empty, so
//! a later feature fills a slot in its provider without touching the bar.

use archicraft_model::units::{NumberFormat, UnitSettings};
use archicraft_model::{DrawingScale, StoreyId};
use egui::{Key, RichText, TextEdit, Ui};

use super::command_widgets::{bar_button, menu_item};
use crate::commands::{CommandCtx, CommandId, CommandSpec, Registry, View3dCtx, view, view3d};
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};
use crate::{i18n, tr};

/// The slots of the bar, in the order they are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Zoom,
    Fit,
    /// The rotation of a plan view (VIEW-060).
    Rotation,
    /// The drawing scale of a 2D view (VIEW-013).
    Scale,
    /// The display settings of the view, such as the 3D display mode.
    Display,
    /// The indicator of an active restriction (VIEW-043).
    Restrictions,
}

impl Slot {
    pub const ALL: [Slot; 6] = [
        Slot::Zoom,
        Slot::Fit,
        Slot::Rotation,
        Slot::Scale,
        Slot::Display,
        Slot::Restrictions,
    ];
}

/// One control of a slot.
#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    /// A button of the command with a short label; an on/off command shows
    /// as selected while it is on.
    Command(CommandId, String),
    /// A value the bar shows, such as the zoom.
    Readout(String),
    /// A drop-down whose button shows `selected` and that lists `options`,
    /// each a command with its label.
    Choice {
        salt: &'static str,
        selected: String,
        hint: String,
        options: Vec<(CommandId, String)>,
    },
    /// A menu button that lists `commands` as menu items.
    Menu {
        label: String,
        hint: String,
        commands: Vec<CommandId>,
    },
    /// The drawing scale of the 2D view `view` (VIEW-013): a drop-down of
    /// the preset scales and a custom scale typed into a field, which
    /// belongs to `view`. `units` and `number_format` render the message of
    /// a rejected scale.
    Scale {
        view: Pane,
        selected: DrawingScale,
        units: UnitSettings,
        number_format: NumberFormat,
    },
}

/// What the user chose in the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarAction {
    /// The command of a clicked control.
    Command(CommandId),
    /// A drawing scale for the active 2D view (VIEW-013). It is view state,
    /// not a model change, so it is not undoable, like zoom.
    Scale(DrawingScale),
}

/// Fills the bar's slots for one pane type.
pub trait ViewBarProvider {
    /// The controls of `slot`, or `None` to leave it empty.
    fn slot(&self, slot: Slot) -> Option<Vec<Item>>;
}

/// The physical size of one egui point at a zoom factor of 1, in
/// millimetres: the reference pixel of 1/96 inch, which the operating
/// systems' display scaling approximates.
const MM_PER_POINT: f64 = 25.4 / 96.0;

/// The zoom of a plan in percent: 100 % shows the paper size at the drawing
/// scale 1:`drawing_scale` on screen.
///
/// `scale` is the camera's points per metre and `zoom_factor` egui's UI zoom
/// (its pixels per point over the display's native ones), so one model metre
/// takes `scale × MM_PER_POINT × zoom_factor` millimetres on screen against
/// `1000 / drawing_scale` millimetres on paper.
pub fn plan_zoom_percent(scale: f64, zoom_factor: f32, drawing_scale: f64) -> f64 {
    let on_screen = scale * MM_PER_POINT * f64::from(zoom_factor);
    let on_paper = 1000.0 / drawing_scale;
    on_screen / on_paper * 100.0
}

/// The zoom readout, such as "125 %" (NFR-L10N-004: the number in the
/// user's format).
fn zoom_readout(percent: f64, nf: &NumberFormat) -> Item {
    Item::Readout(tr!("view-bar-zoom", percent = nf.format_decimal(percent, 0)))
}

/// Zoom in, Zoom out and the readout, the start of every zoom slot.
fn zoom_items(percent: f64, nf: &NumberFormat) -> Vec<Item> {
    vec![
        Item::Command(view::ZOOM_OUT, tr!("view-bar-zoom-out")),
        zoom_readout(percent, nf),
        Item::Command(view::ZOOM_IN, tr!("view-bar-zoom-in")),
    ]
}

/// A drawing scale as the bar shows it, such as "1:100".
pub fn scale_text(scale: DrawingScale) -> String {
    tr!(
        "view-bar-drawing-scale-value",
        denominator = scale.denominator().to_string()
    )
}

/// The bar of a plan view: zoom with the zoom history, fit, the drawing
/// scale (VIEW-013) and the plan's display settings. Rotation (VIEW-060)
/// and the restriction indicator (VIEW-043) follow with their features.
#[derive(Clone, Debug)]
pub struct PlanBar {
    /// The storey of the plan.
    pub storey: StoreyId,
    pub zoom_percent: f64,
    pub number_format: NumberFormat,
    pub scale: DrawingScale,
    pub units: UnitSettings,
}

impl ViewBarProvider for PlanBar {
    fn slot(&self, slot: Slot) -> Option<Vec<Item>> {
        match slot {
            Slot::Zoom => {
                let mut items = zoom_items(self.zoom_percent, &self.number_format);
                items.push(Item::Command(view::PREVIOUS_ZOOM, tr!("view-bar-previous-zoom")));
                items.push(Item::Command(view::NEXT_ZOOM, tr!("view-bar-next-zoom")));
                Some(items)
            }
            Slot::Fit => Some(vec![Item::Command(view::FIT_WINDOW, tr!("view-bar-fit"))]),
            Slot::Scale => Some(vec![Item::Scale {
                view: Pane::Plan(self.storey),
                selected: self.scale,
                units: self.units.clone(),
                number_format: self.number_format,
            }]),
            Slot::Display => Some(vec![Item::Command(
                view::REFERENCE_LINES,
                tr!("view-bar-reference-lines"),
            )]),
            Slot::Rotation | Slot::Restrictions => None,
        }
    }
}

/// The bar of a 3D view: zoom, fit and orbit, the display mode, the view
/// presets and the projection (VIEW-014, VIEW-020, VIEW-050). The
/// restriction indicator (VIEW-043) follows with its feature.
#[derive(Clone, Debug)]
pub struct View3dBar {
    pub view: View3dCtx,
    pub zoom_percent: f64,
    pub number_format: NumberFormat,
}

impl ViewBarProvider for View3dBar {
    fn slot(&self, slot: Slot) -> Option<Vec<Item>> {
        match slot {
            Slot::Zoom => Some(zoom_items(self.zoom_percent, &self.number_format)),
            Slot::Fit => Some(vec![
                Item::Command(view::FIT_WINDOW, tr!("view-bar-fit")),
                Item::Command(view::ORBIT, tr!("view-bar-orbit")),
            ]),
            Slot::Display => Some(vec![
                Item::Choice {
                    salt: "display-mode",
                    selected: tr!(self.view.mode.label_key()),
                    hint: tr!("view-bar-display-mode-hint"),
                    options: DisplayMode::ALL
                        .into_iter()
                        .map(|mode| (view3d::display(mode), tr!(mode.label_key())))
                        .collect(),
                },
                Item::Menu {
                    label: tr!("view-bar-views"),
                    hint: tr!("view-bar-view-preset-hint"),
                    commands: ViewPreset::ALL.into_iter().map(view3d::preset).collect(),
                },
                Item::Command(view3d::TOGGLE_PROJECTION, tr!("view-bar-perspective")),
            ]),
            Slot::Rotation | Slot::Scale | Slot::Restrictions => None,
        }
    }
}

/// The provider for the active pane of `ws`, if a view is active.
/// `zoom_factor` is egui's UI zoom, for the plan's readout.
pub fn provider(ws: &Workspace, cx: &CommandCtx, zoom_factor: f32) -> Option<Box<dyn ViewBarProvider>> {
    let number_format = ws.prefs.number_format();
    match ws.active_pane? {
        Pane::Plan(storey) => {
            let state = ws.plan_views.get(&storey)?;
            Some(Box::new(PlanBar {
                storey,
                zoom_percent: plan_zoom_percent(state.camera.scale, zoom_factor, f64::from(state.scale.denominator())),
                number_format,
                scale: state.scale,
                units: ws.doc.project.settings.units.clone(),
            }))
        }
        Pane::View3d(id) => {
            let state = ws.views_3d.get(&id)?;
            Some(Box::new(View3dBar {
                view: cx.view3d?,
                zoom_percent: state.zoom_percent(),
                number_format,
            }))
        }
    }
}

fn spec(id: CommandId) -> &'static CommandSpec {
    Registry::get().command(id)
}

/// The bar with the slots of `provider`, a separator between two filled
/// slots. Returns what the user chose.
pub fn view_bar(ui: &mut Ui, cx: &CommandCtx, provider: &dyn ViewBarProvider) -> Option<BarAction> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        let mut first = true;
        for slot in Slot::ALL {
            let Some(items) = provider.slot(slot) else {
                continue;
            };
            if !first {
                ui.separator();
            }
            first = false;
            for item in items {
                if let Some(id) = item_ui(ui, cx, item) {
                    chosen = Some(id);
                }
            }
        }
    });
    chosen
}

/// Shows one control and returns what the user chose with it.
fn item_ui(ui: &mut Ui, cx: &CommandCtx, item: Item) -> Option<BarAction> {
    let mut chosen = None;
    match item {
        Item::Command(id, label) => {
            if bar_button(ui, cx, spec(id), &label) {
                chosen = Some(BarAction::Command(id));
            }
        }
        Item::Readout(text) => {
            ui.label(RichText::new(text).monospace());
        }
        Item::Choice {
            salt,
            selected,
            hint,
            options,
        } => {
            egui::ComboBox::from_id_salt(salt)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (id, label) in options {
                        if bar_button(ui, cx, spec(id), &label) {
                            chosen = Some(BarAction::Command(id));
                        }
                    }
                })
                .response
                .on_hover_text(hint);
        }
        Item::Menu { label, hint, commands } => {
            ui.menu_button(label, |ui| {
                for id in commands {
                    if menu_item(ui, cx, spec(id)) {
                        chosen = Some(BarAction::Command(id));
                    }
                }
            })
            .response
            .on_hover_text(hint);
        }
        Item::Scale {
            view,
            selected,
            units,
            number_format,
        } => chosen = scale_ui(ui, view, selected, &units, &number_format).map(BarAction::Scale),
    }
    chosen
}

/// The custom drawing scale being typed, kept in egui's memory between
/// frames per view.
#[derive(Clone, Debug, Default)]
struct CustomScale {
    text: String,
    /// The message of the rejected text, shown beside the field.
    error: Option<String>,
    /// Whether the field takes the keyboard focus this frame.
    focus: bool,
}

/// The drawing-scale drop-down of `view` (VIEW-013): the presets, then
/// "Custom…", which opens a field beside the drop-down. Enter in the field
/// applies the typed scale; text that is not a scale keeps the old one and
/// shows why; Escape in the field or choosing a preset closes it. The field
/// is the view's own: the bar of another view does not show it. Returns the
/// chosen scale.
fn scale_ui(
    ui: &mut Ui,
    view: Pane,
    selected: DrawingScale,
    units: &UnitSettings,
    nf: &NumberFormat,
) -> Option<DrawingScale> {
    let custom_id = ui.make_persistent_id(("drawing-scale-custom", view));
    let mut custom: Option<CustomScale> = ui.data(|d| d.get_temp(custom_id));
    let mut chosen = None;
    egui::ComboBox::from_label(tr!("view-bar-drawing-scale"))
        .selected_text(scale_text(selected))
        // Every preset and "Custom…" without scrolling.
        .height(f32::INFINITY)
        .show_ui(ui, |ui| {
            for preset in DrawingScale::PRESETS {
                if ui.selectable_label(preset == selected, scale_text(preset)).clicked() {
                    chosen = Some(preset);
                    custom = None;
                }
            }
            ui.separator();
            if ui
                .selectable_label(false, tr!("view-bar-drawing-scale-custom"))
                .clicked()
            {
                custom = Some(CustomScale {
                    focus: true,
                    ..CustomScale::default()
                });
            }
        })
        .response
        .on_hover_text(tr!("view-bar-drawing-scale-hint"));
    if let Some(c) = &mut custom {
        let field = ui
            .add(
                TextEdit::singleline(&mut c.text)
                    .id(custom_id.with("field"))
                    .hint_text(scale_text(selected))
                    .desired_width(64.0),
            )
            .on_hover_text(tr!("view-bar-drawing-scale-custom-hint"));
        if std::mem::take(&mut c.focus) {
            field.request_focus();
        }
        if let Some(error) = &c.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
        if field.lost_focus() {
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                match c.text.parse::<DrawingScale>() {
                    Ok(scale) => chosen = Some(scale),
                    Err(e) => {
                        c.error = Some(i18n::render(&e.message(), units, nf));
                        c.focus = true;
                    }
                }
            } else if ui.input(|i| i.key_pressed(Key::Escape)) {
                custom = None;
            }
        }
        if chosen.is_some() {
            custom = None;
        }
    }
    ui.data_mut(|d| match custom {
        Some(c) => {
            d.insert_temp(custom_id, c);
        }
        None => d.remove::<CustomScale>(custom_id),
    });
    chosen
}

#[cfg(test)]
mod tests {
    use egui::Key;
    use egui::accesskit::Role;
    use egui_kittest::Harness;
    use egui_kittest::kittest::{By, NodeT, Queryable};

    use super::*;

    /// A provider that fills every slot with one readout naming the slot,
    /// as the later epics fill Rotation, Scale and Restrictions.
    struct EverySlot;

    impl ViewBarProvider for EverySlot {
        fn slot(&self, slot: Slot) -> Option<Vec<Item>> {
            Some(vec![Item::Readout(format!("{slot:?} slot"))])
        }
    }

    /// UI-001: a provider that fills Rotation, Scale and Restrictions gets
    /// them shown in slot order, between Fit and Display and after Display.
    #[test]
    fn filled_slots_show_in_slot_order() {
        let cx = CommandCtx::default();
        let harness = Harness::new_ui(move |ui| {
            view_bar(ui, &cx, &EverySlot);
        });
        let xs: Vec<f32> = Slot::ALL
            .iter()
            .map(|slot| harness.get_by_label(&format!("{slot:?} slot")).rect().left())
            .collect();
        assert!(xs.windows(2).all(|w| w[0] < w[1]), "{xs:?}");
    }

    fn plan_bar(storey: StoreyId, scale: DrawingScale) -> PlanBar {
        PlanBar {
            storey,
            zoom_percent: 100.0,
            number_format: NumberFormat::default(),
            scale,
            units: UnitSettings::default(),
        }
    }

    /// UI-001, VIEW-013: the plan's bar fills the Scale slot with the view's
    /// drawing scale and leaves Rotation and Restrictions empty until their
    /// features fill them.
    #[test]
    fn the_plan_bar_leaves_the_later_slots_empty() {
        let bar = plan_bar(StoreyId::new(), DrawingScale::default());
        for slot in [Slot::Rotation, Slot::Restrictions] {
            assert_eq!(bar.slot(slot), None, "{slot:?}");
        }
        for slot in [Slot::Zoom, Slot::Fit, Slot::Scale, Slot::Display] {
            assert!(bar.slot(slot).is_some(), "{slot:?}");
        }
        assert!(matches!(
            bar.slot(Slot::Scale).as_deref(),
            Some([Item::Scale { selected, .. }]) if *selected == DrawingScale::default()
        ));
    }

    /// A harness of the plan's bar at `scale` that keeps the last action.
    fn scale_harness(scale: DrawingScale) -> Harness<'static, Option<BarAction>> {
        let cx = CommandCtx::default();
        let storey = StoreyId::new();
        Harness::new_ui_state(
            move |ui, action: &mut Option<BarAction>| {
                if let Some(a) = view_bar(ui, &cx, &plan_bar(storey, scale)) {
                    *action = Some(a);
                }
            },
            None,
        )
    }

    /// Opens the custom scale field from the drop-down that shows `current`
    /// and types `text` into it.
    fn open_custom_scale<State>(h: &mut Harness<'static, State>, current: &str, text: &str) {
        h.get(By::new().role(Role::ComboBox).value(current)).click();
        h.run();
        h.get_by_label("Custom…").click();
        h.run();
        h.get(By::new().role(Role::TextInput)).type_text(text);
        h.run();
    }

    /// Types `text` into the custom scale field, opened from the drop-down,
    /// and presses Enter.
    fn type_custom_scale(h: &mut Harness<'static, Option<BarAction>>, current: &str, text: &str) {
        open_custom_scale(h, current, text);
        h.key_press(Key::Enter);
        h.run();
    }

    /// VIEW-013: the drop-down offers the preset scales; choosing one
    /// returns it.
    #[test]
    fn the_scale_drop_down_offers_the_presets() {
        let mut h = scale_harness(DrawingScale::default());
        h.get(By::new().role(Role::ComboBox).value("1:100")).click();
        h.run();
        for preset in DrawingScale::PRESETS {
            h.get_by_label(&preset.to_string());
        }
        h.get_by_label("1:50").click();
        h.run();
        assert_eq!(*h.state(), Some(BarAction::Scale(DrawingScale::new(50).unwrap())));
    }

    /// VIEW-013: a typed custom scale is applied with Enter; text that is
    /// not a scale is not applied and the field says why.
    #[test]
    fn a_custom_scale_is_typed() {
        let mut h = scale_harness(DrawingScale::default());
        type_custom_scale(&mut h, "1:100", "1:75");
        assert_eq!(*h.state(), Some(BarAction::Scale(DrawingScale::new(75).unwrap())));
        assert!(h.query(By::new().role(Role::TextInput)).is_none(), "the field closes");

        let mut h = scale_harness(DrawingScale::default());
        type_custom_scale(&mut h, "1:100", "abc");
        assert_eq!(*h.state(), None);
        h.get_by_label_contains("“abc” is not a drawing scale");
        assert!(
            h.query(By::new().role(Role::TextInput)).is_some(),
            "the field stays open"
        );
        h.key_press(Key::Escape);
        h.run();
        assert!(h.query(By::new().role(Role::TextInput)).is_none(), "Escape closes it");
        assert_eq!(*h.state(), None);
    }

    /// VIEW-013: the custom scale field belongs to the plan whose bar opened
    /// it: the bar of another plan does not show it, so Enter there cannot
    /// set that plan's scale, and the first plan's bar still shows it with
    /// the typed text on return.
    #[test]
    fn the_custom_scale_field_belongs_to_its_plan() {
        let cx = CommandCtx::default();
        let (ground, upper) = (StoreyId::new(), StoreyId::new());
        let mut h = Harness::new_ui_state(
            move |ui, (storey, action): &mut (StoreyId, Option<BarAction>)| {
                if let Some(a) = view_bar(ui, &cx, &plan_bar(*storey, DrawingScale::default())) {
                    *action = Some(a);
                }
            },
            (ground, None),
        );
        open_custom_scale(&mut h, "1:100", "1:75");
        let field = By::new().role(Role::TextInput);

        h.state_mut().0 = upper;
        h.run();
        assert!(h.query(field.clone()).is_none(), "not in another plan's bar");
        h.key_press(Key::Enter);
        h.run();
        assert_eq!(h.state().1, None);

        h.state_mut().0 = ground;
        h.run();
        assert_eq!(
            h.get(field).accesskit_node().value().as_deref(),
            Some("1:75"),
            "still open in its own plan's bar"
        );
        assert_eq!(h.state().1, None);
    }

    /// UI-001: a plan at 100 % shows one model metre as 10 mm on screen at
    /// 1:100, that is 1000 / 100 / (25.4 / 96) ≈ 37.8 points per metre; at a
    /// UI zoom of 2 the same scale is twice as large, and at 1:50 half the
    /// zoom.
    #[test]
    fn the_plan_zoom_is_paper_size_at_the_drawing_scale() {
        let paper = 10.0 / MM_PER_POINT;
        assert!((plan_zoom_percent(paper, 1.0, 100.0) - 100.0).abs() < 1e-9);
        assert!((plan_zoom_percent(paper, 2.0, 100.0) - 200.0).abs() < 1e-9);
        assert!((plan_zoom_percent(paper, 1.0, 50.0) - 50.0).abs() < 1e-9);
        assert!((plan_zoom_percent(2.0 * paper, 1.0, 100.0) - 200.0).abs() < 1e-9);
    }
}
