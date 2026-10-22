//! Command objects and the undo history (NFR-MNT-001, UND-001, UND-002,
//! UND-005, STR-005).
//!
//! Every change to a project is a [`Command`]. Executing a command first plans
//! it against the current project, which validates it and produces a list of
//! primitive [`Edit`]s, and then applies those edits. Applying an edit returns
//! its inverse, so undo applies the inverses in reverse order and redo applies
//! the recorded forward edits again. Redo therefore never re-plans a command
//! and identities stay stable. Commands are serialisable, so they can be
//! logged and replayed.

use std::collections::{HashMap, VecDeque};

use glam::DVec2;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::element::{Element, KindTag};
use crate::ids::{ElementId, LayerId, MaterialId, StoreyId};
use crate::layer::Layer;
use crate::material::Material;
use crate::message::Message;
use crate::project::{DefaultsSlot, Location, NameCounters, Project, ProjectInfo, ProjectSettings, ToolDefaults};
use crate::storey::{MAX_CUT_PLANE_HEIGHT, Storey, StoreyList};
use crate::tolerance::near_zero;
use crate::transform::{PlanTransform, TransformError};
use crate::validate::{self, MAX_DECIMALS};

/// What happens to the elements of a storey that is deleted (STR-004).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StoreyElements {
    Delete,
    /// Move them to another storey, keeping their absolute position.
    MoveTo(StoreyId),
}

/// A change to the project, as requested by the user or a script.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Command {
    AddElements(Vec<Element>),
    DeleteElements(Vec<ElementId>),
    /// Replaces elements by ID with new versions.
    UpdateElements(Vec<Element>),
    MoveElements {
        ids: Vec<ElementId>,
        delta: DVec2,
    },
    InsertStorey {
        reference: StoreyId,
        above: bool,
        storey: Storey,
    },
    DeleteStorey {
        storey: StoreyId,
        elements: StoreyElements,
    },
    RenameStorey {
        storey: StoreyId,
        name: String,
    },
    SetStoreyElevation {
        storey: StoreyId,
        elevation: f64,
        shift_above: bool,
    },
    SetStoreyHeight {
        storey: StoreyId,
        height: f64,
        shift_above: bool,
    },
    SetStoreyCutPlane {
        storey: StoreyId,
        height: Option<f64>,
    },
    AddLayer(Layer),
    UpdateLayer(Layer),
    DeleteLayer {
        layer: LayerId,
        reassign_to: LayerId,
    },
    AddMaterial(Material),
    UpdateMaterial(Material),
    DeleteMaterial {
        material: MaterialId,
        replacement: MaterialId,
    },
    SetToolDefaults(ToolDefaults),
    SetSettings(ProjectSettings),
    SetInfo(ProjectInfo),
    SetSpatialNames {
        site: String,
        building: String,
    },
    SetLocation(Location),
}

/// Why a command was rejected. Each variant carries a message key with its
/// arguments (NFR-L10N-001); `Display` is the key form, for logs only, and
/// the user interface renders [`CommandError::message`].
#[derive(Clone, Debug, PartialEq, Error)]
pub enum CommandError {
    /// The command would produce an invalid model (NFR-REL-005).
    #[error("{0}")]
    Invalid(Message),
    /// An affected element or layer is locked (ELM-013, LAY-001).
    #[error("{0}")]
    Locked(Message),
    /// The command refers to an object that does not exist.
    #[error("{0}")]
    NotFound(Message),
    /// The command changes nothing.
    #[error("model-error-no-change")]
    NoChange,
}

impl CommandError {
    /// The message to show the user; [`CommandError::NoChange`] is
    /// `model-error-no-change`.
    pub fn message(&self) -> Message {
        match self {
            CommandError::Invalid(m) | CommandError::Locked(m) | CommandError::NotFound(m) => m.clone(),
            CommandError::NoChange => Message::new("model-error-no-change"),
        }
    }
}

fn invalid(key: &'static str) -> CommandError {
    CommandError::Invalid(Message::new(key))
}

fn not_found(key: &'static str) -> CommandError {
    CommandError::NotFound(Message::new(key))
}

