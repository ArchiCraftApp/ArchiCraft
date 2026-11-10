//! Parameter transfer in the views (ELM-003): picking an element's
//! settings up into a tool's defaults and injecting a tool's defaults into
//! elements, with the eyedropper and the syringe.
//!
//! [`TransferState`] is the state machine, without egui: a held modifier
//! gesture (Alt for pick-up, Alt+Primary for inject, remappable in the
//! [`Keymap`]) makes a click transfer instead of selecting or placing a
//! point, and releasing the modifiers returns to the tool, whose input in
//! progress stays. A latched mode stays without keys until Esc or a second
//! latch of the same mode. [`click`] is the one handler every pane calls
//! with the element under the cursor, so plan and 3D views transfer alike.
//!
//! On Linux, window managers may take Alt-click for moving windows; the
//! latched modes are the dependable path there (docs/keymap.md).

use archicraft_model::fields::fields_of;
use archicraft_model::transfer::{SettingsSource, SkipReason, TransferReport, transfer_settings};
use archicraft_model::{Command, Element, ElementId};

use crate::commands::Keymap;
use crate::input::Modifiers;
use crate::tools::{ToolKind, ToolState};
use crate::tr;
use crate::workspace::Workspace;

/// What a click on an element does while transferring (ELM-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TransferMode {
    /// The eyedropper: the element's settings become a tool's defaults.
    PickUp,
    /// The syringe: the active tool's defaults go into the element.
    Inject,
}

impl TransferMode {
    pub const ALL: [TransferMode; 2] = [TransferMode::PickUp, TransferMode::Inject];

    /// The modifiers held for the mode by default: Alt (Option on macOS)
    /// for pick-up and Alt+Primary for inject, as in the reference product.
    pub const fn default_gesture(self) -> Modifiers {
        match self {
            TransferMode::PickUp => Modifiers {
                shift: false,
                command: false,
                alt: true,
            },
            TransferMode::Inject => Modifiers {
                shift: false,
                command: true,
                alt: true,
            },
        }
    }
}

/// The transfer state beside the tool state: the latched mode, if any.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferState {
    latched: Option<TransferMode>,
}

impl TransferState {
    /// The mode a click has now: the latched one, otherwise the one whose
    /// gesture `held` matches, if `gestures` are not taken by the tool (the
    /// edge-only Alt-click of contour detection, INP-018). A gesture
    /// matches when exactly its Primary and Alt keys are held; Shift counts
    /// only for a gesture that includes it.
    pub fn mode(&self, held: Modifiers, keymap: &Keymap, gestures: bool) -> Option<TransferMode> {
        self.latched.or_else(|| {
            gestures
                .then(|| {
                    TransferMode::ALL.into_iter().find(|&mode| {
                        let g = keymap.transfer_gesture(mode);
                        (g.command || g.alt)
                            && (g.command, g.alt) == (held.command, held.alt)
                            && (!g.shift || held.shift)
                    })
                })
                .flatten()
        })
    }

    /// The latched mode.
    pub fn latched(&self) -> Option<TransferMode> {
        self.latched
    }

    /// Latches `mode`, or ends it when it is latched already (a second
    /// invocation of its command).
    pub fn latch(&mut self, mode: TransferMode) {
        self.latched = (self.latched != Some(mode)).then_some(mode);
    }

    /// Ends the latched mode, as Esc does; returns whether one was latched.
    pub fn cancel(&mut self) -> bool {
        self.latched.take().is_some()
    }
}

/// Whether the transfer gestures are free with `tool`: contour detection of
/// the wall and slab tools takes Alt-click for its edge-only pick
/// (INP-018), so a held gesture does nothing there; a latched mode still
/// works.
pub fn gestures_free(tool: &ToolState) -> bool {
    !(tool.detecting() && matches!(tool.kind, ToolKind::Wall | ToolKind::Slab))
}

