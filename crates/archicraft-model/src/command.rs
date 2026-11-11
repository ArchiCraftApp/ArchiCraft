//! Command objects and the undo history (NFR-MNT-001, UND-001, UND-002,
//! UND-005, STR-005, ELM-015, PRF-004, SRS 3.3).
//!
//! Every change to a project is a [`Command`]. Executing a command first plans
//! it against the current project, which validates it and produces a list of
//! primitive [`Edit`]s, and then applies those edits. Applying an edit returns
//! its inverse, so undo applies the inverses in reverse order and redo applies
//! the recorded forward edits again. Redo therefore never re-plans a command
//! and identities stay stable. Commands are serialisable, so they can be
//! logged and replayed.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use glam::DVec2;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::changes::ChangeLog;
use crate::element::{Element, KindTag};
use crate::group::Group;
use crate::ids::{ElementId, GroupId, LayerId, MaterialId, ProfileId, RelationId, StoreyId};
use crate::layer::Layer;
use crate::material::Material;
use crate::message::Message;
use crate::profile::Profile;
use crate::project::{DefaultsSlot, Location, NameCounters, Project, ProjectInfo, ProjectSettings, ToolDefaults};
use crate::relations::Relation;
use crate::storey::{MAX_CUT_PLANE_HEIGHT, Storey, StoreyList};
use crate::tolerance::near_zero;
use crate::transfer::{SettingsSource, transfer_settings};
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
    /// Adds a profile (PRF-004), stored as
    /// [`validate::validate_profile`] normalises it.
    AddProfile(Profile),
    /// Replaces the profile with this identity (PRF-004). Every element and
    /// tool-defaults slot that uses it must stay valid with the new profile,
    /// so an edit cannot remove a usage flag a user needs.
    UpdateProfile(Profile),
    /// Deletes a profile (PRF-004, DSP-003). Its users are re-pointed to
    /// `replacement` through [`crate::kinds::Kind::replace_profile`]; a
    /// profile in use cannot be deleted without one.
    DeleteProfile {
        profile: ProfileId,
        replacement: Option<ProfileId>,
    },
    SetToolDefaults(ToolDefaults),
    SetSettings(ProjectSettings),
    SetInfo(ProjectInfo),
    SetSpatialNames {
        site: String,
        building: String,
    },
    SetLocation(Location),
    /// Groups elements in a new group with the identity `group` (ELM-015).
    /// An element that already belongs to a group joins through its
    /// outermost group, which becomes a child of the new one, so groups
    /// nest. The new group is named `Group <n>` from the project's group
    /// counter. Fewer than two distinct elements and groups change nothing.
    Group {
        group: GroupId,
        elements: Vec<ElementId>,
    },
    /// Removes one level of grouping (ELM-015): the group's child groups and
    /// its own members move to its parent, or out of every group if it is
    /// an outermost group.
    Ungroup {
        group: GroupId,
    },
    RenameGroup {
        group: GroupId,
        name: String,
    },
    /// Records an explicit relation between two elements (SRS 3.3), once
    /// [`check_relation`] accepts it. A derived storey link follows from the
    /// elements and is refused.
    InsertRelation(Relation),
    /// Removes the explicit relation with this identity (SRS 3.3). A derived
    /// storey link is refused.
    RemoveRelation(RelationId),
    /// Copies the compatible settings of `source` into each of `targets`
    /// (ELM-003, [`transfer_settings`]), as one undo step. Locked targets
    /// are refused (ELM-013) and the results are validated as by
    /// [`Command::UpdateElements`]; targets that already have the source's
    /// settings change nothing, and the command is refused when no target
    /// accepts any field of the source.
    InjectSettings {
        source: SettingsSource,
        targets: Vec<ElementId>,
    },
    /// Copies the compatible settings of `element` into the defaults slot
    /// of the tool for `into` (ELM-003, ELM-001), as
    /// [`Command::SetToolDefaults`] would: every transferable field from an
    /// element of that kind, the compatible ones from another kind. The
    /// slot's layer follows the element.
    PickUpSettings {
        element: ElementId,
        into: KindTag,
    },
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
            Command::AddProfile(_) => Message::new("model-label-new-profile"),
            Command::UpdateProfile(_) => Message::new("model-label-change-profile"),
            Command::DeleteProfile { .. } => Message::new("model-label-delete-profile"),
            Command::SetToolDefaults(_) => Message::new("model-label-tool-settings"),
            Command::SetSettings(_) => Message::new("model-label-project-settings"),
            Command::SetInfo(_) => Message::new("model-label-project-info"),
            Command::SetSpatialNames { .. } => Message::new("model-label-spatial-names"),
            Command::SetLocation(_) => Message::new("model-label-location"),
            Command::Group { .. } => Message::new("model-label-group"),
            Command::Ungroup { .. } => Message::new("model-label-ungroup"),
            Command::RenameGroup { .. } => Message::new("model-label-rename-group"),
            Command::InsertRelation(_) => Message::new("model-label-insert-relation"),
            Command::RemoveRelation(_) => Message::new("model-label-remove-relation"),
            Command::InjectSettings { targets, .. } => {
                let distinct: HashSet<ElementId> = targets.iter().copied().collect();
                elements("model-label-inject-settings", distinct.len())
            }
            Command::PickUpSettings { .. } => Message::new("model-label-pick-up-settings"),
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
            Command::AddProfile(profile) => {
                let profile = validate::validate_profile(project, profile).map_err(CommandError::Invalid)?;
                if project.profile(profile.id).is_some() {
                    return Err(invalid("model-error-duplicate-profile-id"));
                }
                Ok(vec![Edit::InsertProfile {
                    index: project.profiles.len(),
                    profile,
                }])
            }
            Command::UpdateProfile(profile) => plan_update_profile(project, profile),
            Command::DeleteProfile { profile, replacement } => plan_delete_profile(project, *profile, *replacement),
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
            Command::Group { group, elements } => plan_group(project, *group, elements),
            Command::Ungroup { group } => plan_ungroup(project, *group),
            Command::RenameGroup { group, name } => {
                let old = find_group(project, *group)?;
                let name = name.trim();
                if name.is_empty() {
                    return Err(invalid("model-error-group-name"));
                }
                if old.name == name {
                    return Err(CommandError::NoChange);
                }
                Ok(vec![Edit::ReplaceGroup(Group {
                    name: name.to_owned(),
                    ..old.clone()
                })])
            }
            Command::InsertRelation(relation) => {
                check_relation(project, relation)?;
                Ok(vec![Edit::InsertRelation(*relation)])
            }
            Command::RemoveRelation(id) => {
                if project.relations.contains(*id) {
                    Ok(vec![Edit::RemoveRelation(*id)])
                } else if project
                    .elements
                    .keys()
                    .flat_map(|e| project.relations.relations_of(*e))
                    .any(|r| r.is_derived() && r.id() == *id)
                {
                    Err(invalid("model-error-relation-derived"))
                } else {
                    Err(not_found("model-error-relation-not-found"))
                }
            }
            Command::InjectSettings { source, targets } => plan_inject_settings(project, *source, targets),
            Command::PickUpSettings { element, into } => plan_pick_up_settings(project, *element, *into),
        }
    }
}

