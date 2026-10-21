//! The `tr!` keys in the sources and the English catalogue match
//! (NFR-L10N-001): every literal key exists, and every catalogue key is used
//! or belongs to a key family built at runtime.

#![allow(clippy::unwrap_used)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use archicraft_app::i18n::{DYNAMIC_PREFIXES, catalogue};
use proc_macro2::{TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ExprLit, Lit, LitStr, Macro, Token};

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg") && a.parse_args::<syn::Ident>().is_ok_and(|ident| ident == "test"))
    })
}

fn is_tr(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|s| s.ident == "tr")
}

/// Collects the literal keys of `tr!` calls, skipping `#[cfg(test)]` and
/// `#[test]` items wherever they are in the file.
#[derive(Default)]
struct KeyScanner {
    keys: Vec<(String, usize)>,
}

impl KeyScanner {
    fn record(&mut self, key: &LitStr) {
        self.keys.push((key.value(), key.span().start().line));
    }

    /// Finds `tr!(…)` in the tokens of a macro whose body is not a list of
    /// expressions, such as `matches!(x, Pattern)`.
    fn scan_tokens(&mut self, tokens: TokenStream) {
        let trees: Vec<TokenTree> = tokens.into_iter().collect();
        for (i, tree) in trees.iter().enumerate() {
            let TokenTree::Group(group) = tree else {
                continue;
            };
            let is_call = matches!(&trees[..i], [.., TokenTree::Ident(name), TokenTree::Punct(bang)]
                if name == "tr" && bang.as_char() == '!');
            if is_call && let Ok(mac) = syn::parse2::<Macro>(quote_macro(group)) {
                self.visit_macro(&mac);
            } else {
                self.scan_tokens(group.stream());
            }
        }
    }
}

/// `tr!` followed by `group`, as tokens that parse as a [`Macro`].
fn quote_macro(group: &proc_macro2::Group) -> TokenStream {
    let mut tokens: TokenStream = "tr!".parse().unwrap();
    tokens.extend([TokenTree::Group(group.clone())]);
    tokens
}

impl<'ast> Visit<'ast> for KeyScanner {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_test(&i.attrs) {
            visit::visit_item_mod(self, i);
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if !is_test(&i.attrs) {
            visit::visit_item_fn(self, i);
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if !is_test(&i.attrs) {
            visit::visit_impl_item_fn(self, i);
        }
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        match mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated) {
            Ok(args) => {
                if is_tr(&mac.path)
                    && let Some(Expr::Lit(ExprLit { lit: Lit::Str(key), .. })) = args.first()
                {
                    self.record(key);
                }
                for arg in &args {
                    self.visit_expr(arg);
                }
            }
            Err(_) => self.scan_tokens(mac.tokens.clone()),
        }
    }
}

/// The literal keys of the `tr!` calls in `source`, with their 1-based lines,
/// read with syn so comments are ignored and raw strings are keys too.
/// `#[cfg(test)]` and `#[test]` items are skipped: tests use keys of their
/// own. A key that is not a string literal is built at runtime and must start
/// with one of [`DYNAMIC_PREFIXES`].
fn literal_keys(source: &str) -> Vec<(String, usize)> {
    let syntax = syn::parse_file(source).unwrap();
    let mut scanner = KeyScanner::default();
    scanner.visit_file(&syntax);
    scanner.keys
}

fn rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, files);
        } else if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
}

/// NFR-L10N-001: keys in comments and in test code are not checked, keys
/// after a test module in the middle of a file and raw-string keys are.
#[test]
fn the_scanner_finds_literal_keys_outside_comments_and_tests() {
    let source = r##"
//! ui.label(tr!("doc-comment-key"));
fn f(ui: &mut Ui) {
    ui.label(tr!("dlg-about-title"));
    ui.heading(tr!("dlg-about-heading", version = "1"));
    ui.label(tr!(
        "menu-help"
    ));
    ui.label(tr!(&format!("cmd-{id}-label")));
    let shader = include_str!("shader.wgsl");
    ui.label(crate::tr!("dlg-about-summary"));
    // ui.label(tr!("line-comment-key"));
    /* ui.label(tr!("block-comment-key")); */
}

#[cfg(test)]
mod tests {
    fn t() { tr!("test-only"); }
}

#[test]
fn test_function() { tr!("test-function-only"); }

fn g(ui: &mut Ui) {
    ui.label(tr!(r"raw-key"));
    ui.label(tr!(r#"raw-hash-key"#, count = 2));
    ui.label(format!("{}: {}", tr!("in-format"), 1));
    let shown = matches!(mode, Mode::A | Mode::B if visible(tr!("in-match-guard")));
}
"##;
    let keys: Vec<String> = literal_keys(source).into_iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        [
            "dlg-about-title",
            "dlg-about-heading",
            "menu-help",
            "dlg-about-summary",
            "raw-key",
            "raw-hash-key",
            "in-format",
            "in-match-guard",
        ]
    );
    let lines: Vec<usize> = literal_keys(source).into_iter().map(|(_, line)| line).collect();
    assert_eq!(lines, [4, 5, 7, 11, 25, 26, 27, 28]);
}

#[test]
fn tr_keys_and_the_english_catalogue_match() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let mut used: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for file in &files {
        let source = std::fs::read_to_string(file).unwrap();
        let name = file.strip_prefix(&src).unwrap().display().to_string();
        for (key, line) in literal_keys(&source) {
            used.entry(key).or_default().push(format!("{name}:{line}"));
        }
    }
    let defined: BTreeSet<String> = catalogue::ENGLISH.iter().flat_map(catalogue::keys).collect();

    let missing: Vec<String> = used
        .iter()
        .filter(|(key, _)| !defined.contains(*key))
        .map(|(key, places)| format!("{key} (used in {})", places.join(", ")))
        .collect();
    assert!(
        missing.is_empty(),
        "tr! keys missing from the English catalogue: {missing:?}"
    );

    let unused: Vec<&String> = defined
        .iter()
        .filter(|key| !used.contains_key(*key) && !DYNAMIC_PREFIXES.iter().any(|p| key.starts_with(p)))
        .collect();
    assert!(
        unused.is_empty(),
        "catalogue keys without a tr! call or a dynamic prefix {DYNAMIC_PREFIXES:?}: {unused:?}"
    );

    // The pilot keys are found, so the scan reached the migrated files.
    assert!(used.contains_key("menu-help") && used.contains_key("dlg-about-heading"));
}