/// Runs `mode` on the element `hit` that a click in any pane found
/// (ELM-003); a click on nothing does nothing. Pick-up copies the element's
/// settings into the active tool's defaults; with a tool that places
/// nothing (selection, measure) into the defaults of the element's own kind,
/// whose tool it activates. Inject copies the active tool's defaults into
/// the element, or into every editable element of the selection when the
/// element is selected. The status line says what transferred and what was
/// skipped, and why.
pub fn click(ws: &mut Workspace, mode: TransferMode, hit: Option<ElementId>) {
    let Some(element) = hit.and_then(|id| ws.doc.project.element(id)).cloned() else {
        return;
    };
    match mode {
        TransferMode::PickUp => pick_up(ws, &element),
        TransferMode::Inject => inject(ws, &element),
    }
}

fn pick_up(ws: &mut Workspace, element: &Element) {
    let project = &ws.doc.project;
    let (into, switch_to) = match ws.tool.kind.element_kind() {
        Some(kind) => (kind, None),
        None => {
            let kind = element.tag();
            (kind, ToolKind::for_kind(kind))
        }
    };
    let applied = project.new_element(into, element.storey, 0).map_or(0, |mut prototype| {
        transfer_settings(element, &mut prototype, project).applied.len()
    });
    let name = element.name.clone();
    let done = ws.run(Command::PickUpSettings {
        element: element.id,
        into,
    });
    if done.is_none() {
        return;
    }
    if let Some(tool) = switch_to {
        ws.select_tool(tool);
    }
    match done {
        Some(true) => ws.info(tr!(
            "status-picked-up",
            count = applied,
            count_text = ws.count_text(applied),
            name = name,
        )),
        _ => ws.info(tr!("status-picked-up-unchanged", name = name)),
    }
}

fn inject(ws: &mut Workspace, element: &Element) {
    let Some(kind) = ws.tool.kind.element_kind() else {
        ws.error(tr!("status-inject-no-defaults"));
        return;
    };
    let project = &ws.doc.project;
    let selected = ws.selected_ids();
    let (targets, locked) = if selected.contains(&element.id) {
        let editable: Vec<ElementId> = selected
            .iter()
            .copied()
            .filter(|id| project.element(*id).is_some_and(|e| project.is_editable(e)))
            .collect();
        let locked = selected.len() - editable.len();
        if editable.is_empty() {
            ws.error(tr!("status-selection-locked"));
            return;
        }
        (editable, locked)
    } else {
        // A locked element is refused by the command with the lock's reason
        // (ELM-013).
        (vec![element.id], 0)
    };
    let preview = match targets.as_slice() {
        [only] => project.element(*only).and_then(|target| {
            let source = project.new_element(kind, project.storeys.ground(), 0)?;
            let mut target = target.clone();
            let report = transfer_settings(&source, &mut target, project);
            Some((target, report))
        }),
        _ => None,
    };
    let count = targets.len();
    let only_name = preview.as_ref().map(|(target, _)| target.name.clone());
    let done = ws.run(Command::InjectSettings {
        source: SettingsSource::Defaults(kind),
        targets,
    });
    let mut text = match (done, preview) {
        (None, _) => return,
        (Some(false), _) if count == 1 => tr!("status-injected-unchanged", name = only_name.unwrap_or_default()),
        (Some(false), _) => tr!(
            "status-injected-unchanged-many",
            count = count,
            count_text = ws.count_text(count),
        ),
        (Some(true), Some((target, report))) => injected_text(ws, &target, &report),
        (Some(true), None) => tr!("status-injected-many", count = count, count_text = ws.count_text(count)),
    };
    if locked > 0 {
        text = tr!(
            "status-injected-locked",
            injected = text,
            count = locked,
            count_text = ws.count_text(locked),
        );
    }
    ws.info(text);
}

