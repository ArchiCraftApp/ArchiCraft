//! Element groups (ELM-015).
//!
//! An element names the innermost group it belongs to ([`Element::group`]),
//! and a group names the group it is nested in ([`Group::parent`]). Keeping
//! membership on the element and the parent link on the group makes nesting
//! a tree and answers "which group?" in O(depth). The commands keep the tree
//! acyclic and every reference valid; the IFCX reader repairs files that
//! break it (PRJ-008).
//!
//! [`Element::group`]: crate::Element::group

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::ids::{ElementId, GroupId};
use crate::project::Project;

/// A named group of elements and groups (ELM-015).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Group {
    pub id: GroupId,
    /// User-visible name, `Group 1` and so on unless renamed.
    pub name: String,
    /// The group this one is nested in; `None` for an outermost group.
    pub parent: Option<GroupId>,
}

impl Group {
    /// Automatic name of the `n`th group of a project, for example `Group 3`.
    pub fn automatic_name(n: u32) -> String {
        format!("Group {n}")
    }
}

impl Project {
    pub fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.get(&id)
    }

    /// `group` and the groups it is nested in, innermost first. Stops at a
    /// group that does not exist, and after as many steps as there are
    /// groups, so that even a parent cycle, which commands never create,
    /// ends.
    fn enclosing_groups(&self, group: Option<GroupId>) -> impl Iterator<Item = GroupId> + '_ {
        let mut next = group;
        (0..self.groups.len()).map_while(move |_| {
            let group = self.group(next?)?;
            next = group.parent;
            Some(group.id)
        })
    }

    /// The groups of an element from its outermost group down to its own
    /// (innermost) one; empty for an ungrouped or unknown element.
    pub fn group_path(&self, element: ElementId) -> Vec<GroupId> {
        let mut path: Vec<GroupId> = self
            .enclosing_groups(self.element(element).and_then(|e| e.group))
            .collect();
        path.reverse();
        path
    }

    /// The outermost group of an element, which a click on the element
    /// selects (ELM-015); `None` for an ungrouped or unknown element.
    pub fn outermost_group(&self, element: ElementId) -> Option<GroupId> {
        self.enclosing_groups(self.element(element)?.group).last()
    }

    /// Every element of `group` and of the groups nested in it, in model
    /// order.
    pub fn group_members(&self, group: GroupId) -> Vec<ElementId> {
        self.elements
            .values()
            .filter(|e| self.enclosing_groups(e.group).any(|g| g == group))
            .map(|e| e.id)
            .collect()
    }

    /// `ids` with each grouped id replaced by every member of its outermost
    /// group (ELM-015): what a selection of `ids` covers. Ungrouped and
    /// unknown ids stay alone. Members follow in model order at the place of
    /// their group's first id; every id appears once.
    pub fn expand_groups(&self, ids: &[ElementId]) -> Vec<ElementId> {
        let outermost: Vec<Option<GroupId>> = ids.iter().map(|id| self.outermost_group(*id)).collect();
        let wanted: HashSet<GroupId> = outermost.iter().flatten().copied().collect();
        let mut members: HashMap<GroupId, Vec<ElementId>> = HashMap::new();
        if !wanted.is_empty() {
            for e in self.elements.values() {
                if let Some(group) = self.enclosing_groups(e.group).last().filter(|g| wanted.contains(g)) {
                    members.entry(group).or_default().push(e.id);
                }
            }
        }
        let mut seen = HashSet::new();
        let mut expanded = Vec::with_capacity(ids.len());
        for (id, group) in ids.iter().zip(outermost) {
            let covered = match group {
                Some(group) => members.remove(&group).unwrap_or_default(),
                None => vec![*id],
            };
            expanded.extend(covered.into_iter().filter(|id| seen.insert(*id)));
        }
        expanded
    }
}

#[cfg(test)]
mod tests {
    use glam::DVec2;

    use super::*;
    use crate::template::default_metric_project;