fn find_element(project: &Project, id: ElementId) -> Result<&Element, CommandError> {
    project
        .element(id)
        .ok_or_else(|| not_found("model-error-element-not-found"))
}

fn find_group(project: &Project, id: GroupId) -> Result<&Group, CommandError> {
    project
        .group(id)
        .ok_or_else(|| not_found("model-error-group-not-found"))
}

/// Checks that `relation` can be recorded (SRS 3.3, NFR-REL-005): it is an
/// explicit relation, both ends are elements of the project and they differ,
/// it is not recorded yet (a junction neither the other way round, as it is
/// symmetric), and a host relation gives its hosted element its only host
/// without closing a cycle of hosts, so that hosted elements nest under
/// their hosts. Every refusal is [`CommandError::Invalid`] naming the kind
/// of relation and, where it exists, the element.
fn check_relation(project: &Project, relation: &Relation) -> Result<(), CommandError> {
    let Some((first, second)) = relation.ends() else {
        return Err(invalid("model-error-relation-derived"));
    };
    let kind = || Message::new(relation.label_key());
    let (Some(dependent), Some(_)) = (project.element(first), project.element(second)) else {
        return Err(CommandError::Invalid(
            Message::new("model-error-relation-end-missing").nested("relation", kind()),
        ));
    };
    let named = |key| Message::new(key).text("name", &dependent.name);
    let refuse = |key| CommandError::Invalid(named(key).nested("relation", kind()));
    if first == second {
        return Err(refuse("model-error-relation-self"));
    }
    let reversed = match *relation {
        Relation::Junction { a, b } => Some(Relation::Junction { a: b, b: a }),
        _ => None,
    };
    if std::iter::once(*relation)
        .chain(reversed)
        .any(|r| project.relations.contains(r.id()))
    {
        return Err(refuse("model-error-relation-duplicate"));
    }
    if let Relation::Host { hosted, host } = *relation {
        if project.relations.host_of(hosted).is_some() {
            return Err(CommandError::Invalid(named("model-error-host-twice")));
        }
        // With one host per element the hosts above `host` form a chain; it
        // ends at an element without a host, or at `hosted` for a cycle. A
        // file's own cycle, not repaired at load (NFR-REL-004), ends the
        // walk when it comes round.
        let mut seen = HashSet::new();
        let mut above = Some(host);
        while let Some(element) = above {
            if element == hosted {
                return Err(CommandError::Invalid(named("model-error-host-cycle")));
            }
            if !seen.insert(element) {
                break;
            }
            above = project.relations.host_of(element);
        }
    }
    Ok(())
}

