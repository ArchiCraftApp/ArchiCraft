//! The keys that run each command (UI-009, HW-003): the default bindings of
//! the registry, replaced per command where the keymap says so. The views
//! that match their own keys ([`CommandSpec::local`]) ask the keymap, so a
//! rebound key works there at once. The user's keymap and its editor follow
//! with archicraft-qb2; until then the keymap holds the defaults.
//!
//! [`CommandSpec::local`]: super::CommandSpec::local

use std::cmp::Reverse;
use std::collections::HashMap;

use egui::{Key, Modifiers};

use super::{Binding, CommandId, Registry};

/// The bindings that differ from the registry's defaults.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keymap {
    overrides: HashMap<CommandId, Vec<Binding>>,
}

impl Keymap {
    /// Binds `id` to `bindings` instead of its defaults. A
    /// [fixed](super::CommandSpec::fixed) command keeps its defaults.
    pub fn set(&mut self, id: CommandId, bindings: Vec<Binding>) {
        self.overrides.insert(id, bindings);
    }

    /// The bindings that run `id` now: its own from the keymap, otherwise
    /// the registry's defaults; none for an unknown id.
    pub fn effective_bindings(&self, id: CommandId) -> &[Binding] {
        let Some(spec) = Registry::get().spec(id.as_str()) else {
            return &[];
        };
        match self.overrides.get(&id) {
            Some(bindings) if !spec.fixed => bindings,
            _ => &spec.default_bindings,
        }
    }

    /// The first of `ids` that the press of `key` with `modifiers` runs. As in
    /// [`Registry::consume_shortcuts`] a binding matches when its modifiers are
    /// held, an extra Shift ignored, and bindings with more modifiers are
    /// tried first, so Shift+R wins over R.
    pub fn matching(&self, ids: &[CommandId], key: Key, modifiers: Modifiers) -> Option<CommandId> {
        let mut bindings: Vec<(Binding, CommandId)> = ids
            .iter()
            .flat_map(|&id| self.effective_bindings(id).iter().map(move |&b| (b, id)))
            .filter(|(binding, _)| binding.key == key)
            .collect();
        bindings.sort_by_key(|(binding, _)| Reverse(binding.modifier_count()));
        bindings
            .into_iter()
            .find(|(binding, _)| modifiers.matches_logically(binding.modifiers()))
            .map(|(_, id)| id)
    }
}
