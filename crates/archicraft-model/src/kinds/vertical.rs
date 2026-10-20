//! The base and top of vertical elements (ELM-005, ELM-006, STR-004,
//! STR-006).
//!
//! Walls use it today; columns, curtain walls, stairs and storey-spanning
//! openings reuse the same type and helpers, so that storey links, relinking
//! and deleting a linked storey behave alike for every vertical kind.

use serde::{Deserialize, Serialize};

use super::StoreyLinkRef;
use crate::element::TopLink;
use crate::ids::StoreyId;
use crate::storey::StoreyList;

/// Base offset and top of a vertical element. The base is an offset from
/// the home storey's elevation (ELM-005); the top is a fixed height above the
/// base or linked to a storey, absolutely or relative to the home storey,
/// plus an offset (ELM-006). Links are resolved at evaluation time, so linked
/// elements follow storey edits (STR-006).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct VerticalLink {
    /// Base offset relative to the home storey elevation.
    pub base_offset: f64,
    pub top: TopLink,
}

impl VerticalLink {
    /// Absolute bottom and top elevations for an element on `home`. `None`
    /// if the home storey or a linked storey is missing.
    pub fn extent(&self, home: StoreyId, storeys: &StoreyList) -> Option<(f64, f64)> {
        let bottom = storeys.get(home)?.elevation + self.base_offset;
        Some((bottom, self.top.elevation(home, bottom, storeys)?))
    }

    /// The first storey the link refers to by ID that is not in `storeys`,
    /// if any.
    pub fn missing_storey(&self, storeys: &StoreyList) -> Option<StoreyLinkRef> {
        self.storey_links()
            .into_iter()
            .find(|l| storeys.get(l.storey).is_none())
    }

    /// The storeys referred to by ID: the top's storey, if it is linked
    /// absolutely. Relative links follow the home storey instead.
    pub fn storey_links(&self) -> Vec<StoreyLinkRef> {
        match self.top {
            TopLink::Storey { storey, .. } => vec![StoreyLinkRef { role: "top", storey }],
            TopLink::Fixed { .. } | TopLink::Relative { .. } => Vec::new(),
        }
    }

    /// Re-expresses the link for a new home storey at `target_elevation`
    /// without moving the element: the base offset is recomputed and the top
    /// becomes a fixed height (STR-004). `None`, leaving the link unchanged,
    /// if a storey is missing.
    pub fn rehome(&mut self, home: StoreyId, storeys: &StoreyList, target_elevation: f64) -> Option<()> {
        let (bottom, top) = self.extent(home, storeys)?;
        *self = VerticalLink {
            base_offset: bottom - target_elevation,
            top: TopLink::Fixed { height: top - bottom },
        };
        Some(())
    }

    /// Re-expresses the link for the new home storey `target` without moving
    /// the element (ELM-005). A top linked relative to the old home storey is
    /// linked to the same storey absolutely, so it stays where it is and
    /// still follows that storey; a storey link stays; a fixed height stays.
    /// `None`, leaving the link unchanged, if a storey is missing.
    pub fn relink(&mut self, home: StoreyId, storeys: &StoreyList, target: StoreyId) -> Option<()> {
        let target_elevation = storeys.get(target)?.elevation;
        let mut moved = *self;
        moved.rehome(home, storeys, target_elevation)?;
        moved.top = match self.top {
            TopLink::Relative { storeys: n, offset } => match storeys.relative(home, n) {
                Some(s) => TopLink::Storey { storey: s.id, offset },
                None => moved.top,
            },
            other @ TopLink::Storey { .. } => other,
            TopLink::Fixed { .. } => moved.top,
        };
        *self = moved;
        Some(())
    }

    /// Replaces a link to `storey`, which is about to be deleted, by a fixed
    /// height that keeps the current top (STR-004). Returns whether the link
    /// changed.
    pub fn unlink_storey(&mut self, home: StoreyId, storeys: &StoreyList, storey: StoreyId) -> bool {
        if !matches!(self.top, TopLink::Storey { storey: s, .. } if s == storey) {
            return false;
        }
        let Some((bottom, top)) = self.extent(home, storeys) else {
            return false;
        };
        self.top = TopLink::Fixed { height: top - bottom };
        true
    }
}