/// An [`Edit::RemoveRelation`] for every explicit relation of an element
/// that `edits` remove (SRS 3.3), each once, in identity order. They go
/// before the element removals, so that undo puts the elements back before
/// their relations and no relation is left dangling.
fn remove_relations_of(project: &Project, edits: &[Edit]) -> Vec<Edit> {
    let relations: BTreeSet<RelationId> = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveElement { id } => Some(*id),
            _ => None,
        })
        .flat_map(|id| project.relations.ids_touching(id))
        .collect();
    relations.into_iter().map(Edit::RemoveRelation).collect()
}

fn storey_edit(project: &Project, list: StoreyList) -> Result<Vec<Edit>, CommandError> {
    if list == project.storeys {
        return Err(CommandError::NoChange);
    }
    check_linked_heights(project, &list, &[])?;
    Ok(vec![Edit::SetStoreys(list)])
}

/// Refuses a new storey list that would leave an element whose extent
/// depends on the storeys lower than [`validate::MIN_HEIGHT`] or higher
/// than [`validate::MAX_HEIGHT`] (NFR-REL-005, ELM-017, STR-002).
/// `element_edits` are the element changes planned with the list (removals
/// and replacements). Only elements the edit lowers below the minimum, or
/// raises above the maximum, count, beyond the rounding of their elevations
/// ([`near_zero`]): a fixed-height element on a moved storey keeps its
/// height, and an element already too low or too high (made so outside the
/// commands, or, for too high, added with a linked extent above the
/// maximum, which `validate_element` does not check) does not block storey
/// edits that leave it as it is or move it towards the valid range. The
/// message names the first element and counts the others.
fn check_linked_heights(project: &Project, list: &StoreyList, element_edits: &[Edit]) -> Result<(), CommandError> {
    let planned: HashMap<ElementId, Option<&Element>> = element_edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveElement { id } => Some((*id, None)),
            Edit::ReplaceElement(new) => Some((new.id, Some(new))),
            _ => None,
        })
        .collect();
    // Each element's height before and after the edit, when the edit
    // changes it beyond rounding; `None` before when it had no extent.
    let changed: Vec<(&Element, Option<f64>, f64)> = project
        .elements
        .values()
        .filter_map(|old| {
            let new = planned.get(&old.id).copied().unwrap_or(Some(old))?;
            let (bottom, top) = new.kind.vertical_extent(new.storey, list)?;
            let after = top - bottom;
            let Some((old_bottom, old_top)) = project.vertical_extent(old) else {
                return Some((old, None, after));
            };
            let before = old_top - old_bottom;
            let scale = [bottom, top, old_bottom, old_top]
                .map(f64::abs)
                .into_iter()
                .fold(0.0, f64::max);
            (!near_zero(before - after, scale)).then_some((old, Some(before), after))
        })
        .collect();
    let refuse = |key: &'static str, offends: &dyn Fn(Option<f64>, f64) -> bool, limit: (&'static str, f64)| {
        let mut offending = changed.iter().filter(|(_, before, after)| offends(*before, *after));
        match offending.next() {
            None => Ok(()),
            Some((first, _, _)) => Err(CommandError::Invalid(
                Message::new(key)
                    .text("name", &first.name)
                    .count("count", offending.count())
                    .length(limit.0, limit.1),
            )),
        }
    };
    refuse(
        "model-error-linked-element-too-low",
        &|before, after| after < validate::MIN_HEIGHT && before.is_none_or(|before| after < before),
        ("min", validate::MIN_HEIGHT),
    )?;
    refuse(
        "model-error-linked-element-too-high",
        &|before, after| after > validate::MAX_HEIGHT && before.is_none_or(|before| after > before),
        ("max", validate::MAX_HEIGHT),
    )
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
        if element.group.is_some_and(|g| project.group(g).is_none()) {
            return Err(not_found("model-error-group-not-found"));
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

/// Removes elements with their explicit relations (SRS 3.3) and the groups
/// they leave empty (ELM-015).
fn plan_delete_elements(project: &Project, ids: &[ElementId]) -> Result<Vec<Edit>, CommandError> {
    if ids.is_empty() {
        return Err(CommandError::NoChange);
    }
    let mut removals = Vec::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            continue;
        }
        let element = find_element(project, *id)?;
        if let Some(reason) = project.edit_lock_reason(element) {
            return Err(CommandError::Locked(reason));
        }
        removals.push(Edit::RemoveElement { id: *id });
    }
    let mut edits = remove_relations_of(project, &removals);
    edits.extend(removals);
    edits.extend(remove_emptied_groups(project, &edits));
    Ok(edits)
}

