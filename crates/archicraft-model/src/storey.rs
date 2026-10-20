//! Storeys (STR-001…006).
//!
//! The storey list is kept sorted by elevation. For every storey but the top
//! one, the height is derived: it is the distance to the next storey. The top
//! storey has a nominal height that elements linked to "the storey above" use.
//!
//! Errors are message keys (NFR-L10N-001), with names as text and limits as
//! length arguments.

use serde::{Deserialize, Serialize};

use crate::ids::StoreyId;
use crate::message::Message;
use crate::tolerance::MODEL_TOLERANCE;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Storey {
    pub id: StoreyId,
    pub name: String,
    /// Signed elevation relative to project zero, in metres.
    pub elevation: f64,
    /// Distance to the next storey (positive). For the top storey, a nominal
    /// height.
    pub height: f64,
    /// Plan cut-plane height above the storey elevation; `None` uses the
    /// project default (STR-008).
    pub cut_plane_height: Option<f64>,
}

impl Storey {
    pub fn new(name: impl Into<String>, elevation: f64, height: f64) -> Self {
        Self {
            id: StoreyId::new(),
            name: name.into(),
            elevation,
            height,
            cut_plane_height: None,
        }
    }
}

/// The ordered storeys of the building together with the ground storey,
/// which is numbered 0 (STR-001).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoreyList {
    storeys: Vec<Storey>,
    ground: StoreyId,
}

/// Smallest allowed storey height: 1 cm.
pub const MIN_STOREY_HEIGHT: f64 = 0.01;
/// Largest storey cut-plane height above the storey elevation: ±100 m.
pub const MAX_CUT_PLANE_HEIGHT: f64 = 100.0;

fn unknown_storey() -> Message {
    Message::new("model-error-storey-unknown")
}

fn height_too_small() -> Message {
    Message::new("model-error-storey-height-min").length("min", MIN_STOREY_HEIGHT)
}

impl StoreyList {
    /// Builds a list from storeys in any order. The ground storey must be one
    /// of them; heights of all but the top storey are recomputed.
    pub fn new(mut storeys: Vec<Storey>, ground: StoreyId) -> Result<Self, Message> {
        if storeys.is_empty() {
            return Err(Message::new("model-error-no-storeys"));
        }
        if !storeys.iter().any(|s| s.id == ground) {
            return Err(Message::new("model-error-ground-not-in-list"));
        }
        storeys.sort_by(|a, b| a.elevation.total_cmp(&b.elevation));
        let list = Self { storeys, ground };
        list.check()?;
        Ok(list.with_derived_heights())
    }

