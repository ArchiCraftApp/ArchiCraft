//! The message catalogues compiled into the application (NFR-L10N-001).
//!
//! Each language is a set of Fluent files under `crates/archicraft-app/locales/<language>/`,
//! embedded with `include_str!` so the application needs no files beside the
//! executable. Every file owns the key prefixes of one area, which keeps a key
//! from being defined in two files.

use fluent_bundle::{FluentBundle, FluentResource};
use fluent_syntax::ast::Entry;
use unic_langid::LanguageIdentifier;

/// One embedded Fluent file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CatalogueFile {
    /// The file name, for messages about the file.
    pub name: &'static str,
    /// The Fluent source.
    pub source: &'static str,
    /// The key prefixes this file owns: every message in it starts with one
    /// of them.
    pub prefixes: &'static [&'static str],
}

/// The English catalogue, the default and the fallback for every other
/// language (NFR-L10N-001).
pub const ENGLISH: &[CatalogueFile] = &[
    CatalogueFile {
        name: "en/app.ftl",
        source: include_str!("../../locales/en/app.ftl"),
        prefixes: &["menu-", "status-", "browser-"],
    },
    CatalogueFile {
        name: "en/commands.ftl",
        source: include_str!("../../locales/en/commands.ftl"),
        prefixes: &["cmd-"],
    },
    CatalogueFile {
        name: "en/dialogs.ftl",
        source: include_str!("../../locales/en/dialogs.ftl"),
        prefixes: &["dlg-"],
    },
    CatalogueFile {
        name: "en/tools.ftl",
        source: include_str!("../../locales/en/tools.ftl"),
        prefixes: &["tool-", "prompt-"],
    },
    CatalogueFile {
        name: "en/model.ftl",
        source: include_str!("../../locales/en/model.ftl"),
        prefixes: &["model-"],
    },
];

/// The language of [`ENGLISH`].
pub fn english() -> LanguageIdentifier {
    "en".parse().unwrap_or_default()
}

/// A problem in a catalogue file: a Fluent syntax error or a message that is
/// defined twice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogueError {
    pub file: &'static str,
    pub message: String,
}

impl std::fmt::Display for CatalogueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.file, self.message)
    }
}

/// Parses one file. A file with syntax errors still yields every message
/// that could be read, together with the errors.
pub fn parse(file: &CatalogueFile) -> (FluentResource, Vec<CatalogueError>) {
    match FluentResource::try_new(file.source.to_owned()) {
        Ok(resource) => (resource, Vec::new()),
        Err((resource, errors)) => {
            let errors = errors
                .into_iter()
                .map(|e| CatalogueError {
                    file: file.name,
                    message: format!("line {}: {e}", line_of(file.source, e.pos.start)),
                })
                .collect();
            (resource, errors)
        }
    }
}

/// The message keys (Fluent message identifiers) of one file, in file order.
/// Terms (`-term`) are private to the catalogue and not listed.
pub fn keys(file: &CatalogueFile) -> Vec<String> {
    let (resource, _) = parse(file);
    resource
        .entries()
        .filter_map(|entry| match entry {
            Entry::Message(m) => Some(m.id.name.to_owned()),
            _ => None,
        })
        .collect()
}

/// Builds a bundle for `language` from `files`, with Unicode isolation marks
/// switched off: egui draws U+2068/U+2069 as missing glyphs, and ArchiCraft
/// has no right-to-left language. Returns the bundle and every problem met;
/// the bundle holds whatever could be read.
pub fn bundle(
    language: LanguageIdentifier,
    files: &[CatalogueFile],
) -> (FluentBundle<FluentResource>, Vec<CatalogueError>) {
    let mut bundle = FluentBundle::new(vec![language]);
    bundle.set_use_isolating(false);
    let mut problems = Vec::new();
    for file in files {
        let (resource, errors) = parse(file);
        problems.extend(errors);
        if let Err(errors) = bundle.add_resource(resource) {
            problems.extend(errors.into_iter().map(|e| CatalogueError {
                file: file.name,
                message: e.to_string(),
            }));
        }
    }
    (bundle, problems)
}

/// The 1-based line of a byte offset.
fn line_of(source: &str, offset: usize) -> usize {
    source.get(..offset).unwrap_or(source).matches('\n').count() + 1
}