/// Removes the groups that the element removals among `edits` leave without
/// members, nested groups included (ELM-015). A group that was empty before
/// stays.
fn remove_emptied_groups(project: &Project, edits: &[Edit]) -> Vec<Edit> {
    let removed: HashSet<ElementId> = edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveElement { id } => Some(*id),
            _ => None,
        })
        .collect();
    let mut emptied = HashSet::new();
    let mut kept = HashSet::new();
    for e in project.elements.values() {
        let groups = if removed.contains(&e.id) {
            &mut emptied
        } else {
            &mut kept
        };
        groups.extend(project.group_path(e.id));
    }
    project
        .groups
        .keys()
        .filter(|g| emptied.contains(*g) && !kept.contains(*g))
        .map(|&id| Edit::RemoveGroup { id })
        .collect()
}

/// Plans [`Command::Group`]: each element stands for its outermost group if
/// it has one, and for itself otherwise (ELM-015). Locks do not matter:
/// grouping edits neither geometry nor parameters (ELM-013).
fn plan_group(project: &Project, group: GroupId, elements: &[ElementId]) -> Result<Vec<Edit>, CommandError> {
    if project.group(group).is_some() {
        return Err(invalid("model-error-duplicate-group-id"));
    }
    let mut seen_groups = HashSet::new();
    let mut seen_elements = HashSet::new();
    let (mut groups, mut loose) = (Vec::new(), Vec::new());
    for id in elements {
        let element = find_element(project, *id)?;
        match project.outermost_group(*id) {
            Some(g) if seen_groups.insert(g) => groups.push(find_group(project, g)?),
            Some(_) => {}
            None if seen_elements.insert(*id) => loose.push(element),
            None => {}
        }
    }
    if groups.len() + loose.len() < 2 {
        return Err(CommandError::NoChange);
    }
    let n = project.group_counter.saturating_add(1);
    let mut edits = vec![
        Edit::InsertGroup {
            index: project.groups.len(),
            group: Group {
                id: group,
                name: Group::automatic_name(n),
                parent: None,
            },
        },
        Edit::SetGroupCounter(n),
    ];
    edits.extend(groups.into_iter().map(|g| {
        Edit::ReplaceGroup(Group {
            parent: Some(group),
            ..g.clone()
        })
    }));
    edits.extend(loose.into_iter().map(|e| {
        Edit::ReplaceElement(Element {
            group: Some(group),
            ..e.clone()
        })
    }));
    Ok(edits)
}