impl Command {
    /// Short label for menus and the history, for example "Delete 3
    /// elements", as a message key (NFR-L10N-001).
    pub fn label(&self) -> Message {
        let elements = |key, n: usize| Message::new(key).count("count", n);
        match self {
            Command::AddElements(e) if e.len() == 1 => Message::new("model-label-create-kind").kind("kind", e[0].tag()),
            Command::AddElements(e) => elements("model-label-create-elements", e.len()),
            Command::DeleteElements(ids) => elements("model-label-delete-elements", ids.len()),
            Command::UpdateElements(e) => elements("model-label-edit-elements", e.len()),
            Command::MoveElements { ids, .. } => elements("model-label-move-elements", ids.len()),
            Command::InsertStorey { .. } => Message::new("model-label-insert-storey"),
            Command::DeleteStorey { .. } => Message::new("model-label-delete-storey"),
            Command::RenameStorey { .. } => Message::new("model-label-rename-storey"),
            Command::SetStoreyElevation { .. } => Message::new("model-label-storey-elevation"),
            Command::SetStoreyHeight { .. } => Message::new("model-label-storey-height"),
            Command::SetStoreyCutPlane { .. } => Message::new("model-label-cut-plane"),
            Command::AddLayer(_) => Message::new("model-label-new-layer"),
            Command::UpdateLayer(_) => Message::new("model-label-change-layer"),
            Command::DeleteLayer { .. } => Message::new("model-label-delete-layer"),
            Command::AddMaterial(_) => Message::new("model-label-new-material"),
            Command::UpdateMaterial(_) => Message::new("model-label-change-material"),
            Command::DeleteMaterial { .. } => Message::new("model-label-delete-material"),
            Command::SetToolDefaults(_) => Message::new("model-label-tool-settings"),
            Command::SetSettings(_) => Message::new("model-label-project-settings"),
            Command::SetInfo(_) => Message::new("model-label-project-info"),
            Command::SetSpatialNames { .. } => Message::new("model-label-spatial-names"),
            Command::SetLocation(_) => Message::new("model-label-location"),
        }
    }