    /// Walls A to F and the groups Outer { Inner { A, B }, C } and
    /// Pair { D, E }; F is ungrouped. The elements are in the order A to F.
    fn nested() -> (Project, [ElementId; 6], [GroupId; 3]) {
        let mut p = default_metric_project();
        let ground = p.storeys.ground();
        let walls: Vec<_> = (0..6)
            .map(|n| {
                let y = f64::from(n);
                p.new_wall(ground, DVec2::new(0.0, y), DVec2::new(4.0, y), n)
            })
            .collect();
        let ids: [ElementId; 6] = std::array::from_fn(|i| walls[i].id);
        let group = |name: &str, parent| Group {
            id: GroupId::new(),
            name: name.into(),
            parent,
        };
        let outer = group("Outer", None);
        let inner = group("Inner", Some(outer.id));
        let pair = group("Pair", None);
        let groups = [outer.id, inner.id, pair.id];
        let membership = [
            Some(inner.id),
            Some(inner.id),
            Some(outer.id),
            Some(pair.id),
            Some(pair.id),
            None,
        ];
        for (mut wall, group) in walls.into_iter().zip(membership) {
            wall.group = group;
            p.elements.insert(wall.id, wall);
        }
        // The inner group comes first, so the order of groups does not decide.
        for g in [inner, outer, pair] {
            p.groups.insert(g.id, g);
        }
        (p, ids, groups)
    }

    #[test]
    fn outermost_group_follows_the_parents_to_the_top() {
        let (p, [a, _, c, d, _, f], [outer, _, pair]) = nested();
        assert_eq!(p.outermost_group(a), Some(outer));
        assert_eq!(p.outermost_group(c), Some(outer));
        assert_eq!(p.outermost_group(d), Some(pair));
        assert_eq!(p.outermost_group(f), None);
        assert_eq!(p.outermost_group(ElementId::new()), None);
    }

    #[test]
    fn group_members_include_the_nested_groups() {
        let (p, [a, b, c, d, e, _], [outer, inner, pair]) = nested();
        assert_eq!(p.group_members(outer), [a, b, c]);
        assert_eq!(p.group_members(inner), [a, b]);
        assert_eq!(p.group_members(pair), [d, e]);
        assert!(p.group_members(GroupId::new()).is_empty());
    }

    #[test]
    fn group_path_runs_from_the_outermost_group_inwards() {
        let (p, [a, _, c, d, _, f], [outer, inner, pair]) = nested();
        assert_eq!(p.group_path(a), [outer, inner]);
        assert_eq!(p.group_path(c), [outer]);
        assert_eq!(p.group_path(d), [pair]);
        assert!(p.group_path(f).is_empty());
    }

    #[test]
    fn expand_groups_covers_each_outermost_group_and_keeps_ungrouped_ids_alone() {
        let (p, [a, b, c, d, e, f], _) = nested();
        assert_eq!(p.expand_groups(&[b]), [a, b, c]);
        assert_eq!(p.expand_groups(&[f]), [f]);
        assert_eq!(p.expand_groups(&[f, e, c, a]), [f, d, e, a, b, c]);
        let unknown = ElementId::new();
        assert_eq!(p.expand_groups(&[unknown, unknown]), [unknown]);
        assert!(p.expand_groups(&[]).is_empty());
    }

    /// A parent cycle, which only a damaged project can have, ends the
    /// walk instead of looping.
    #[test]
    fn a_parent_cycle_does_not_hang_the_queries() {
        let (mut p, [a, ..], [outer, inner, _]) = nested();
        p.groups.get_mut(&outer).unwrap().parent = Some(inner);
        assert_eq!(p.group_path(a).len(), 3);
        assert!(p.outermost_group(a).is_some());
        assert_eq!(p.group_members(outer).len(), 3);
    }

    #[test]
    fn automatic_names_count_from_one() {
        assert_eq!(Group::automatic_name(1), "Group 1");
        assert_eq!(Group::automatic_name(12), "Group 12");
    }
}
