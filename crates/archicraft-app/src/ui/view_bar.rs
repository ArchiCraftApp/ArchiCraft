//! The view-control bar of the active view (UI-001): zoom, fit, view
//! rotation, drawing scale, the active display settings and the restriction
//! indicator (VIEW-043), in that order of slots.
//!
//! What a slot holds depends on the pane type: a [`ViewBarProvider`] per
//! type fills the slots it has controls for and leaves the others empty, so
//! a later feature fills a slot in its provider without touching the bar.

use archicraft_model::units::NumberFormat;
use egui::{RichText, Ui};

use super::command_widgets::{bar_button, menu_item};
use crate::commands::{CommandCtx, CommandId, CommandSpec, Registry, View3dCtx, view, view3d};
use crate::tr;
use crate::views::camera::ViewPreset;
use crate::views::render3d::DisplayMode;
use crate::workspace::{Pane, Workspace};

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
}

/// Fills the bar's slots for one pane type.
pub trait ViewBarProvider {
    /// The controls of `slot`, or `None` to leave it empty.
    fn slot(&self, slot: Slot) -> Option<Vec<Item>>;
}

/// The drawing scale of plans until views have their own (VIEW-013, default
/// 1:100).
const DRAWING_SCALE: f64 = 100.0;

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

/// The bar of a plan view: zoom with the zoom history, fit and the plan's
/// display settings. Rotation (VIEW-060), scale (VIEW-013) and the
/// restriction indicator (VIEW-043) follow with their features.
#[derive(Clone, Debug)]
pub struct PlanBar {
    pub zoom_percent: f64,
    pub number_format: NumberFormat,
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
            Slot::Display => Some(vec![Item::Command(
                view::REFERENCE_LINES,
                tr!("view-bar-reference-lines"),
            )]),
            Slot::Rotation | Slot::Scale | Slot::Restrictions => None,
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
                zoom_percent: plan_zoom_percent(state.camera.scale, zoom_factor, DRAWING_SCALE),
                number_format,
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
/// slots. Returns the command of the clicked control.
pub fn view_bar(ui: &mut Ui, cx: &CommandCtx, provider: &dyn ViewBarProvider) -> Option<CommandId> {
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

/// Shows one control and returns its command when clicked.
fn item_ui(ui: &mut Ui, cx: &CommandCtx, item: Item) -> Option<CommandId> {
    let mut chosen = None;
    match item {
        Item::Command(id, label) => {
            if bar_button(ui, cx, spec(id), &label) {
                chosen = Some(id);
            }
        }
        Item::Readout(text) => {
            ui.label(RichText::new(text).small().monospace());
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
                            chosen = Some(id);
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
                        chosen = Some(id);
                    }
                }
            })
            .response
            .on_hover_text(hint);
        }
    }
    chosen
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable;

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

    /// UI-001: the plan's bar leaves Rotation, Scale and Restrictions empty
    /// until their features fill them; there is no hard-coded scale.
    #[test]
    fn the_plan_bar_leaves_the_later_slots_empty() {
        let bar = PlanBar {
            zoom_percent: 100.0,
            number_format: NumberFormat::default(),
        };
        for slot in [Slot::Rotation, Slot::Scale, Slot::Restrictions] {
            assert_eq!(bar.slot(slot), None, "{slot:?}");
        }
        for slot in [Slot::Zoom, Slot::Fit, Slot::Display] {
            assert!(bar.slot(slot).is_some(), "{slot:?}");
        }
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