    /// Validates the command against `project` and returns the edits that
    /// carry it out. Does not change the project.
    pub fn plan(&self, project: &Project) -> Result<Vec<Edit>, CommandError> {
        match self {
            Command::AddElements(elements) => plan_add_elements(project, elements),
            Command::DeleteElements(ids) => plan_delete_elements(project, ids),
            Command::UpdateElements(elements) => plan_update_elements(project, elements),
            Command::MoveElements { ids, delta } => {
                if !delta.is_finite() {
                    return Err(invalid("model-error-invalid-distance"));
                }
                plan_transform_elements(project, ids, &PlanTransform::translation(*delta))
            }
            Command::InsertStorey {
                reference,
                above,
                storey,
            } => {
                if project.storeys.get(storey.id).is_some() {
                    return Err(invalid("model-error-duplicate-storey-id"));
                }
                let list = project
                    .storeys
                    .insert(*reference, *above, storey.clone())
                    .map_err(CommandError::Invalid)?;
                check_linked_heights(project, &list, &[])?;
                Ok(vec![Edit::SetStoreys(list)])
            }
            Command::DeleteStorey { storey, elements } => plan_delete_storey(project, *storey, *elements),
            Command::RenameStorey { storey, name } => {
                let list = project.storeys.rename(*storey, name).map_err(CommandError::Invalid)?;
                storey_edit(project, list)
            }
            Command::SetStoreyElevation {
                storey,
                elevation,
                shift_above,
            } => {
                let list = project
                    .storeys
                    .set_elevation(*storey, *elevation, *shift_above)
                    .map_err(CommandError::Invalid)?;
                storey_edit(project, list)
            }
            Command::SetStoreyHeight {
                storey,
                height,
                shift_above,
            } => {
                let list = project
                    .storeys
                    .set_height(*storey, *height, *shift_above)
                    .map_err(CommandError::Invalid)?;
                storey_edit(project, list)
            }
            Command::SetStoreyCutPlane { storey, height } => {
                let list = project
                    .storeys
                    .set_cut_plane(*storey, *height)
                    .map_err(CommandError::Invalid)?;
                storey_edit(project, list)
            }
            Command::AddLayer(layer) => {
                check_layer(layer)?;
                if project.layer(layer.id).is_some() {
                    return Err(invalid("model-error-duplicate-layer-id"));
                }
                Ok(vec![Edit::InsertLayer {
                    index: project.layers.len(),
                    layer: layer.clone(),
                }])
            }
            Command::UpdateLayer(layer) => {
                check_layer(layer)?;
                match project.layer(layer.id) {
                    None => Err(not_found("model-error-layer-not-found")),
                    Some(old) if old == layer => Err(CommandError::NoChange),
                    Some(_) => Ok(vec![Edit::ReplaceLayer(layer.clone())]),
                }
            }
            Command::DeleteLayer { layer, reassign_to } => plan_delete_layer(project, *layer, *reassign_to),
            Command::AddMaterial(material) => {
                material.validate().map_err(CommandError::Invalid)?;
                if project.material(material.id).is_some() {
                    return Err(invalid("model-error-duplicate-material-id"));
                }
                Ok(vec![Edit::InsertMaterial {
                    index: project.materials.len(),
                    material: material.clone(),
                }])
            }
            Command::UpdateMaterial(material) => {
                material.validate().map_err(CommandError::Invalid)?;
                match project.material(material.id) {
                    None => Err(not_found("model-error-material-not-found")),
                    Some(old) if old == material => Err(CommandError::NoChange),
                    Some(_) => Ok(vec![Edit::ReplaceMaterial(material.clone())]),
                }
            }
            Command::DeleteMaterial { material, replacement } => plan_delete_material(project, *material, *replacement),
            Command::SetToolDefaults(defaults) => {
                check_tool_defaults(project, defaults)?;
                if &project.tool_defaults == defaults {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::SetToolDefaults(defaults.clone())])
            }
            Command::SetSettings(settings) => {
                check_settings(settings)?;
                if &project.settings == settings {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::SetSettings(settings.clone())])
            }
            Command::SetInfo(info) => {
                if info.name.trim().is_empty() {
                    return Err(invalid("model-error-project-name"));
                }
                if &project.info == info {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::SetInfo(info.clone())])
            }
            Command::SetSpatialNames { site, building } => {
                if site.trim().is_empty() || building.trim().is_empty() {
                    return Err(invalid("model-error-spatial-names"));
                }
                if &project.site.name == site && &project.building.name == building {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::SetSpatialNames {
                    site: site.trim().to_owned(),
                    building: building.trim().to_owned(),
                }])
            }
            Command::SetLocation(location) => {
                let values = [
                    location.altitude,
                    location.survey_point[0],
                    location.survey_point[1],
                    location.true_north,
                ];
                if values.iter().any(|v| !v.is_finite()) {
                    return Err(invalid("model-error-location-invalid"));
                }
                if &project.site.location == location {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::SetLocation(*location)])
            }
        }
    }
}

fn find_element(project: &Project, id: ElementId) -> Result<&Element, CommandError> {
    project
        .element(id)
        .ok_or_else(|| not_found("model-error-element-not-found"))
}

fn storey_edit(project: &Project, list: StoreyList) -> Result<Vec<Edit>, CommandError> {
    if list == project.storeys {
        return Err(CommandError::NoChange);
    }
    check_linked_heights(project, &list, &[])?;
    Ok(vec![Edit::SetStoreys(list)])
}

/// Refuses a new storey list that would leave an element whose extent
/// depends on the storeys lower than [`validate::MIN_HEIGHT`] (NFR-REL-005,
/// ELM-017, STR-002). `element_edits` are the element changes planned with
/// the list (removals and replacements). Only elements the edit lowers
/// count, beyond the rounding of their elevations ([`near_zero`]): a
/// fixed-height element on a moved storey keeps its height, and an element
/// made too low outside the commands does not block storey edits that leave
/// it as it is or raise it. The message names the first element and counts
/// the others.
fn check_linked_heights(project: &Project, list: &StoreyList, element_edits: &[Edit]) -> Result<(), CommandError> {
    let planned: HashMap<ElementId, Option<&Element>> = element_edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveElement { id } => Some((*id, None)),
            Edit::ReplaceElement(new) => Some((new.id, Some(new))),
            _ => None,
        })
        .collect();
    let mut too_low = project.elements.values().filter(|old| {
        let new = planned.get(&old.id).copied().unwrap_or(Some(old));
        let Some((bottom, top)) = new.and_then(|new| new.kind.vertical_extent(new.storey, list)) else {
            return false;
        };
        let after = top - bottom;
        after < validate::MIN_HEIGHT
            && project.vertical_extent(old).is_none_or(|(old_bottom, old_top)| {
                let before = old_top - old_bottom;
                let scale = [bottom, top, old_bottom, old_top]
                    .map(f64::abs)
                    .into_iter()
                    .fold(0.0, f64::max);
                after < before && !near_zero(before - after, scale)
            })
    });
    let Some(first) = too_low.next() else {
        return Ok(());
    };
    Err(CommandError::Invalid(
        Message::new("model-error-linked-element-too-low")
            .text("name", &first.name)
            .count("count", too_low.count())
            .length("min", validate::MIN_HEIGHT),
    ))
}

