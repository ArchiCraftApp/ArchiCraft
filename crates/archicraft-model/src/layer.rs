//! Layers: named visibility and locking categories (LAY-001).

use serde::{Deserialize, Serialize};

use crate::ids::LayerId;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Layer {
    pub id: LayerId,
    pub name: String,
    /// Elements on hidden layers are neither drawn nor selectable.
    pub visible: bool,
    /// Elements on locked layers are drawn but not editable.
    pub locked: bool,
    /// Elements on excluded layers are drawn but provide no snap targets
    /// (INP-017).
    #[serde(default)]
    pub snap_excluded: bool,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: LayerId::new(),
            name: name.into(),
            visible: true,
            locked: false,
            snap_excluded: false,
        }
    }
}
