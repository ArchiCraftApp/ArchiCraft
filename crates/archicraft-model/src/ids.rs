//! Stable identities (SRS 3.2, IFX-010, IFX-011).
//!
//! Every object that is written to IFCX as a node has a UUID. New objects get
//! random (version 4) UUIDs; identities never change across edits, undo/redo or
//! save/open cycles.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// A new, random identity.
            #[allow(clippy::new_without_default)]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID, for example one read from a file.
            pub const fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            pub const fn uuid(self) -> Uuid {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(
    /// Identity of the project (the `IfcProject` node; `header.id` derives from it).
    ProjectId
);
id_type!(
    /// Identity of the site.
    SiteId
);
id_type!(
    /// Identity of the building.
    BuildingId
);
id_type!(
    /// Identity of a storey.
    StoreyId
);
id_type!(
    /// Identity of an element.
    ElementId
);
id_type!(
    /// Identity of a layer resource.
    LayerId
);
id_type!(
    /// Identity of a material resource.
    MaterialId
);
id_type!(
    /// Identity of a view definition.
    ViewId
);