fn plan_add_elements(project: &Project, elements: &[Element]) -> Result<Vec<Edit>, CommandError> {
    if elements.is_empty() {
        return Err(CommandError::NoChange);
    }
    let mut edits = Vec::with_capacity(elements.len() + 1);
    let mut counters = project.counters.clone();
    for (i, element) in elements.iter().enumerate() {
        if project.element(element.id).is_some() || elements[..i].iter().any(|e| e.id == element.id) {
            return Err(invalid("model-error-duplicate-element-id"));
        }
        let element = validate::validate_element(project, element).map_err(CommandError::Invalid)?;
        counters.bump(element.tag());
        edits.push(Edit::InsertElement {
            index: project.elements.len() + i,
            element,
        });
    }
    edits.push(Edit::SetCounters(counters));
    Ok(edits)
}

fn plan_delete_elements(project: &Project, ids: &[ElementId]) -> Result<Vec<Edit>, CommandError> {
    if ids.is_empty() {
        return Err(CommandError::NoChange);
    }
    let mut edits = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            continue;
        }
        let element = find_element(project, *id)?;
        if let Some(reason) = project.edit_lock_reason(element) {
            return Err(CommandError::Locked(reason));
        }
        edits.push(Edit::RemoveElement { id: *id });
    }
    Ok(edits)
}

/// True if `new` differs from `old` only in the element lock flag or the
/// snap flag, the changes an element-locked element accepts (ELM-013,
/// INP-017): the snap flag is an input aid, not an edit of the element.
fn only_lock_or_snap_changed(old: &Element, new: &Element) -> bool {
    Element {
        locked: old.locked,
        snap_excluded: old.snap_excluded,
        ..new.clone()
    } == *old
}

fn plan_update_elements(project: &Project, elements: &[Element]) -> Result<Vec<Edit>, CommandError> {
    let mut edits = Vec::with_capacity(elements.len());
    for element in elements {
        let old = find_element(project, element.id)?;
        if old == element {
            continue;
        }
        if old.tag() != element.tag() {
            return Err(invalid("model-error-kind-change"));
        }
        if let Some(reason) = project.edit_lock_reason(old) {
            let layer_locked = project.layer(old.layer).is_some_and(|l| l.locked);
            if layer_locked || !only_lock_or_snap_changed(old, element) {
                return Err(CommandError::Locked(reason));
            }
        }
        let element = validate::validate_element(project, element).map_err(|e| {
            CommandError::Invalid(
                Message::new("model-error-element-invalid")
                    .text("name", &element.name)
                    .nested("reason", e),
            )
        })?;
        edits.push(Edit::ReplaceElement(element));
    }
    if edits.is_empty() {
        return Err(CommandError::NoChange);
    }
    Ok(edits)
}

/// Moves, rotates or mirrors elements through their kinds' transform hooks
/// (EDT-001…EDT-003). A kind that cannot represent the result rejects the
/// whole command.
fn plan_transform_elements(
    project: &Project,
    ids: &[ElementId],
    transform: &PlanTransform,
) -> Result<Vec<Edit>, CommandError> {
    let transformed = ids
        .iter()
        .map(|id| {
            let mut e = find_element(project, *id)?.clone();
            e.kind.transform(transform).map_err(|err| match err {
                TransformError::Unsupported => {
                    CommandError::Invalid(Message::new("model-error-transform-unsupported").text("name", &e.name))
                }
            })?;
            Ok(e)
        })
        .collect::<Result<Vec<_>, CommandError>>()?;
    plan_update_elements(project, &transformed)
}