/// The status line after injecting into one element: how many of the
/// source's settings it took and, in brackets, the skipped settings the
/// target has, with the reason; those it lacks altogether only count.
fn injected_text(ws: &Workspace, target: &Element, report: &TransferReport) -> String {
    let applied = report.applied.len();
    let total = applied + report.skipped.len();
    let injected = tr!(
        "status-injected",
        total = total,
        applied_text = ws.count_text(applied),
        total_text = ws.count_text(total),
        name = target.name.clone(),
    );
    let skipped: Vec<String> = report
        .skipped
        .iter()
        .filter_map(|(key, reason)| {
            let label = fields_of(target).find(|spec| spec.key == *key)?.label;
            let field = tr!(label);
            Some(match reason {
                SkipReason::Absent => return None,
                SkipReason::NotOffered => tr!("status-transfer-skip-not-offered", field = field),
                SkipReason::Inapplicable => tr!("status-transfer-skip-inapplicable", field = field),
                SkipReason::Invalid(message) => {
                    tr!(
                        "status-transfer-skip-invalid",
                        field = field,
                        reason = ws.render(message)
                    )
                }
            })
        })
        .collect();
    if skipped.is_empty() {
        injected
    } else {
        tr!(
            "status-injected-skipped",
            injected = injected,
            skipped = skipped.join(", ")
        )
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    const NONE: Modifiers = Modifiers {
        shift: false,
        command: false,
        alt: false,
    };
    const ALT: Modifiers = TransferMode::PickUp.default_gesture();
    const ALT_CMD: Modifiers = TransferMode::Inject.default_gesture();
    const CMD: Modifiers = Modifiers {
        shift: false,
        command: true,
        alt: false,
    };

    /// ELM-003: holding Alt picks up, Alt+Primary injects, and releasing the
    /// modifiers returns the click to the tool; Primary alone stays the
    /// selection of a drawing tool, and Shift does not hide the gesture.
    #[test]
    fn held_modifiers_choose_the_mode_and_releasing_them_returns_to_the_tool() {
        let keymap = Keymap::default();
        let state = TransferState::default();
        assert_eq!(state.mode(ALT, &keymap, true), Some(TransferMode::PickUp));
        assert_eq!(state.mode(ALT_CMD, &keymap, true), Some(TransferMode::Inject));
        assert_eq!(state.mode(NONE, &keymap, true), None, "released");
        assert_eq!(state.mode(CMD, &keymap, true), None, "Primary selects");
        let shift_alt = Modifiers { shift: true, ..ALT };
        assert_eq!(state.mode(shift_alt, &keymap, true), Some(TransferMode::PickUp));
        assert_eq!(state.mode(ALT, &keymap, false), None, "taken by the tool");
    }

    /// ELM-003, D-7: the gestures live in the keymap, so a remapped gesture
    /// replaces the default one.
    #[test]
    fn a_remapped_gesture_replaces_the_default() {
        let mut keymap = Keymap::default();
        keymap.set_transfer_gesture(TransferMode::PickUp, Modifiers { shift: true, ..ALT });
        let state = TransferState::default();
        assert_eq!(state.mode(ALT, &keymap, true), None);
        assert_eq!(
            state.mode(Modifiers { shift: true, ..ALT }, &keymap, true),
            Some(TransferMode::PickUp)
        );
    }

    /// ELM-003: a latched mode survives clicks and every change of the
    /// modifiers, even to the other mode's gesture, until Esc; a second
    /// latch of the same mode ends it, of the other mode switches to it.
    #[test]
    fn a_latched_mode_survives_clicks_and_modifier_changes_until_esc() {
        let keymap = Keymap::default();
        let mut state = TransferState::default();
        state.latch(TransferMode::PickUp);
        for held in [NONE, ALT_CMD, CMD, ALT, NONE] {
            assert_eq!(state.mode(held, &keymap, true), Some(TransferMode::PickUp), "{held:?}");
            assert_eq!(state.mode(held, &keymap, false), Some(TransferMode::PickUp), "{held:?}");
        }
        assert!(state.cancel(), "Esc ends the latch");
        assert_eq!(state.mode(NONE, &keymap, true), None);
        assert!(!state.cancel(), "nothing left to end");

        state.latch(TransferMode::Inject);
        state.latch(TransferMode::PickUp);
        assert_eq!(state.latched(), Some(TransferMode::PickUp));
        state.latch(TransferMode::PickUp);
        assert_eq!(state.latched(), None, "a second invocation ends it");
    }
}
