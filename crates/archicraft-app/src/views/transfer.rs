//! Parameter transfer in the views (ELM-003): picking an element's
//! settings up into a tool's defaults and injecting a tool's defaults into
//! elements, with the eyedropper and the syringe.
//!
//! [`TransferState`] is the state machine, without egui: a held modifier
//! gesture (Alt for pick-up, Alt+Primary for inject, remappable in the
//! [`Keymap`]) makes a click transfer instead of selecting or placing a
//! point, and releasing the modifiers returns to the tool, whose input in
//! progress stays. A latched mode stays without keys until Esc or a second
//! latch of the same mode (the Pick up and Inject parameters commands,
//! EDT-020). [`click`] is the one handler every pane calls with the element
//! under the cursor, so plan and 3D views transfer alike, and [`preview`]
//! says before the click what it would transfer and skip.
//!
//! On Linux, window managers may take Alt-click for moving windows; the
//! latched modes are the dependable path there (docs/keymap.md).

use archicraft_model::fields::fields_of;
use archicraft_model::transfer::{SettingsSource, SkipReason, TransferReport, transfer_settings};
use archicraft_model::{Command, Element, ElementId, KindTag};

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

/// The kind of the defaults a pick-up from `element` goes into: the active
/// tool's, or with a tool that places nothing the element's own, whose tool
/// the pick-up activates.
fn pick_up_into(ws: &Workspace, element: &Element) -> (KindTag, Option<ToolKind>) {
    match ws.tool.kind.element_kind() {
        Some(kind) => (kind, None),
        None => {
            let kind = element.tag();
            (kind, ToolKind::for_kind(kind))
        }
    }
}

/// A rehearsal of `mode` on `element` with clones (ELM-003): the element the
/// transfer would change as it would be afterwards (the defaults of the kind
/// picked up into, or the injected element) with the report of what it took
/// and skipped. `None` for an inject without a tool's defaults.
fn rehearse(ws: &Workspace, mode: TransferMode, element: &Element) -> Option<(Element, TransferReport)> {
    let project = &ws.doc.project;
    match mode {
        TransferMode::PickUp => {
            let (into, _) = pick_up_into(ws, element);
            let mut defaults = project.new_element(into, element.storey, 0)?;
            let report = transfer_settings(element, &mut defaults, project);
            Some((defaults, report))
        }
        TransferMode::Inject => {
            let source = project.new_element(ws.tool.kind.element_kind()?, project.storeys.ground(), 0)?;
            let mut target = element.clone();
            let report = transfer_settings(&source, &mut target, project);
            Some((target, report))
        }
    }
}

/// What a click in `mode` would do on an element, shown while it is hovered
/// (ELM-003, EDT-020).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preview {
    /// What the click does, such as "Pick up from W-003".
    pub title: String,
    /// The labels of the settings that would transfer.
    pub applied: Vec<String>,
    /// The settings that would be skipped, each with its reason, such as
    /// "IFC class not allowed"; those the target lacks altogether are left
    /// out, as in the status line.
    pub skipped: Vec<String>,
    /// Why nothing would happen, instead of the lists: no tool's defaults
    /// to inject, or a locked element.
    pub refused: Option<String>,
}

/// The preview of a click in `mode` on the element `id`, from a rehearsal
/// with clones: nothing changes until the click. An inject previews the
/// elements the click goes into, as the inject chooses them: with `id` in
/// the selection only its editable elements, counted in the title and
/// refused only when none is; the lists are those of `id`, or of the first
/// editable selected element while `id` is locked.
pub fn preview(ws: &Workspace, mode: TransferMode, id: ElementId) -> Option<Preview> {
    let project = &ws.doc.project;
    let hovered = project.element(id)?;
    let refused = |title, reason| Preview {
        title,
        applied: Vec::new(),
        skipped: Vec::new(),
        refused: Some(reason),
    };
    let (title, element) = match mode {
        TransferMode::PickUp => (
            tr!("view-transfer-preview-pick-up", name = hovered.name.clone()),
            hovered,
        ),
        TransferMode::Inject => {
            let locked = || {
                Some(refused(
                    tr!("view-transfer-preview-inject", name = hovered.name.clone()),
                    tr!("view-transfer-preview-locked"),
                ))
            };
            let Some((targets, _)) = inject_targets(ws, hovered) else {
                return locked();
            };
            let shown = if targets.contains(&id) {
                hovered
            } else {
                project.element(*targets.first()?)?
            };
            // A locked element alone is refused by the command (ELM-013).
            if !project.is_editable(shown) {
                return locked();
            }
            let name = shown.name.clone();
            let title = match targets.len() {
                1 => tr!("view-transfer-preview-inject", name = name),
                n => tr!(
                    "view-transfer-preview-inject-selection",
                    count = n,
                    count_text = ws.count_text(n),
                    name = name,
                ),
            };
            (title, shown)
        }
    };
    let Some((target, report)) = rehearse(ws, mode, element) else {
        return Some(refused(title, tr!("status-inject-no-defaults")));
    };
    let applied = report
        .applied
        .iter()
        .filter_map(|key| fields_of(&target).find(|spec| spec.key == *key))
        .map(|spec| tr!(spec.label))
        .collect();
    let skipped = report
        .skipped
        .iter()
        .filter_map(|(key, reason)| skip_text(ws, &target, key, reason))
        .collect();
    Some(Preview {
        title,
        applied,
        skipped,
        refused: None,
    })
}