/// Keeps an element's absolute position while it moves to `target` storey
/// whose elevation will be `target_elevation`; storey-linked values become
/// fixed (STR-004).
fn rehome(project: &Project, element: &Element, target: StoreyId, target_elevation: f64) -> Option<Element> {
    let mut moved = element.clone();
    moved.kind.rehome(element.storey, &project.storeys, target_elevation)?;
    moved.storey = target;
    Some(moved)
}

/// The element moved to another home storey while keeping its absolute
/// position (ELM-005). A top linked relative to the old home storey is
/// linked to the same storey absolutely, so the top stays where it is.
pub fn relink_to_storey(project: &Project, element: &Element, target: StoreyId) -> Option<Element> {
    let mut moved = element.clone();
    moved.kind.relink(element.storey, &project.storeys, target)?;
    moved.storey = target;
    Some(moved)
}

fn plan_delete_storey(project: &Project, storey: StoreyId, mode: StoreyElements) -> Result<Vec<Edit>, CommandError> {
    let list = project.storeys.remove(storey).map_err(CommandError::Invalid)?;
    let mut edits = Vec::new();
    for element in project.elements.values() {
        if element.storey == storey {
            match mode {
                StoreyElements::Delete => edits.push(Edit::RemoveElement { id: element.id }),
                StoreyElements::MoveTo(target) => {
                    if target == storey {
                        return Err(invalid("model-error-move-target-same"));
                    }
                    let target_elevation = list
                        .get(target)
                        .ok_or_else(|| not_found("model-error-target-storey-not-found"))?
                        .elevation;
                    let moved = rehome(project, element, target, target_elevation).ok_or_else(|| {
                        CommandError::Invalid(
                            Message::new("model-error-element-not-movable").text("name", &element.name),
                        )
                    })?;
                    edits.push(Edit::ReplaceElement(moved));
                }
            }
        } else {
            // A linked storey disappears: keep the current top as a fixed
            // height (STR-004).
            let mut kept = element.clone();
            if kept.kind.unlink_storey(element.storey, &project.storeys, storey) {
                edits.push(Edit::ReplaceElement(kept));
            }
        }
    }
    // ELM-006: default tops linked to the storey become fixed heights above
    // the ground storey, like the elements' tops. Where that height is
    // invalid (zero or less, or above the maximum), only the top changes, to
    // the height of the remaining ground storey; the rest of the slot stays
    // (ELM-001).
    let ground = project.storeys.ground();
    let fallback = list
        .get(list.ground())
        .map_or(validate::MIN_HEIGHT, |s| s.height)
        .clamp(validate::MIN_HEIGHT, validate::MAX_HEIGHT);
    let defaults: ToolDefaults = project
        .tool_defaults
        .iter()
        .map(|(kind, slot)| {
            let mut slot = slot.clone();
            if slot.kind.unlink_storey(ground, &project.storeys, storey) && check_slot(project, kind, &slot).is_err() {
                slot.kind.fix_top(fallback);
            }
            slot
        })
        .collect();
    if defaults != project.tool_defaults {
        edits.push(Edit::SetToolDefaults(defaults));
    }
    check_linked_heights(project, &list, &edits)?;
    edits.push(Edit::SetStoreys(list));
    Ok(edits)
}

fn check_layer(layer: &Layer) -> Result<(), CommandError> {
    if layer.name.trim().is_empty() {
        return Err(invalid("model-error-layer-name"));
    }
    Ok(())
}

fn plan_delete_layer(project: &Project, layer: LayerId, reassign_to: LayerId) -> Result<Vec<Edit>, CommandError> {
    if project.layer(layer).is_none() {
        return Err(not_found("model-error-layer-not-found"));
    }
    if layer == reassign_to || project.layer(reassign_to).is_none() {
        return Err(invalid("model-error-reassign-layer"));
    }
    let mut edits: Vec<Edit> = project
        .elements
        .values()
        .filter(|e| e.layer == layer)
        .map(|e| {
            Edit::ReplaceElement(Element {
                layer: reassign_to,
                ..e.clone()
            })
        })
        .collect();
    let mut defaults = project.tool_defaults.clone();
    for slot in defaults.slots_mut().filter(|s| s.layer == layer) {
        slot.layer = reassign_to;
    }
    if defaults != project.tool_defaults {
        edits.push(Edit::SetToolDefaults(defaults));
    }
    edits.push(Edit::RemoveLayer { id: layer });
    Ok(edits)
}

