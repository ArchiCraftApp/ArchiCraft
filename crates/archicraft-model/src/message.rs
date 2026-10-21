//! Message keys for user-visible text produced by the model (NFR-L10N-001).
//!
//! The model has no message catalogue and no user-interface dependencies
//! (NFR-MNT-002). Where it produces text for the user, such as command
//! labels, it returns a [`Message`]: a key and typed arguments. The
//! application looks the key up in its catalogue (`locales/en/model.ftl` in
//! archicraft-app) and formats the arguments: lengths and angles in the
//! project units, element kinds as localized nouns, counts with the user's
//! number format and the language's plural rules.
//!
//! Every key the crate emits is listed in [`ALL_KEYS`], so the application
//! can check that its catalogue covers them. [`Message`]'s `Display` is the
//! key form, for example `model-label-delete-elements(count=3)`, for logs and
//! the command journal; the user interface never shows it.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::element::KindTag;

/// Every message key archicraft-model emits, including the label and noun
/// keys of the element kinds ([`crate::kinds::KindDescriptor`]).
pub const ALL_KEYS: &[&str] = &[
    // Command labels (Command::label).
    "model-label-create-kind",
    "model-label-create-elements",
    "model-label-delete-elements",
    "model-label-edit-elements",
    "model-label-move-elements",
    "model-label-insert-storey",
    "model-label-delete-storey",
    "model-label-rename-storey",
    "model-label-storey-elevation",
    "model-label-storey-height",
    "model-label-cut-plane",
    "model-label-new-layer",
    "model-label-change-layer",
    "model-label-delete-layer",
    "model-label-new-material",
    "model-label-change-material",
    "model-label-delete-material",
    "model-label-tool-settings",
    "model-label-project-settings",
    "model-label-project-info",
    "model-label-spatial-names",
    "model-label-location",
    // Element kinds (KindDescriptor::label_key and noun_key).
    "model-kind-wall",
    "model-kind-wall-noun",
    "model-kind-slab",
    "model-kind-slab-noun",
    // The test kind (feature `test-kinds`), listed in every build so that
    // the application's catalogue is the same in test and release builds.
    "model-kind-test",
    "model-kind-test-noun",
    // Command errors (CommandError::message).
    "model-error-no-change",
    "model-error-invalid-distance",
    "model-error-element-not-found",
    "model-error-duplicate-element-id",
    "model-error-element-invalid",
    "model-error-kind-change",
    "model-error-element-not-movable",
    "model-error-transform-unsupported",
    "model-error-duplicate-storey-id",
    "model-error-move-target-same",
    "model-error-target-storey-not-found",
    "model-error-duplicate-layer-id",
    "model-error-layer-not-found",
    "model-error-layer-name",
    "model-error-reassign-layer",
    "model-error-duplicate-material-id",
    "model-error-material-not-found",
    "model-error-replacement-material",
    "model-error-default-layer-missing",
    "model-error-default-material-missing",
    "model-error-default-ifc-class",
    "model-error-cut-plane-range",
    "model-error-tessellation-range",
    "model-error-display-precision",
    "model-error-project-name",
    "model-error-spatial-names",
    "model-error-location-invalid",
    // Element validation (validate.rs, Kind::validate).
    "model-error-coordinates-range",
    "model-error-polygon-points",
    "model-error-polygon-area",
    "model-error-polygon-sliver",
    "model-error-polygon-self-intersection",
    "model-error-hole",
    "model-error-hole-outside",
    "model-error-holes-overlap",
    "model-error-slab-points",
    "model-error-thickness-range",
    "model-error-base-offset-range",
    "model-error-top-offset-range",
    "model-error-slab-offset-range",
    "model-error-wall-length",
    "model-error-reference-line-range",
    "model-error-wall-height-range",
    "model-error-linked-storey-range",
    "model-error-wall-top-storey-missing",
    "model-error-element-name",
    "model-error-home-storey-missing",
    "model-error-element-layer-missing",
    "model-error-element-material-missing",
    "model-error-ifc-class-not-allowed",
    "model-error-vertical-extent",
    "model-error-top-below-base",
    // Storeys (storey.rs).
    "model-error-no-storeys",
    "model-error-ground-not-in-list",
    "model-error-storeys-too-close",
    "model-error-storey-invalid-values",
    "model-error-storey-name",
    "model-error-storey-cut-plane",
    "model-error-storey-height-min",
    "model-error-storey-unknown",
    "model-error-last-storey",
    "model-error-no-ground-left",
    "model-error-invalid-elevation",
    "model-error-storey-below",
    "model-error-storey-above",
    "model-error-storey-collision",
    // Materials (material.rs).
    "model-error-material-name",
    "model-error-opacity-range",
    "model-error-priority-range",
    "model-error-colour-range",
    // Project (project.rs): plan grid, locks and integrity problems.
    "model-error-snap-grid-range",
    "model-error-construction-grid-range",
    "model-error-auxiliary-spacing",
    "model-error-grid-rotation",
    "model-error-grid-origin",
    "model-error-element-locked",
    "model-error-layer-locked",
    "model-problem-home-storey-missing",
    "model-problem-layer-missing",
    "model-problem-material-missing",
];

/// Every argument name a [`Message`] of this crate uses. Names are
/// `&'static str`, so reading a message back (from a journal, for example)
/// maps the name onto this list.
pub const ARG_NAMES: &[&str] = &[
    "count", "kind", "min", "max", "name", "reason", "class", "lower", "upper", "layer",
];

/// A typed argument of a [`Message`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Arg {
    /// A number of things; selects the plural form.
    Count(u64),
    /// Text shown as is, such as an element or storey name.
    Text(String),
    /// A length in metres, shown in the project length unit.
    Length(f64),
    /// An angle in radians, shown in the project angle unit.
    Angle(f64),
    /// An element kind, shown as its localized noun.
    Kind(KindTag),
    /// Another message, rendered in place.
    Nested(Box<Message>),
}