fn pick_up(ws: &mut Workspace, element: &Element) {
    let (into, switch_to) = pick_up_into(ws, element);
    let applied = rehearse(ws, TransferMode::PickUp, element).map_or(0, |(_, report)| report.applied.len());
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

/// The elements an inject on `element` goes into (ELM-003), with the count
/// of the selection's locked elements it skips: every editable element of
/// the selection when `element` is selected, otherwise `element` alone,
/// which the command refuses with the lock's reason when it is locked
/// (ELM-013). `None` when `element` is selected and every selected element
/// is locked.
fn inject_targets(ws: &Workspace, element: &Element) -> Option<(Vec<ElementId>, usize)> {
    let project = &ws.doc.project;
    let selected = ws.selected_ids();
    if !selected.contains(&element.id) {
        return Some((vec![element.id], 0));
    }
    let editable: Vec<ElementId> = selected
        .iter()
        .copied()
        .filter(|id| project.element(*id).is_some_and(|e| project.is_editable(e)))
        .collect();
    let locked = selected.len() - editable.len();
    (!editable.is_empty()).then_some((editable, locked))
}

fn inject(ws: &mut Workspace, element: &Element) {
    let Some(kind) = ws.tool.kind.element_kind() else {
        ws.error(tr!("status-inject-no-defaults"));
        return;
    };
    let Some((targets, locked)) = inject_targets(ws, element) else {
        ws.error(tr!("status-selection-locked"));
        return;
    };
    let project = &ws.doc.project;
    let preview = match targets.as_slice() {
        [only] => project
            .element(*only)
            .and_then(|target| rehearse(ws, TransferMode::Inject, target)),
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
        .filter_map(|(key, reason)| skip_text(ws, target, key, reason))
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

/// The setting `key` of `target` that a transfer skipped, with the reason,
/// such as "IFC class not allowed"; `None` for one `target` lacks.
fn skip_text(ws: &Workspace, target: &Element, key: &str, reason: &SkipReason) -> Option<String> {
    let label = fields_of(target).find(|spec| spec.key == key)?.label;
    let field = tr!(label);
    Some(match reason {
        SkipReason::Absent => return None,
        SkipReason::NotOffered => tr!("status-transfer-skip-not-offered", field = field),
        SkipReason::Inapplicable => tr!("status-transfer-skip-inapplicable", field = field),
        SkipReason::Invalid(message) => tr!(
            "status-transfer-skip-invalid",
            field = field,
            reason = ws.render(message)
        ),
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use archicraft_model::glam::DVec2;
    use archicraft_model::template::default_metric_project;

    use super::*;
    use crate::document::Document;
    use crate::persistence::prefs::Preferences;

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

    /// A workspace with the wall tool and four walls, the last two locked.
    fn walls() -> (Workspace, Vec<ElementId>) {
        let mut ws = Workspace::new(Document::new(default_metric_project(), 100), Preferences::default());
        let p = &ws.doc.project;
        let ground = p.storeys.ground();
        let walls: Vec<Element> = (0..4)
            .map(|n| {
                let y = 3.0 * f64::from(n);
                let mut e = p.new_wall(ground, DVec2::new(0.0, y), DVec2::new(4.0, y), n);
                e.locked = n >= 2;
                e
            })
            .collect();
        let ids = walls.iter().map(|e| e.id).collect();
        assert!(ws.execute(Command::AddElements(walls)));
        ws.select_tool(ToolKind::Wall);
        (ws, ids)
    }

    fn name(ws: &Workspace, id: ElementId) -> String {
        ws.doc.project.element(id).unwrap().name.clone()
    }

    /// ELM-003, EDT-020: hovering a selected element previews the inject
    /// into the selection's editable elements, which it counts, as the
    /// click does; a locked hovered element among them previews the inject
    /// into the others instead of refusing.
    #[test]
    fn an_inject_preview_on_the_selection_counts_only_its_editable_elements() {
        let (mut ws, ids) = walls();
        ws.selection = [ids[0], ids[1], ids[2]].into_iter().collect();
        let on_editable = preview(&ws, TransferMode::Inject, ids[0]).unwrap();
        assert_eq!(
            on_editable.title,
            format!("Inject into 2 selected elements, such as {}", name(&ws, ids[0]))
        );
        assert_eq!(on_editable.refused, None);
        assert!(on_editable.applied.contains(&"Thickness".to_owned()), "{on_editable:?}");

        let on_locked = preview(&ws, TransferMode::Inject, ids[2]).unwrap();
        assert_eq!(on_locked.refused, None, "the click injects into the others");
        assert!(on_locked.title.contains("2 selected elements"), "{}", on_locked.title);
        assert!(on_locked.applied.contains(&"Thickness".to_owned()), "{on_locked:?}");

        // One editable element left: the inject goes into it alone.
        ws.selection = [ids[1], ids[2]].into_iter().collect();
        let one = preview(&ws, TransferMode::Inject, ids[2]).unwrap();
        assert_eq!(one.title, format!("Inject into {}", name(&ws, ids[1])));
        assert_eq!(one.refused, None);
    }

    /// ELM-003, ELM-013: an inject preview refuses a locked element that
    /// would be the only target, and a selection with nothing editable; a
    /// pick-up from a locked element changes only the defaults and goes.
    #[test]
    fn an_inject_preview_refuses_a_locked_element_alone() {
        let (mut ws, ids) = walls();
        let locked = Some("Locked: nothing would change".to_owned());
        let alone = preview(&ws, TransferMode::Inject, ids[2]).unwrap();
        assert_eq!(alone.title, format!("Inject into {}", name(&ws, ids[2])));
        assert_eq!(alone.refused, locked);
        assert!(alone.applied.is_empty() && alone.skipped.is_empty());
        for selection in [vec![ids[2]], vec![ids[2], ids[3]]] {
            ws.selection = selection.into_iter().collect();
            assert_eq!(preview(&ws, TransferMode::Inject, ids[2]).unwrap().refused, locked);
        }
        // Not selected: the click injects into the hovered element alone.
        let unselected = preview(&ws, TransferMode::Inject, ids[0]).unwrap();
        assert_eq!(unselected.title, format!("Inject into {}", name(&ws, ids[0])));
        assert_eq!(unselected.refused, None);
        let pick_up = preview(&ws, TransferMode::PickUp, ids[2]).unwrap();
        assert_eq!(pick_up.refused, None);
        assert!(pick_up.applied.contains(&"Thickness".to_owned()), "{pick_up:?}");
    }

    /// ELM-003: with a tool that has no defaults (selection, measure), an
    /// inject preview says to choose the tool whose settings to inject.
    #[test]
    fn an_inject_preview_without_tool_defaults_says_to_choose_a_tool() {
        let (mut ws, ids) = walls();
        for tool in [ToolKind::Select, ToolKind::Measure] {
            ws.select_tool(tool);
            let preview = preview(&ws, TransferMode::Inject, ids[0]).unwrap();
            assert_eq!(preview.title, format!("Inject into {}", name(&ws, ids[0])));
            assert_eq!(
                preview.refused,
                Some("Choose the tool whose settings to inject".to_owned())
            );
        }
    }

    /// ELM-003, EDT-020: when the target rejects the transfer as a whole (a
    /// text without content, which the validator refuses however its
    /// settings are), the preview lists nothing as transferred, so the
    /// tooltip says there is nothing to transfer, and refuses nothing.
    #[test]
    fn a_preview_where_nothing_fits_lists_nothing_as_transferred() {
        let (mut ws, _) = walls();
        let p = &ws.doc.project;
        let text = p.new_text(p.storeys.ground(), DVec2::new(10.0, 0.0), "", 0);
        let id = text.id;
        ws.doc.project.elements.insert(id, text);
        let preview = preview(&ws, TransferMode::Inject, id).unwrap();
        assert_eq!(preview.refused, None);
        assert!(preview.applied.is_empty(), "{preview:?}");
        assert!(!preview.skipped.is_empty(), "the skipped settings say why: {preview:?}");
    }
}