/// Plans [`Command::Ungroup`] (ELM-015). Like grouping, it ignores locks.
fn plan_ungroup(project: &Project, group: GroupId) -> Result<Vec<Edit>, CommandError> {
    let parent = find_group(project, group)?.parent;
    let mut edits: Vec<Edit> = project
        .groups
        .values()
        .filter(|g| g.parent == Some(group))
        .map(|g| Edit::ReplaceGroup(Group { parent, ..g.clone() }))
        .collect();
    edits.extend(project.elements.values().filter(|e| e.group == Some(group)).map(|e| {
        Edit::ReplaceElement(Element {
            group: parent,
            ..e.clone()
        })
    }));
    edits.push(Edit::RemoveGroup { id: group });
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
        // ELM-015: membership changes only through the group commands, which
        // keep every group with members.
        if old.group != element.group {
            return Err(invalid("model-error-group-change"));
        }
        if let Some(reason) = project.edit_lock_reason(old) {
            let layer_locked = project.layer(old.layer).is_some_and(|l| l.locked);
            if layer_locked || !only_lock_or_snap_changed(old, element) {
                return Err(CommandError::Locked(reason));
            }
        }
        let element = validate::validate_element(project, element).map_err(|e| element_invalid(element, e))?;
        edits.push(Edit::ReplaceElement(element));
    }
    if edits.is_empty() {
        return Err(CommandError::NoChange);
    }
    Ok(edits)
}

/// The element settings come from (ELM-003): a placed element, or the
/// prototype of a tool's defaults on the ground storey (ELM-001).
fn settings_source(project: &Project, source: SettingsSource) -> Result<Element, CommandError> {
    match source {
        SettingsSource::Element(id) => find_element(project, id).cloned(),
        SettingsSource::Defaults(kind) => project
            .new_element(kind, project.storeys.ground(), 0)
            .ok_or_else(|| CommandError::Invalid(Message::new("model-error-default-missing").kind("kind", kind))),
    }
}

/// Plans [`Command::InjectSettings`] through [`plan_update_elements`], which
/// checks locks (ELM-013) and validates, and reports a command that changes
/// nothing as [`CommandError::NoChange`] (ELM-003).
/// Every target takes at least its envelope fields (layer, position), so
/// the command is refused as unsupported only when no target passes its
/// validator with any of the source's settings, such as a target linked to
/// a storey that is gone.
fn plan_inject_settings(
    project: &Project,
    source: SettingsSource,
    targets: &[ElementId],
) -> Result<Vec<Edit>, CommandError> {
    let source = settings_source(project, source)?;
    let mut seen = HashSet::new();
    let mut updated = Vec::with_capacity(targets.len());
    let mut accepted = false;
    for id in targets {
        if !seen.insert(*id) {
            continue;
        }
        let mut target = find_element(project, *id)?.clone();
        accepted |= !transfer_settings(&source, &mut target, project).applied.is_empty();
        updated.push(target);
    }
    if updated.is_empty() {
        return Err(CommandError::NoChange);
    }
    if !accepted {
        return Err(invalid("model-error-transfer-unsupported"));
    }
    plan_update_elements(project, &updated)
}

/// Plans [`Command::PickUpSettings`] as [`Command::SetToolDefaults`] on the
/// slot of `into` (ELM-003, ELM-001). The slot's prototype is made on the
/// element's storey, so that storey links resolve as they do for the
/// element, and stored on the ground storey, like the properties panel
/// stores an edited prototype.
fn plan_pick_up_settings(project: &Project, element: ElementId, into: KindTag) -> Result<Vec<Edit>, CommandError> {
    let source = find_element(project, element)?;
    let before = project
        .new_element(into, source.storey, 0)
        .ok_or_else(|| CommandError::Invalid(Message::new("model-error-default-missing").kind("kind", into)))?;
    let mut prototype = before.clone();
    if transfer_settings(source, &mut prototype, project).applied.is_empty() {
        return Err(invalid("model-error-transfer-unsupported"));
    }
    if prototype == before {
        return Err(CommandError::NoChange);
    }
    let mut kind = prototype.kind;
    kind.place_on(project.storeys.ground());
    let mut defaults = project.tool_defaults.clone();
    defaults.insert(DefaultsSlot {
        layer: prototype.layer,
        position: prototype.position,
        structural: prototype.structural,
        ifc_class: prototype.ifc_class,
        predefined_type: prototype.predefined_type,
        kind,
    });
    Command::SetToolDefaults(defaults).plan(project)
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
    moved
        .kind
        .rehome(element.storey, &project.storeys, target, target_elevation)?;
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
    // SRS 3.3: the deleted elements' relations go first, as for
    // `plan_delete_elements`.
    let mut planned = remove_relations_of(project, &edits);
    planned.extend(edits);
    planned.extend(remove_emptied_groups(project, &planned));
    planned.push(Edit::SetStoreys(list));
    Ok(planned)
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
        .filter(|e| e.kind.material() == Some(material))
        .map(|e| {
            let mut changed = e.clone();
            changed.kind.set_material(replacement);
            Edit::ReplaceElement(changed)
        })
        .collect();
    // PRF-004: the parametric shapes and regions of profiles made of it.
    for profile in &project.profiles {
        let mut changed = profile.clone();
        if changed.replace_material(material, replacement) {
            edits.push(Edit::ReplaceProfile(changed));
        }
    }
    let mut defaults = project.tool_defaults.clone();
    for slot in defaults.slots_mut().filter(|s| s.kind.material() == Some(material)) {
        slot.kind.set_material(replacement);
    }
    if defaults != project.tool_defaults {
        edits.push(Edit::SetToolDefaults(defaults));
    }
    edits.push(Edit::RemoveMaterial { id: material });
    Ok(edits)
}