fn plan_delete_material(
    project: &Project,
    material: MaterialId,
    replacement: MaterialId,
) -> Result<Vec<Edit>, CommandError> {
    if project.material(material).is_none() {
        return Err(not_found("model-error-material-not-found"));
    }
    // DSP-003: deleting a material needs a replacement.
    if material == replacement || project.material(replacement).is_none() {
        return Err(invalid("model-error-replacement-material"));
    }
    let mut edits: Vec<Edit> = project
        .elements
        .values()
        .filter(|e| e.kind.material() == material)
        .map(|e| {
            let mut changed = e.clone();
            changed.kind.set_material(replacement);
            Edit::ReplaceElement(changed)
        })
        .collect();
    let mut defaults = project.tool_defaults.clone();
    for slot in defaults.slots_mut().filter(|s| s.kind.material() == material) {
        slot.kind.set_material(replacement);
    }
    if defaults != project.tool_defaults {
        edits.push(Edit::SetToolDefaults(defaults));
    }
    edits.push(Edit::RemoveMaterial { id: material });
    Ok(edits)
}

/// Checks every slot as the prototype element it makes on the ground
/// storey: its layer, material, IFC class among the kind's (ELM-011),
/// parameters and storey links (ELM-006), through
/// [`validate::validate_prototype`]. Every kind with template defaults needs
/// a slot (ELM-001).
fn check_tool_defaults(project: &Project, defaults: &ToolDefaults) -> Result<(), CommandError> {
    if let Some(&kind) = KindTag::ALL
        .iter()
        .find(|k| k.descriptor().template_defaults.is_some() && defaults.get(**k).is_none())
    {
        return Err(CommandError::Invalid(
            Message::new("model-error-default-missing").kind("kind", kind),
        ));
    }
    for (kind, slot) in defaults.iter() {
        check_slot(project, kind, slot)?;
    }
    Ok(())
}

/// Checks the slot stored under `kind` as the prototype element it makes on
/// the ground storey (see [`check_tool_defaults`]).
fn check_slot(project: &Project, kind: KindTag, slot: &DefaultsSlot) -> Result<(), CommandError> {
    let invalid_slot = |reason| {
        CommandError::Invalid(
            Message::new("model-error-default-invalid")
                .kind("kind", kind)
                .nested("reason", reason),
        )
    };
    if slot.tag() != kind {
        return Err(invalid_slot(Message::new("model-error-kind-change")));
    }
    let prototype = Element {
        id: ElementId::new(),
        name: project.next_element_name(kind, 0),
        storey: project.storeys.ground(),
        layer: slot.layer,
        ifc_class: slot.ifc_class,
        position: slot.position,
        locked: false,
        snap_excluded: false,
        kind: slot.kind.clone(),
    };
    validate::validate_prototype(project, &prototype).map_err(invalid_slot)?;
    Ok(())
}

fn check_settings(settings: &ProjectSettings) -> Result<(), CommandError> {
    if !settings.cut_plane_height.is_finite() || settings.cut_plane_height.abs() > MAX_CUT_PLANE_HEIGHT {
        return Err(CommandError::Invalid(
            Message::new("model-error-cut-plane-range").length("max", MAX_CUT_PLANE_HEIGHT),
        ));
    }
    let t = settings.tessellation;
    if !(t.chord_deviation > 0.0 && t.chord_deviation <= 1.0) || !(t.max_angle_deg > 0.0 && t.max_angle_deg <= 90.0) {
        return Err(invalid("model-error-tessellation-range"));
    }
    if settings.units.length_decimals > MAX_DECIMALS || settings.units.angle_decimals > MAX_DECIMALS {
        return Err(CommandError::Invalid(
            Message::new("model-error-display-precision").count("max", usize::from(MAX_DECIMALS)),
        ));
    }
    settings.plan_grid.check().map_err(CommandError::Invalid)?;
    Ok(())
}

/// A primitive, invertible change. Edits are produced by [`Command::plan`] and
/// applied by [`History`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Edit {
    InsertElement { index: usize, element: Element },
    RemoveElement { id: ElementId },
    ReplaceElement(Element),
    SetStoreys(StoreyList),
    InsertLayer { index: usize, layer: Layer },
    RemoveLayer { id: LayerId },
    ReplaceLayer(Layer),
    InsertMaterial { index: usize, material: Material },
    RemoveMaterial { id: MaterialId },
    ReplaceMaterial(Material),
    SetToolDefaults(ToolDefaults),
    SetSettings(ProjectSettings),
    SetInfo(ProjectInfo),
    SetSpatialNames { site: String, building: String },
    SetLocation(Location),
    SetCounters(NameCounters),
}

