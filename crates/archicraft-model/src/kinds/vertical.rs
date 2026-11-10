//! The base and top of vertical elements (ELM-005, ELM-006, STR-004,
//! STR-006).
//!
//! Walls use it today; columns, curtain walls, stairs and storey-spanning
//! openings reuse the same type and helpers, so that storey links, relinking
//! and deleting a linked storey behave alike for every vertical kind.

use serde::{Deserialize, Serialize};

use super::StoreyLinkRef;
use crate::element::{Element, TopLink};
use crate::fields::{self, FieldSpec, FieldType, FieldValue, OptionLabel, Section};
use crate::ids::StoreyId;
use crate::message::Message;
use crate::project::Project;
use crate::storey::StoreyList;
use crate::validate::{MAX_HEIGHT, MAX_LINKED_STOREYS, MAX_OFFSET, MIN_HEIGHT};

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

    /// Replaces the top by the fixed height `height` above the base
    /// (ELM-006). Always returns `true`: every vertical link has a top.
    pub fn fix_top(&mut self, height: f64) -> bool {
        self.top = TopLink::Fixed { height };
        true
    }
}

/// Access to the [`VerticalLink`] of a vertical kind's elements, for the
/// shared height fields of [`fields`].
pub(crate) trait Vertical {
    /// The link, if `element` is of this kind.
    fn link_of(element: &Element) -> Option<VerticalLink>;

    /// Replaces the link; does nothing for an element of another kind.
    fn set_link_of(element: &mut Element, link: VerticalLink);
}

/// Applies `change` to the link of `element`, if it is of kind `K`.
fn change<K: Vertical>(element: &mut Element, change: impl FnOnce(&mut VerticalLink)) {
    if let Some(mut link) = K::link_of(element) {
        change(&mut link);
        K::set_link_of(element, link);
    }
}

/// The new top of an element of kind `K` on `home` for the top mode `mode`:
/// a fixed height or a storey link that keeps the current top elevation
/// (the storey above the home storey, else the storey at the top), or the
/// home storey + 1 (ELM-006). The current top, if `mode` is unknown.
fn top_for_mode(element: &Element, link: VerticalLink, mode: &str, project: &Project) -> TopLink {
    let (bottom, top) = project.vertical_extent(element).unwrap_or((0.0, 3.0));
    match mode {
        TopLink::FIXED => TopLink::Fixed { height: top - bottom },
        TopLink::STOREY => {
            let s = project
                .storeys
                .relative(element.storey, 1)
                .unwrap_or_else(|| project.storeys.storey_at_elevation(top));
            TopLink::Storey {
                storey: s.id,
                offset: top - s.elevation,
            }
        }
        TopLink::RELATIVE => TopLink::Relative {
            storeys: 1,
            offset: 0.0,
        },
        _ => link.top,
    }
}

/// The labels of [`TopLink::CODES`] in the `top_mode` field.
const TOP_MODE_LABELS: [&str; 3] = [
    "model-top-mode-fixed",
    "model-top-mode-storey",
    "model-top-mode-relative",
];

/// The storey offsets the `linked_storey` field offers (ELM-006).
const LINKED_STOREYS: [i32; 6] = [-2, -1, 0, 1, 2, 3];

/// The keys of the base and top fields of [`fields`], in their order: one
/// group that together sets the vertical extent, which settings transfer
/// keeps or restores as a whole (ELM-003).
pub(crate) const FIELD_KEYS: [&str; 6] = [
    "base_offset",
    "top_mode",
    "height",
    "top_storey",
    "linked_storey",
    "top_offset",
];