/// The edit of [`Command::UpdateProfile`] (PRF-004). Editing a profile
/// updates every element that uses it, so each user and each tool-defaults
/// slot that refers to it is validated against the project with the new
/// profile, which rejects, for example, removing a usage flag a user needs.
fn plan_update_profile(project: &Project, profile: &Profile) -> Result<Vec<Edit>, CommandError> {
    let profile = validate::validate_profile(project, profile).map_err(CommandError::Invalid)?;
    match project.profile(profile.id) {
        None => return Err(not_found("model-error-profile-not-found")),
        Some(old) if *old == profile => return Err(CommandError::NoChange),
        Some(_) => {}
    }
    let id = profile.id;
    let slot_uses = |slot: &DefaultsSlot| slot.kind.profiles().contains(&id);
    if project.profile_users(id).next().is_some() || project.tool_defaults.iter().any(|(_, s)| slot_uses(s)) {
        let mut next = project.clone();
        if let Some(slot) = next.profiles.iter_mut().find(|p| p.id == id) {
            *slot = profile.clone();
        }
        for user in project.profile_users(id) {
            validate::validate_element(&next, user).map_err(|e| element_invalid(user, e))?;
        }
        for (kind, slot) in project.tool_defaults.iter().filter(|(_, s)| slot_uses(s)) {
            check_slot(&next, kind, slot)?;
        }
    }
    Ok(vec![Edit::ReplaceProfile(profile)])
}

/// The edits of [`Command::DeleteProfile`] (PRF-004, DSP-003). Every
/// element and tool-defaults slot that uses the profile is re-pointed to the
/// replacement through [`crate::kinds::Kind::replace_profile`] and must stay
/// valid with it, so a replacement without a usage flag a user needs is
/// rejected. A profile in use needs a replacement.
fn plan_delete_profile(
    project: &Project,
    profile: ProfileId,
    replacement: Option<ProfileId>,
) -> Result<Vec<Edit>, CommandError> {
    let Some(deleted) = project.profile(profile) else {
        return Err(not_found("model-error-profile-not-found"));
    };
    let users: Vec<&Element> = project.profile_users(profile).collect();
    let mut defaults = project.tool_defaults.clone();
    let mut edits = Vec::new();
    match replacement {
        None => {
            if !users.is_empty() || defaults.iter().any(|(_, s)| s.kind.profiles().contains(&profile)) {
                return Err(CommandError::Invalid(
                    Message::new("model-error-profile-in-use")
                        .text("name", &deleted.name)
                        .count("count", users.len()),
                ));
            }
        }
        Some(replacement) => {
            if replacement == profile || project.profile(replacement).is_none() {
                return Err(invalid("model-error-replacement-profile"));
            }
            for user in users {
                let mut changed = user.clone();
                changed.kind.replace_profile(profile, replacement);
                let changed = validate::validate_element(project, &changed).map_err(|e| element_invalid(user, e))?;
                edits.push(Edit::ReplaceElement(changed));
            }
            for slot in defaults.slots_mut() {
                if slot.kind.replace_profile(profile, replacement) {
                    check_slot(project, slot.tag(), slot)?;
                }
            }
            if defaults != project.tool_defaults {
                edits.push(Edit::SetToolDefaults(defaults));
            }
        }
    }
    edits.push(Edit::RemoveProfile { id: profile });
    Ok(edits)
}