    pub fn as_slice(&self) -> &[Storey] {
        &self.storeys
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Storey> {
        self.storeys.iter()
    }

    pub fn len(&self) -> usize {
        self.storeys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.storeys.is_empty()
    }

    pub fn ground(&self) -> StoreyId {
        self.ground
    }

    pub fn get(&self, id: StoreyId) -> Option<&Storey> {
        self.storeys.iter().find(|s| s.id == id)
    }

    pub fn index_of(&self, id: StoreyId) -> Option<usize> {
        self.storeys.iter().position(|s| s.id == id)
    }

    /// Storey number: 0 for the ground storey, positive above, negative below.
    pub fn number(&self, id: StoreyId) -> Option<i32> {
        let index = self.index_of(id)? as i32;
        let ground = self.index_of(self.ground)? as i32;
        Some(index - ground)
    }

    /// The storey `offset` positions above (`offset` > 0) or below `id`.
    pub fn relative(&self, id: StoreyId, offset: i32) -> Option<&Storey> {
        let index = self.index_of(id)? as i64 + i64::from(offset);
        usize::try_from(index).ok().and_then(|i| self.storeys.get(i))
    }

    /// Elevation of the storey `offset` positions from `id`. Beyond the top
    /// storey, the top storey's nominal height is repeated; below the lowest
    /// storey, the lowest storey's height is repeated (STR-001).
    pub fn relative_elevation(&self, id: StoreyId, offset: i32) -> Option<f64> {
        let index = self.index_of(id)? as i64 + i64::from(offset);
        let last = self.storeys.len() as i64 - 1;
        if index < 0 {
            let lowest = &self.storeys[0];
            Some(lowest.elevation + index as f64 * lowest.height)
        } else if index > last {
            let top = &self.storeys[last as usize];
            Some(top.elevation + (index - last) as f64 * top.height)
        } else {
            Some(self.storeys[index as usize].elevation)
        }
    }

    /// The storey whose elevation range contains `elevation` (the lowest
    /// storey for anything below it, the top storey for anything above). An
    /// elevation within the model tolerance of a storey's elevation belongs
    /// to that storey (NFR-ACC-003).
    pub fn storey_at_elevation(&self, elevation: f64) -> &Storey {
        self.storeys
            .iter()
            .rev()
            .find(|s| s.elevation < elevation + MODEL_TOLERANCE)
            .unwrap_or(&self.storeys[0])
    }

    /// Storeys from the highest to the lowest, as shown in the project
    /// browser (VIEW-003).
    pub fn highest_first(&self) -> impl Iterator<Item = &Storey> {
        self.storeys.iter().rev()
    }

    fn check(&self) -> Result<(), Message> {
        for pair in self.storeys.windows(2) {
            if pair[1].elevation - pair[0].elevation < MIN_STOREY_HEIGHT {
                return Err(Message::new("model-error-storeys-too-close")
                    .text("lower", &pair[0].name)
                    .text("upper", &pair[1].name)
                    .length("min", MIN_STOREY_HEIGHT));
            }
        }
        for s in &self.storeys {
            if !s.elevation.is_finite() || !s.height.is_finite() {
                return Err(Message::new("model-error-storey-invalid-values").text("name", &s.name));
            }
            if s.name.trim().is_empty() {
                return Err(Message::new("model-error-storey-name"));
            }
            if let Some(cut) = s.cut_plane_height
                && (!cut.is_finite() || cut.abs() > MAX_CUT_PLANE_HEIGHT)
            {
                return Err(Message::new("model-error-storey-cut-plane")
                    .text("name", &s.name)
                    .length("max", MAX_CUT_PLANE_HEIGHT));
            }
        }
        if let Some(top) = self.storeys.last()
            && top.height < MIN_STOREY_HEIGHT
        {
            return Err(height_too_small());
        }
        Ok(())
    }

    fn with_derived_heights(mut self) -> Self {
        for i in 0..self.storeys.len().saturating_sub(1) {
            self.storeys[i].height = self.storeys[i + 1].elevation - self.storeys[i].elevation;
        }
        self
    }

    fn validated(self) -> Result<Self, Message> {
        self.check()?;
        Ok(self.with_derived_heights())
    }

    /// Inserts `storey` directly above or below `reference` (STR-003). The new
    /// storey gets the height `storey.height`; storeys above an inserted storey
    /// move up by that height, storeys below a storey inserted below move down.
    pub fn insert(&self, reference: StoreyId, above: bool, mut storey: Storey) -> Result<Self, Message> {
        let index = self.index_of(reference).ok_or_else(unknown_storey)?;
        if storey.height < MIN_STOREY_HEIGHT || !storey.height.is_finite() {
            return Err(height_too_small());
        }
        let mut storeys = self.storeys.clone();
        let reference_storey = &storeys[index];
        if above {
            storey.elevation = reference_storey.elevation + reference_storey.height;
            for s in storeys.iter_mut().skip(index + 1) {
                s.elevation += storey.height;
            }
            storeys.insert(index + 1, storey);
        } else {
            storey.elevation = reference_storey.elevation - storey.height;
            for s in storeys.iter_mut().take(index) {
                s.elevation -= storey.height;
            }
            storeys.insert(index, storey);
        }
        Self {
            storeys,
            ground: self.ground,
        }
        .validated()
    }

    /// Removes a storey so that the remaining storey heights are unchanged,
    /// which makes it the inverse of [`StoreyList::insert`]: removing a storey
    /// at or above the ground storey moves the storeys above it down by its
    /// height; removing a basement storey moves the storeys below it up.
    /// Deleting the ground storey makes the storey above it (or, if none, the
    /// one below it) the ground storey.
    pub fn remove(&self, id: StoreyId) -> Result<Self, Message> {
        if self.storeys.len() <= 1 {
            return Err(Message::new("model-error-last-storey"));
        }
        let index = self.index_of(id).ok_or_else(unknown_storey)?;
        let below_ground = self.number(id).is_some_and(|n| n < 0);
        let mut storeys = self.storeys.clone();
        let removed = storeys.remove(index);
        if below_ground {
            for s in storeys.iter_mut().take(index) {
                s.elevation += removed.height;
            }
        } else if index < storeys.len() {
            for s in storeys.iter_mut().skip(index) {
                s.elevation -= removed.height;
            }
        }
        let ground = if self.ground == id {
            storeys
                .get(index)
                .or_else(|| storeys.get(index.wrapping_sub(1)))
                .map(|s| s.id)
                .ok_or_else(|| Message::new("model-error-no-ground-left"))?
        } else {
            self.ground
        };
        Self { storeys, ground }.validated()
    }

    pub fn rename(&self, id: StoreyId, name: &str) -> Result<Self, Message> {
        let mut list = self.clone();
        let index = list.index_of(id).ok_or_else(unknown_storey)?;
        list.storeys[index].name = name.trim().to_owned();
        list.validated()
    }

    /// Changes a storey's elevation (STR-002). Without `shift_above`, the
    /// storeys above keep their elevations and the heights of this storey and
    /// the one below change. With `shift_above`, this storey and all above
    /// move together and keep their heights.
    pub fn set_elevation(&self, id: StoreyId, elevation: f64, shift_above: bool) -> Result<Self, Message> {
        if !elevation.is_finite() {
            return Err(Message::new("model-error-invalid-elevation"));
        }
        let mut list = self.clone();
        let index = list.index_of(id).ok_or_else(unknown_storey)?;
        let delta = elevation - list.storeys[index].elevation;
        if shift_above {
            for s in list.storeys.iter_mut().skip(index) {
                s.elevation += delta;
            }
        } else {
            list.storeys[index].elevation = elevation;
        }
        if index > 0 && list.storeys[index].elevation - list.storeys[index - 1].elevation < MIN_STOREY_HEIGHT {
            return Err(Message::new("model-error-storey-below"));
        }
        if index + 1 < list.storeys.len()
            && list.storeys[index + 1].elevation - list.storeys[index].elevation < MIN_STOREY_HEIGHT
        {
            return Err(Message::new("model-error-storey-above"));
        }
        list.validated()
    }

    /// Changes a storey's height (STR-002). With `shift_above`, all storeys
    /// above move by the difference. Without it, only the next storey moves,
    /// which changes that storey's height instead.
    pub fn set_height(&self, id: StoreyId, height: f64, shift_above: bool) -> Result<Self, Message> {
        if !height.is_finite() || height < MIN_STOREY_HEIGHT {
            return Err(height_too_small());
        }
        let mut list = self.clone();
        let index = list.index_of(id).ok_or_else(unknown_storey)?;
        let delta = height - list.storeys[index].height;
        list.storeys[index].height = height;
        if shift_above {
            for s in list.storeys.iter_mut().skip(index + 1) {
                s.elevation += delta;
            }
        } else if let Some(next) = list.storeys.get_mut(index + 1) {
            next.elevation += delta;
            if let Some(after) = self.storeys.get(index + 2)
                && after.elevation - next.elevation < MIN_STOREY_HEIGHT
            {
                return Err(Message::new("model-error-storey-collision"));
            }
        }
        list.validated()
    }

    pub fn set_cut_plane(&self, id: StoreyId, height: Option<f64>) -> Result<Self, Message> {
        let mut list = self.clone();
        let index = list.index_of(id).ok_or_else(unknown_storey)?;
        list.storeys[index].cut_plane_height = height;
        list.validated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three() -> (StoreyList, [StoreyId; 3]) {
        let s0 = Storey::new("00", 0.0, 3.0);
        let s1 = Storey::new("01", 3.0, 3.0);
        let s2 = Storey::new("02", 6.0, 2.8);
        let ids = [s0.id, s1.id, s2.id];
        (StoreyList::new(vec![s2, s0, s1], ids[0]).unwrap(), ids)
    }

    #[test]
    fn sorted_and_numbered() {
        let (list, ids) = three();
        assert_eq!(list.as_slice()[0].id, ids[0]);
        assert_eq!(list.number(ids[2]), Some(2));
        assert_eq!(list.highest_first().next().unwrap().id, ids[2]);
    }

    #[test]
    fn height_change_shifts_storeys_above() {
        let (list, ids) = three();
        let list = list.set_height(ids[0], 3.5, true).unwrap();
        assert_eq!(list.get(ids[1]).unwrap().elevation, 3.5);
        assert_eq!(list.get(ids[2]).unwrap().elevation, 6.5);
        assert_eq!(list.get(ids[1]).unwrap().height, 3.0);
    }

    #[test]
    fn elevation_change_changes_height_below() {
        let (list, ids) = three();
        let list = list.set_elevation(ids[1], 3.2, false).unwrap();
        assert!((list.get(ids[0]).unwrap().height - 3.2).abs() < 1e-12);
        assert!((list.get(ids[1]).unwrap().height - 2.8).abs() < 1e-12);
        assert_eq!(list.get(ids[2]).unwrap().elevation, 6.0);
        assert!(list.set_elevation(ids[1], 6.5, false).is_err());
    }

    #[test]
    fn insert_and_remove_are_inverse() {
        let (list, ids) = three();
        let basement = Storey::new("-01", 0.0, 2.5);
        let basement_id = basement.id;
        let inserted = list.insert(ids[0], false, basement).unwrap();
        assert_eq!(inserted.get(basement_id).unwrap().elevation, -2.5);
        assert_eq!(inserted.number(basement_id), Some(-1));
        let removed = inserted.remove(basement_id).unwrap();
        assert_eq!(removed, list);

        let mezzanine = Storey::new("00a", 0.0, 1.5);
        let mezzanine_id = mezzanine.id;
        let inserted = list.insert(ids[0], true, mezzanine).unwrap();
        assert_eq!(inserted.get(mezzanine_id).unwrap().elevation, 3.0);
        assert_eq!(inserted.get(ids[1]).unwrap().elevation, 4.5);
        assert_eq!(inserted.remove(mezzanine_id).unwrap(), list);
    }

    #[test]
    fn storey_at_elevation_uses_the_model_tolerance() {
        let (list, ids) = three();
        assert_eq!(list.storey_at_elevation(4.0).id, ids[1]);
        assert_eq!(list.storey_at_elevation(-1.0).id, ids[0]);
        assert_eq!(list.storey_at_elevation(10.0).id, ids[2]);
        // NFR-ACC-003: 5 µm below the first floor is the first floor; 20 µm
        // below is still the ground floor.
        assert_eq!(list.storey_at_elevation(3.0 - 0.000_005).id, ids[1]);
        assert_eq!(list.storey_at_elevation(3.0 - 0.000_020).id, ids[0]);
    }

    #[test]
    fn relative_elevation_extrapolates_beyond_top() {
        let (list, ids) = three();
        assert_eq!(list.relative_elevation(ids[0], 1), Some(3.0));
        assert_eq!(list.relative_elevation(ids[2], 1), Some(6.0 + 2.8));
        assert_eq!(list.relative_elevation(ids[0], -1), Some(-3.0));
    }

    #[test]
    fn deleting_ground_promotes_next_storey() {
        let (list, ids) = three();
        let list = list.remove(ids[0]).unwrap();
        assert_eq!(list.ground(), ids[1]);
        assert!(list.remove(ids[1]).unwrap().remove(ids[2]).is_err());
    }
}