/// The base and top fields of a vertical kind `K` (ELM-005, ELM-006), in
/// the order of the properties panel: base offset, top mode, height (fixed
/// tops), top storey (storey links), linked storey (relative links) and top
/// offset (linked tops). `height_message` is the key the kind's validator
/// rejects a fixed height out of range with.
pub(crate) const fn fields<K: Vertical>(height_message: &'static str) -> [FieldSpec; 6] {
    [
        FieldSpec::new(
            FIELD_KEYS[0],
            "model-field-base-offset",
            Section::Geometry,
            FieldType::Elevation,
            |e, _| K::link_of(e).map(|l| FieldValue::Length(l.base_offset)),
            |e, v, _| {
                if let Some(v) = fields::length(v) {
                    change::<K>(e, |l| l.base_offset = v);
                }
            },
        )
        .ranged(-MAX_OFFSET, MAX_OFFSET, "model-error-base-offset-range"),
        FieldSpec::choice(
            FIELD_KEYS[1],
            "model-field-top-mode",
            Section::Geometry,
            |e, _| K::link_of(e).map(|l| FieldValue::Code(l.top.code())),
            |e, v, project| {
                if let (Some(mode), Some(link)) = (fields::code(v), K::link_of(e)) {
                    let top = top_for_mode(e, link, mode, project);
                    change::<K>(e, |l| l.top = top);
                }
            },
            |_, _| {
                TopLink::CODES
                    .into_iter()
                    .zip(TOP_MODE_LABELS)
                    .map(|(code, key)| (FieldValue::Code(code), fields::message(key)))
                    .collect()
            },
        ),
        FieldSpec::new(
            FIELD_KEYS[2],
            "model-field-height",
            Section::Geometry,
            FieldType::Length,
            |e, _| match K::link_of(e)?.top {
                TopLink::Fixed { height } => Some(FieldValue::Length(height)),
                _ => None,
            },
            |e, v, _| {
                if let Some(height) = fields::length(v) {
                    change::<K>(e, |l| l.top = TopLink::Fixed { height });
                }
            },
        )
        .ranged(MIN_HEIGHT, MAX_HEIGHT, height_message),
        FieldSpec::choice(
            FIELD_KEYS[3],
            "model-field-top-storey",
            Section::Geometry,
            |e, _| match K::link_of(e)?.top {
                TopLink::Storey { storey, .. } => Some(FieldValue::Storey(storey)),
                _ => None,
            },
            |e, v, _| {
                if let FieldValue::Storey(storey) = *v {
                    change::<K>(e, |l| {
                        if let TopLink::Storey { offset, .. } = l.top {
                            l.top = TopLink::Storey { storey, offset };
                        }
                    });
                }
            },
            fields::storey_options,
        ),
        FieldSpec::choice(
            FIELD_KEYS[4],
            "model-field-linked-storey",
            Section::Geometry,
            |e, _| match K::link_of(e)?.top {
                TopLink::Relative { storeys, .. } => Some(FieldValue::Count(i64::from(storeys))),
                _ => None,
            },
            |e, v, _| {
                if let FieldValue::Count(n) = *v
                    && let Ok(storeys) = i32::try_from(n)
                {
                    change::<K>(e, |l| {
                        if let TopLink::Relative { offset, .. } = l.top {
                            l.top = TopLink::Relative { storeys, offset };
                        }
                    });
                }
            },
            |_, _| {
                LINKED_STOREYS
                    .iter()
                    .map(|n| {
                        let label = Message::new("model-top-linked-storey").text("storeys", format!("{n:+}"));
                        (FieldValue::Count(i64::from(*n)), OptionLabel::Message(label))
                    })
                    .collect()
            },
        )
        .ranged(
            -(MAX_LINKED_STOREYS as f64),
            MAX_LINKED_STOREYS as f64,
            "model-error-linked-storey-range",
        ),
        FieldSpec::new(
            FIELD_KEYS[5],
            "model-field-top-offset",
            Section::Geometry,
            FieldType::Elevation,
            |e, _| match K::link_of(e)?.top {
                TopLink::Storey { offset, .. } | TopLink::Relative { offset, .. } => Some(FieldValue::Length(offset)),
                TopLink::Fixed { .. } => None,
            },
            |e, v, _| {
                if let Some(v) = fields::length(v) {
                    change::<K>(e, |l| match &mut l.top {
                        TopLink::Storey { offset, .. } | TopLink::Relative { offset, .. } => *offset = v,
                        TopLink::Fixed { .. } => {}
                    });
                }
            },
        )
        .ranged(-MAX_OFFSET, MAX_OFFSET, "model-error-top-offset-range"),
    ]
}