/// The error for `element`, which `reason` makes invalid.
fn element_invalid(element: &Element, reason: Message) -> CommandError {
    CommandError::Invalid(
        Message::new("model-error-element-invalid")
            .text("name", &element.name)
            .nested("reason", reason),
    )
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
    let mut payload = slot.kind.clone();
    // A 2D element's view in the slot is a placeholder (ANN-002).
    payload.place_on(project.storeys.ground());
    let prototype = Element {
        id: ElementId::new(),
        name: project.next_element_name(kind, 0),
        storey: project.storeys.ground(),
        layer: slot.layer,
        ifc_class: slot.ifc_class,
        predefined_type: slot.predefined_type,
        position: slot.position,
        structural: slot.structural,
        locked: false,
        snap_excluded: false,
        group: None,
        kind: payload,
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
/// applied by [`History`]. `InsertRelation` and `RemoveRelation` record and
/// remove an explicit relation (SRS 3.3), each the other's inverse.
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
    InsertProfile { index: usize, profile: Profile },
    RemoveProfile { id: ProfileId },
    ReplaceProfile(Profile),
    SetToolDefaults(ToolDefaults),
    SetSettings(ProjectSettings),
    SetInfo(ProjectInfo),
    SetSpatialNames { site: String, building: String },
    SetLocation(Location),
    SetCounters(NameCounters),
    InsertGroup { index: usize, group: Group },
    RemoveGroup { id: GroupId },
    ReplaceGroup(Group),
    SetGroupCounter(u32),
    InsertRelation(Relation),
    RemoveRelation(RelationId),
}

impl Edit {
    /// Applies the edit and returns its inverse. An edit that inserts,
    /// removes or replaces an element updates the relation store's index
    /// for that element (STR-006), so that the index stays exact through
    /// undo and redo. Every edit advances the project's revision and stamps
    /// what it changed in the project's change log with the revision after
    /// it (NFR-PERF-006), forward, in undo and in redo alike.
    pub fn apply(self, project: &mut Project) -> Result<Edit, CommandError> {
        let inverse = match self {
            Edit::InsertElement { index, element } => {
                let id = element.id;
                let index = index.min(project.elements.len());
                project.elements.shift_insert(index, id, element);
                project.relations.update_element(id, project.elements.get(&id));
                Edit::RemoveElement { id }
            }
            Edit::RemoveElement { id } => {
                let (index, _, element) = project
                    .elements
                    .shift_remove_full(&id)
                    .ok_or_else(|| not_found("model-error-element-not-found"))?;
                project.relations.update_element(id, None);
                Edit::InsertElement { index, element }
            }
            Edit::ReplaceElement(element) => {
                let id = element.id;
                let slot = project
                    .elements
                    .get_mut(&id)
                    .ok_or_else(|| not_found("model-error-element-not-found"))?;
                let old = std::mem::replace(slot, element);
                project.relations.update_element(id, Some(slot));
                Edit::ReplaceElement(old)
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
            Edit::InsertProfile { index, profile } => {
                let id = profile.id;
                let index = index.min(project.profiles.len());
                project.profiles.insert(index, profile);
                Edit::RemoveProfile { id }
            }
            Edit::RemoveProfile { id } => {
                let index = project
                    .profiles
                    .iter()
                    .position(|p| p.id == id)
                    .ok_or_else(|| not_found("model-error-profile-not-found"))?;
                Edit::InsertProfile {
                    index,
                    profile: project.profiles.remove(index),
                }
            }
            Edit::ReplaceProfile(profile) => {
                let slot = project
                    .profiles
                    .iter_mut()
                    .find(|p| p.id == profile.id)
                    .ok_or_else(|| not_found("model-error-profile-not-found"))?;
                Edit::ReplaceProfile(std::mem::replace(slot, profile))
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
            Edit::InsertGroup { index, group } => {
                let id = group.id;
                let index = index.min(project.groups.len());
                project.groups.shift_insert(index, id, group);
                Edit::RemoveGroup { id }
            }
            Edit::RemoveGroup { id } => {
                let (index, _, group) = project
                    .groups
                    .shift_remove_full(&id)
                    .ok_or_else(|| not_found("model-error-group-not-found"))?;
                Edit::InsertGroup { index, group }
            }
            Edit::ReplaceGroup(group) => {
                let slot = project
                    .groups
                    .get_mut(&group.id)
                    .ok_or_else(|| not_found("model-error-group-not-found"))?;
                Edit::ReplaceGroup(std::mem::replace(slot, group))
            }
            Edit::SetGroupCounter(counter) => {
                Edit::SetGroupCounter(std::mem::replace(&mut project.group_counter, counter))
            }
            Edit::InsertRelation(relation) => {
                if relation.is_derived() {
                    return Err(invalid("model-error-relation-derived"));
                }
                if !project.relations.insert(relation) {
                    return Err(invalid("model-error-relation-duplicate"));
                }
                Edit::RemoveRelation(relation.id())
            }
            Edit::RemoveRelation(id) => Edit::InsertRelation(
                project
                    .relations
                    .remove(id)
                    .ok_or_else(|| not_found("model-error-relation-not-found"))?,
            ),
        };
        project.revision += 1;
        stamp_changes(project, &inverse);
        Ok(inverse)
    }
}

/// Stamps in the project's change log what the edit undone by `inverse`
/// changed, with the project's revision after it (NFR-PERF-006). The
/// inverse names what changed: an inserted element is undone by removing
/// it, a removed one by inserting it again, a replaced one by its old
/// version, moved storeys by the old storey list. A relation decides what
/// its ends depend on (SRS 3.3), so recording or removing one stamps the
/// ends that are in the project. The settings hold the tessellation
/// tolerances every body depends on (IFX-034): everything is stamped. An
/// element's body depends on the profiles it uses (PRF-004), so inserting,
/// replacing or removing a profile stamps the elements whose
/// [`crate::kinds::Kind::profiles`] name it. The match names every edit, so
/// that a new one decides what it stamps.
fn stamp_changes(project: &mut Project, inverse: &Edit) {
    let Project {
        changes,
        storeys,
        elements,
        relations,
        revision,
        ..
    } = project;
    let revision = *revision;
    let mut stamp_ends = |relation: Option<&Relation>| {
        let Some((first, second)) = relation.and_then(Relation::ends) else {
            return;
        };
        for end in [first, second] {
            if elements.contains_key(&end) {
                changes.stamp_element(end, revision, false);
            }
        }
    };
    match inverse {
        Edit::RemoveElement { id } => changes.stamp_element(*id, revision, false),
        Edit::ReplaceElement(old) => changes.stamp_element(old.id, revision, false),
        Edit::InsertElement { element, .. } => changes.stamp_element(element.id, revision, true),
        Edit::SetStoreys(old) => changes.stamp_storeys(old, storeys, revision),
        Edit::SetSettings(_) => changes.stamp_everything(revision),
        Edit::RemoveRelation(id) => stamp_ends(relations.get(*id)),
        Edit::InsertRelation(relation) => stamp_ends(Some(relation)),
        Edit::InsertProfile { profile, .. } => stamp_profile_users(changes, elements, profile.id, revision),
        Edit::RemoveProfile { id } => stamp_profile_users(changes, elements, *id, revision),
        Edit::ReplaceProfile(old) => stamp_profile_users(changes, elements, old.id, revision),
        Edit::InsertLayer { .. }
        | Edit::RemoveLayer { .. }
        | Edit::ReplaceLayer(_)
        | Edit::InsertMaterial { .. }
        | Edit::RemoveMaterial { .. }
        | Edit::ReplaceMaterial(_)
        | Edit::SetToolDefaults(_)
        | Edit::SetInfo(_)
        | Edit::SetSpatialNames { .. }
        | Edit::SetLocation(_)
        | Edit::SetCounters(_)
        | Edit::InsertGroup { .. }
        | Edit::RemoveGroup { .. }
        | Edit::ReplaceGroup(_)
        | Edit::SetGroupCounter(_) => {}
    }
}

/// Stamps the elements that use `profile` (PRF-004), see [`stamp_changes`].
fn stamp_profile_users(
    changes: &mut ChangeLog,
    elements: &IndexMap<ElementId, Element>,
    profile: ProfileId,
    revision: u64,
) {
    for element in elements.values().filter(|e| e.kind.profiles().contains(&profile)) {
        changes.stamp_element(element.id, revision, false);
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

impl Transaction {
    /// The edits that carried the command out, in the order they were
    /// applied; redo applies them again.
    pub fn forward(&self) -> &[Edit] {
        &self.forward
    }

    /// The edits that undo the command, in the order undo applies them.
    pub fn inverse(&self) -> &[Edit] {
        &self.inverse
    }
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

    /// The transactions that produced the current state, oldest first, with
    /// their edits.
    pub fn transactions(&self) -> impl Iterator<Item = &Transaction> {
        self.undo.iter()
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