impl Edit {
    /// Applies the edit and returns its inverse.
    pub fn apply(self, project: &mut Project) -> Result<Edit, CommandError> {
        let inverse = match self {
            Edit::InsertElement { index, element } => {
                let id = element.id;
                let index = index.min(project.elements.len());
                project.elements.shift_insert(index, id, element);
                Edit::RemoveElement { id }
            }
            Edit::RemoveElement { id } => {
                let (index, _, element) = project
                    .elements
                    .shift_remove_full(&id)
                    .ok_or_else(|| not_found("model-error-element-not-found"))?;
                Edit::InsertElement { index, element }
            }
            Edit::ReplaceElement(element) => {
                let slot = project
                    .elements
                    .get_mut(&element.id)
                    .ok_or_else(|| not_found("model-error-element-not-found"))?;
                Edit::ReplaceElement(std::mem::replace(slot, element))
            }
            Edit::SetStoreys(list) => Edit::SetStoreys(std::mem::replace(&mut project.storeys, list)),
            Edit::InsertLayer { index, layer } => {
                let id = layer.id;
                let index = index.min(project.layers.len());
                project.layers.insert(index, layer);
                Edit::RemoveLayer { id }
            }
            Edit::RemoveLayer { id } => {
                let index = project
                    .layers
                    .iter()
                    .position(|l| l.id == id)
                    .ok_or_else(|| not_found("model-error-layer-not-found"))?;
                Edit::InsertLayer {
                    index,
                    layer: project.layers.remove(index),
                }
            }
            Edit::ReplaceLayer(layer) => {
                let slot = project
                    .layers
                    .iter_mut()
                    .find(|l| l.id == layer.id)
                    .ok_or_else(|| not_found("model-error-layer-not-found"))?;
                Edit::ReplaceLayer(std::mem::replace(slot, layer))
            }
            Edit::InsertMaterial { index, material } => {
                let id = material.id;
                let index = index.min(project.materials.len());
                project.materials.insert(index, material);
                Edit::RemoveMaterial { id }
            }
            Edit::RemoveMaterial { id } => {
                let index = project
                    .materials
                    .iter()
                    .position(|m| m.id == id)
                    .ok_or_else(|| not_found("model-error-material-not-found"))?;
                Edit::InsertMaterial {
                    index,
                    material: project.materials.remove(index),
                }
            }
            Edit::ReplaceMaterial(material) => {
                let slot = project
                    .materials
                    .iter_mut()
                    .find(|m| m.id == material.id)
                    .ok_or_else(|| not_found("model-error-material-not-found"))?;
                Edit::ReplaceMaterial(std::mem::replace(slot, material))
            }
            Edit::SetToolDefaults(defaults) => {
                Edit::SetToolDefaults(std::mem::replace(&mut project.tool_defaults, defaults))
            }
            Edit::SetSettings(settings) => Edit::SetSettings(std::mem::replace(&mut project.settings, settings)),
            Edit::SetInfo(info) => Edit::SetInfo(std::mem::replace(&mut project.info, info)),
            Edit::SetSpatialNames { site, building } => Edit::SetSpatialNames {
                site: std::mem::replace(&mut project.site.name, site),
                building: std::mem::replace(&mut project.building.name, building),
            },
            Edit::SetLocation(location) => Edit::SetLocation(std::mem::replace(&mut project.site.location, location)),
            Edit::SetCounters(counters) => Edit::SetCounters(std::mem::replace(&mut project.counters, counters)),
        };
        project.revision += 1;
        Ok(inverse)
    }
}

/// Applies edits in order. On failure, the edits applied so far are rolled
/// back and the project is unchanged.
fn apply_all(project: &mut Project, edits: Vec<Edit>) -> Result<Vec<Edit>, CommandError> {
    let mut inverses: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        match edit.apply(project) {
            Ok(inverse) => inverses.push(inverse),
            Err(e) => {
                for inverse in inverses.into_iter().rev() {
                    // An inverse of an edit that just succeeded always applies.
                    let _ = inverse.apply(project);
                }
                return Err(e);
            }
        }
    }
    inverses.reverse();
    Ok(inverses)
}