/// User-visible text as a message key with named arguments (NFR-L10N-001).
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(into = "StoredMessage")]
pub struct Message {
    key: &'static str,
    args: Vec<(&'static str, Arg)>,
}

impl Message {
    /// A message without arguments; add them with the builder methods.
    pub fn new(key: &'static str) -> Self {
        Self { key, args: Vec::new() }
    }

    pub fn key(&self) -> &'static str {
        self.key
    }

    /// The arguments, in the order they were added.
    pub fn args(&self) -> &[(&'static str, Arg)] {
        &self.args
    }

    /// The argument called `name`.
    pub fn arg(&self, name: &str) -> Option<&Arg> {
        self.args.iter().find(|(n, _)| *n == name).map(|(_, a)| a)
    }

    pub fn with(mut self, name: &'static str, arg: Arg) -> Self {
        self.args.push((name, arg));
        self
    }

    pub fn count(self, name: &'static str, n: usize) -> Self {
        self.with(name, Arg::Count(u64::try_from(n).unwrap_or(u64::MAX)))
    }

    pub fn text(self, name: &'static str, text: impl Into<String>) -> Self {
        self.with(name, Arg::Text(text.into()))
    }

    pub fn length(self, name: &'static str, meters: f64) -> Self {
        self.with(name, Arg::Length(meters))
    }

    pub fn angle(self, name: &'static str, radians: f64) -> Self {
        self.with(name, Arg::Angle(radians))
    }

    pub fn kind(self, name: &'static str, kind: KindTag) -> Self {
        self.with(name, Arg::Kind(kind))
    }

    pub fn nested(self, name: &'static str, message: Message) -> Self {
        self.with(name, Arg::Nested(Box::new(message)))
    }
}

impl fmt::Display for Arg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Arg::Count(n) => write!(f, "{n}"),
            Arg::Text(text) => write!(f, "{text:?}"),
            Arg::Length(meters) => write!(f, "{meters}m"),
            Arg::Angle(radians) => write!(f, "{radians}rad"),
            Arg::Kind(kind) => f.write_str(kind.code()),
            Arg::Nested(message) => write!(f, "{message}"),
        }
    }
}

/// The key form, for example `model-label-create-kind(kind=wall)`, for logs
/// and the journal.
impl fmt::Display for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key)?;
        if !self.args.is_empty() {
            f.write_str("(")?;
            for (i, (name, arg)) in self.args.iter().enumerate() {
                if i > 0 {
                    f.write_str(", ")?;
                }
                write!(f, "{name}={arg}")?;
            }
            f.write_str(")")?;
        }
        Ok(())
    }
}

/// The serialised form of a [`Message`], with owned key and names.
#[derive(Serialize, Deserialize)]
struct StoredMessage {
    key: String,
    args: Vec<(String, Arg)>,
}

impl From<Message> for StoredMessage {
    fn from(message: Message) -> Self {
        Self {
            key: message.key.to_owned(),
            args: message.args.into_iter().map(|(n, a)| (n.to_owned(), a)).collect(),
        }
    }
}

/// Looks `name` up in `list`, returning the static string.
fn known(list: &[&'static str], name: &str) -> Option<&'static str> {
    list.iter().copied().find(|k| *k == name)
}

// Written by hand: a derived impl would borrow the `&'static str` fields
// from the input.
impl<'de> Deserialize<'de> for Message {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let stored = StoredMessage::deserialize(deserializer)?;
        Message::try_from(stored).map_err(serde::de::Error::custom)
    }
}

impl TryFrom<StoredMessage> for Message {
    type Error = String;

    fn try_from(stored: StoredMessage) -> Result<Self, String> {
        let key = known(ALL_KEYS, &stored.key).ok_or_else(|| format!("unknown message key {:?}", stored.key))?;
        let args = stored
            .args
            .into_iter()
            .map(|(name, arg)| {
                known(ARG_NAMES, &name)
                    .map(|n| (n, arg))
                    .ok_or_else(|| format!("unknown message argument {name:?}"))
            })
            .collect::<Result<_, String>>()?;
        Ok(Self { key, args })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_is_the_key_form() {
        assert_eq!(
            Message::new("model-label-new-layer").to_string(),
            "model-label-new-layer"
        );
        let message = Message::new("model-label-create-kind")
            .kind("kind", KindTag::Wall)
            .count("count", 2)
            .length("count", 0.25)
            .angle("count", 1.5)
            .text("kind", "A \"b\"")
            .nested("kind", Message::new("model-label-new-layer"));
        assert_eq!(
            message.to_string(),
            "model-label-create-kind(kind=wall, count=2, count=0.25m, count=1.5rad, kind=\"A \\\"b\\\"\", kind=model-label-new-layer)"
        );
    }

    #[test]
    fn unknown_keys_and_argument_names_are_rejected_on_reading() {
        let unknown_key = r#"{"key":"model-no-such-key","args":[]}"#;
        assert!(serde_json::from_str::<Message>(unknown_key).is_err());
        let unknown_arg = r#"{"key":"model-label-new-layer","args":[["nope",{"Count":1}]]}"#;
        assert!(serde_json::from_str::<Message>(unknown_arg).is_err());
    }

    #[test]
    fn keys_and_argument_names_are_unique() {
        for list in [ALL_KEYS, ARG_NAMES] {
            let unique: std::collections::BTreeSet<_> = list.iter().collect();
            assert_eq!(unique.len(), list.len());
        }
        assert!(ALL_KEYS.iter().all(|k| k.starts_with("model-")));
    }
}