/// One executed command with the edits needed to undo and redo it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub serial: u64,
    pub command: Command,
    forward: Vec<Edit>,
    /// Inverse edits, already in the order they must be applied.
    inverse: Vec<Edit>,
}

/// A line of the command journal, for logs and crash reports: the command
/// label as a [`Message`], whose key form stays readable without the
/// catalogue.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JournalEntry {
    Executed(Message),
    Undone(Message),
    Redone(Message),
}

/// The undo/redo history of an open project (UND-001, UND-002).
#[derive(Debug)]
pub struct History {
    undo: VecDeque<Transaction>,
    redo: Vec<Transaction>,
    limit: usize,
    next_serial: u64,
    journal: VecDeque<JournalEntry>,
}

/// Default number of undo steps (Appendix D.1: at least 100).
pub const DEFAULT_UNDO_LIMIT: usize = 200;
const JOURNAL_LIMIT: usize = 200;

impl Default for History {
    fn default() -> Self {
        Self::new(DEFAULT_UNDO_LIMIT)
    }
}

impl History {
    pub fn new(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            next_serial: 1,
            journal: VecDeque::new(),
        }
    }

    /// Plans and applies a command. On error, the project is unchanged.
    pub fn execute(&mut self, project: &mut Project, command: Command) -> Result<(), CommandError> {
        let forward = command.plan(project)?;
        let inverse = apply_all(project, forward.clone())?;
        self.push_journal(JournalEntry::Executed(command.label()));
        let transaction = Transaction {
            serial: self.next_serial,
            command,
            forward,
            inverse,
        };
        self.next_serial += 1;
        self.undo.push_back(transaction);
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
        self.redo.clear();
        Ok(())
    }

    /// Undoes the last command and returns its label.
    pub fn undo(&mut self, project: &mut Project) -> Option<Message> {
        let transaction = self.undo.pop_back()?;
        match apply_all(project, transaction.inverse.clone()) {
            Ok(_) => {
                let label = transaction.command.label();
                self.push_journal(JournalEntry::Undone(label.clone()));
                self.redo.push(transaction);
                Some(label)
            }
            Err(_) => {
                // The history no longer matches the project; drop it rather
                // than corrupting the model.
                self.undo.clear();
                self.redo.clear();
                None
            }
        }
    }

    /// Redoes the last undone command and returns its label.
    pub fn redo(&mut self, project: &mut Project) -> Option<Message> {
        let transaction = self.redo.pop()?;
        match apply_all(project, transaction.forward.clone()) {
            Ok(_) => {
                let label = transaction.command.label();
                self.push_journal(JournalEntry::Redone(label.clone()));
                self.undo.push_back(transaction);
                Some(label)
            }
            Err(_) => {
                self.redo.clear();
                None
            }
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<Message> {
        self.undo.back().map(|t| t.command.label())
    }

    pub fn redo_label(&self) -> Option<Message> {
        self.redo.last().map(|t| t.command.label())
    }

    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Identifies the current model state: the serial of the last applied
    /// command, or 0. Comparing it with the value at the last save tells
    /// whether there are unsaved changes (PRJ-003).
    pub fn state_id(&self) -> u64 {
        self.undo.back().map_or(0, |t| t.serial)
    }

    /// The commands that produced the current state, oldest first. Together
    /// with the starting project they replay the session.
    pub fn applied_commands(&self) -> impl Iterator<Item = &Command> {
        self.undo.iter().map(|t| &t.command)
    }

    /// Recent executed, undone and redone commands, oldest first.
    pub fn journal(&self) -> impl Iterator<Item = &JournalEntry> {
        self.journal.iter()
    }

    fn push_journal(&mut self, entry: JournalEntry) {
        self.journal.push_back(entry);
        while self.journal.len() > JOURNAL_LIMIT {
            self.journal.pop_front();
        }
    }
}

/// Executes commands in order on a project, as a script would (NFR-MNT-001).
pub fn replay<'a>(
    project: &mut Project,
    commands: impl IntoIterator<Item = &'a Command>,
) -> Result<History, CommandError> {
    let mut history = History::default();
    for command in commands {
        history.execute(project, command.clone())?;
    }
    Ok(history)
}
