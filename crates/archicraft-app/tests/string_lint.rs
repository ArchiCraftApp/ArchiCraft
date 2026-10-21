//! NFR-L10N-001: user-visible text lives in the message catalogue
//! (`crates/archicraft-app/locales/en/*.ftl`, looked up with `tr!`), not in
//! string literals.
//!
//! The lint parses every source file of archicraft-app and archicraft-model
//! with syn and reports string literals that reach the user:
//!
//! - in archicraft-app, literal arguments (also the format string of
//!   `format!`) of egui calls that show text ([`SINK_METHODS`],
//!   [`SINK_FUNCTIONS`]) and of `Workspace::info`/`error` and
//!   `ArchiCraftApp::message`, and every literal in the body of a function
//!   that returns display text ([`TEXT_FUNCTIONS`], such as `Action::label`);
//! - in archicraft-model, which has no catalogue and returns message keys,
//!   every string literal with a space or an uppercase letter in its text;
//! - in both, the messages of thiserror's `#[error("…")]`.
//!
//! Not checked: `#[cfg(test)]` and `#[test]` code, `tests/` directories,
//! other attributes (doc comments, serde, cfg), log and panic messages
//! ([`SKIPPED_MACROS`], `.expect(…)`), `include_str!` and `concat!`. egui
//! `Id`s and salts are not sinks, so they stay literal. Strings the lint
//! cannot follow (built at runtime, `.into()` far from the call) are found by
//! the en-XA pseudo-locale audits.
//!
//! Text that is not user-interface text (project data such as default storey
//! names, file-format keywords) is listed with a reason in
//! `string_lint_allow.txt`. Files and items whose strings are still being
//! moved to the catalogue are listed in [`PENDING`]; the list only shrinks.

#![allow(clippy::unwrap_used)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Expr, ExprLit, Lit, LitStr, Macro, Stmt, Token, Type};

/// Files (`path`) and items (`path::Type::function`) not migrated to the
/// catalogue yet. The lint fails on literals outside these and on entries
/// without literals, so each migration must remove its entry, and the list
/// only shrinks. The About window and the Help menu title were migrated first
/// (archicraft-o4x.1); the comments name the task that empties each group.
const PENDING: &[&str] = &[
    // archicraft-5eu: the command registry replaces the action labels.
    "crates/archicraft-app/src/ui/actions.rs",
    // archicraft-e19: main window, views, tools and workspace.
    "crates/archicraft-app/src/app.rs",
    "crates/archicraft-app/src/input/snap.rs",
    "crates/archicraft-app/src/ui/browser.rs",
    "crates/archicraft-app/src/ui/shell.rs::menu_bar",
    "crates/archicraft-app/src/ui/shell.rs::status_bar",
    "crates/archicraft-app/src/ui/shell.rs::toolbox",
    "crates/archicraft-app/src/views/camera.rs",
    "crates/archicraft-app/src/views/plan.rs",
    "crates/archicraft-app/src/views/render3d.rs",
    "crates/archicraft-app/src/views/view3d.rs",
    "crates/archicraft-app/src/workspace.rs",
    // archicraft-e19, prompts with archicraft-bnf: tool labels, descriptions and prompts.
    "crates/archicraft-app/src/tools/mod.rs",
    "crates/archicraft-app/src/tools/slab.rs",
    "crates/archicraft-app/src/tools/wall.rs",
    // archicraft-yso8: the Performance window (feature `perf`).
    "crates/archicraft-app/src/perf.rs",
    // archicraft-emg: dialogs, properties and widgets.
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::delete_storey_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::layers_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::materials_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::preferences_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::project_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::settings_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::shortcuts_window",
    "crates/archicraft-app/src/ui/dialogs.rs::Dialogs::storeys_window",
    "crates/archicraft-app/src/ui/dialogs.rs::material_editor",
    "crates/archicraft-app/src/ui/dialogs.rs::problems_panel",
    "crates/archicraft-app/src/ui/properties.rs",
    "crates/archicraft-app/src/ui/widgets.rs",
];

/// The allowlist, next to this file.
const ALLOWLIST: &str = include_str!("string_lint_allow.txt");

/// egui methods (and the application's status-line and message methods)
/// whose string arguments are shown to the user.
const SINK_METHODS: &[&str] = &[
    // egui::Ui and egui::Response
    "label",
    "button",
    "small_button",
    "heading",
    "strong",
    "weak",
    "small",
    "monospace",
    "code",
    "colored_label",
    "link",
    "hyperlink_to",
    "selectable_label",
    "selectable_value",
    "checkbox",
    "radio",
    "radio_value",
    "toggle_value",
    "menu_button",
    "collapsing",
    "on_hover_text",
    "on_hover_text_at_pointer",
    "on_disabled_hover_text",
    // Widget builders
    "hint_text",
    "selected_text",
    "shortcut_text",
    "prefix",
    "suffix",
    "text",
    // rfd::FileDialog
    "add_filter",
    "set_title",
    // Workspace::info/error (status line), ArchiCraftApp::message
    "info",
    "error",
    "message",
];

/// Constructors (`Type::function`) whose string arguments are shown to the
/// user.
const SINK_FUNCTIONS: &[(&str, &str)] = &[
    ("RichText", "new"),
    ("Button", "new"),
    ("Button", "selectable"),
    ("Label", "new"),
    ("Window", "new"),
    ("CollapsingHeader", "new"),
    ("ComboBox", "from_label"),
    ("Checkbox", "new"),
    ("RadioButton", "new"),
    ("SelectableLabel", "new"),
    ("Link", "new"),
    ("Hyperlink", "from_label_and_url"),
];

/// Functions and methods that return display text: every literal with
/// letters in their bodies counts (archicraft-app).
const TEXT_FUNCTIONS: &[&str] = &["label", "description", "prompt", "title", "tooltip"];

/// Macros whose strings are not user-interface text: logging, assertions and
/// panics (developer-facing, they end up in the log or a crash report),
/// compile-time file and text helpers, and `tr!` itself.
const SKIPPED_MACROS: &[&str] = &[
    "trace",
    "debug",
    "info",
    "warn",
    "error",
    "event",
    "span",
    "trace_span",
    "debug_span",
    "info_span",
    "warn_span",
    "error_span",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "panic",
    "unreachable",
    "todo",
    "unimplemented",
    "include_str",
    "include_bytes",
    "concat",
    "env",
    "option_env",
    "stringify",
    "tr",
];

/// Macros whose first argument is a format string.
const FORMAT_MACROS: &[&str] = &["format", "format_args"];

/// How literals are judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rule {
    /// Only literals passed to UI sinks count (archicraft-app).
    Sinks,
    /// Every literal with a space or an uppercase letter counts
    /// (archicraft-model).
    AllText,
}

/// A user-visible literal.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Finding {
    file: String,
    line: usize,
    column: usize,
    /// The enclosing item, such as `Dialogs::about_window`.
    item: String,
    text: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{} ({}) {:?}",
            self.file, self.line, self.column, self.item, self.text
        )
    }
}

/// The text of a format string without its `{…}` placeholders.
fn without_placeholders(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' if chars.peek() == Some(&'{') => {
                chars.next();
                out.push('{');
            }
            '{' => {
                for c in chars.by_ref() {
                    if c == '}' {
                        break;
                    }
                }
            }
            '}' if chars.peek() == Some(&'}') => {
                chars.next();
                out.push('}');
            }
            c => out.push(c),
        }
    }
    out
}

fn is_test(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("test")
            || (a.path().is_ident("cfg") && a.parse_args::<syn::Ident>().is_ok_and(|ident| ident == "test"))
    })
}

fn last_segment(path: &syn::Path) -> String {
    path.segments.last().map(|s| s.ident.to_string()).unwrap_or_default()
}

fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(p) => last_segment(&p.path),
        _ => "impl".to_owned(),
    }
}

fn macro_arguments(mac: &Macro) -> Option<Punctuated<Expr, Token![,]>> {
    mac.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        .ok()
}

struct Scanner<'a> {
    file: &'a str,
    rule: Rule,
    items: Vec<String>,
    /// Inside a function of [`TEXT_FUNCTIONS`].
    text_functions: usize,
    findings: Vec<Finding>,
}

impl Scanner<'_> {
    fn record(&mut self, lit: &LitStr) {
        let text = lit.value();
        let visible = without_placeholders(&text);
        let has_letters = visible.chars().any(char::is_alphabetic);
        let counts = match self.rule {
            Rule::Sinks => has_letters,
            Rule::AllText => has_letters && (visible.contains(' ') || visible.chars().any(char::is_uppercase)),
        };
        if counts {
            let start = lit.span().start();
            self.findings.push(Finding {
                file: self.file.to_owned(),
                line: start.line,
                column: start.column + 1,
                item: self.items.join("::"),
                text,
            });
        }
    }

    /// Records the literal a sink argument shows: the literal itself, the
    /// format string of `format!`, or the literals of each branch.
    fn sink_argument(&mut self, expr: &Expr) {
        match expr {
            Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => self.record(s),
            Expr::Reference(r) => self.sink_argument(&r.expr),
            Expr::Paren(p) => self.sink_argument(&p.expr),
            Expr::Group(g) => self.sink_argument(&g.expr),
            Expr::MethodCall(m) if ["to_owned", "to_string", "into"].contains(&m.method.to_string().as_str()) => {
                self.sink_argument(&m.receiver);
            }
            Expr::Call(c) if is_function(&c.func, "String", "from") => {
                if let Some(arg) = c.args.first() {
                    self.sink_argument(arg);
                }
            }
            Expr::Macro(m) if FORMAT_MACROS.contains(&last_segment(&m.mac.path).as_str()) => {
                if let Some(arg) = macro_arguments(&m.mac).and_then(|args| args.into_iter().next()) {
                    self.sink_argument(&arg);
                }
            }
            Expr::Block(b) => self.block_value(&b.block),
            Expr::If(i) => {
                self.block_value(&i.then_branch);
                if let Some((_, otherwise)) = &i.else_branch {
                    self.sink_argument(otherwise);
                }
            }
            Expr::Match(m) => {
                for arm in &m.arms {
                    self.sink_argument(&arm.body);
                }
            }
            _ => {}
        }
    }

    fn block_value(&mut self, block: &syn::Block) {
        if let Some(Stmt::Expr(tail, None)) = block.stmts.last() {
            self.sink_argument(tail);
        }
    }

    /// Records string literals in macro tokens that are not expressions,
    /// skipping attributes (`#[doc = "…"]` from doc comments, for example).
    fn scan_tokens(&mut self, tokens: proc_macro2::TokenStream) {
        let mut attribute = false;
        for tree in tokens {
            match tree {
                proc_macro2::TokenTree::Punct(p) => {
                    attribute = p.as_char() == '#' || (attribute && p.as_char() == '!');
                }
                proc_macro2::TokenTree::Group(g) => {
                    if !(attribute && g.delimiter() == proc_macro2::Delimiter::Bracket) {
                        self.scan_tokens(g.stream());
                    }
                    attribute = false;
                }
                proc_macro2::TokenTree::Literal(l) => {
                    attribute = false;
                    if let Lit::Str(s) = Lit::new(l) {
                        self.record(&s);
                    }
                }
                proc_macro2::TokenTree::Ident(_) => attribute = false,
            }
        }
    }

    fn within(&mut self, name: String, f: impl FnOnce(&mut Self)) {
        self.items.push(name);
        f(self);
        self.items.pop();
    }

    /// Visits a function, treating its body as display text if its name is
    /// one of [`TEXT_FUNCTIONS`].
    fn function(&mut self, name: &syn::Ident, f: impl FnOnce(&mut Self)) {
        let text = self.rule == Rule::Sinks && TEXT_FUNCTIONS.iter().any(|t| name == t);
        self.text_functions += usize::from(text);
        self.within(name.to_string(), f);
        self.text_functions -= usize::from(text);
    }
}

fn is_function(func: &Expr, ty: &str, name: &str) -> bool {
    let Expr::Path(p) = func else {
        return false;
    };
    let segments: Vec<String> = p.path.segments.iter().map(|s| s.ident.to_string()).collect();
    matches!(segments.as_slice(), [.., t, n] if t == ty && n == name)
}

impl<'ast> Visit<'ast> for Scanner<'_> {
    fn visit_attribute(&mut self, attr: &'ast Attribute) {
        // Attributes are not user-interface text (doc comments, serde, cfg),
        // except the messages of thiserror's #[error("…")].
        if attr.path().is_ident("error")
            && let Ok(args) = attr.parse_args_with(Punctuated::<Expr, Token![,]>::parse_terminated)
            && let Some(Expr::Lit(ExprLit { lit: Lit::Str(s), .. })) = args.first()
        {
            self.record(s);
        }
    }

    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_mod(s, i));
        }
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if !is_test(&i.attrs) {
            self.function(&i.sig.ident, |s| visit::visit_item_fn(s, i));
        }
    }

    fn visit_item_impl(&mut self, i: &'ast syn::ItemImpl) {
        if !is_test(&i.attrs) {
            self.within(type_name(&i.self_ty), |s| visit::visit_item_impl(s, i));
        }
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if !is_test(&i.attrs) {
            self.function(&i.sig.ident, |s| visit::visit_impl_item_fn(s, i));
        }
    }

    fn visit_item_trait(&mut self, i: &'ast syn::ItemTrait) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_trait(s, i));
        }
    }

    fn visit_trait_item_fn(&mut self, i: &'ast syn::TraitItemFn) {
        if !is_test(&i.attrs) {
            self.function(&i.sig.ident, |s| visit::visit_trait_item_fn(s, i));
        }
    }

    fn visit_item_const(&mut self, i: &'ast syn::ItemConst) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_const(s, i));
        }
    }

    fn visit_item_static(&mut self, i: &'ast syn::ItemStatic) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_static(s, i));
        }
    }

    fn visit_item_struct(&mut self, i: &'ast syn::ItemStruct) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_struct(s, i));
        }
    }

    fn visit_item_enum(&mut self, i: &'ast syn::ItemEnum) {
        if !is_test(&i.attrs) {
            self.within(i.ident.to_string(), |s| visit::visit_item_enum(s, i));
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let method = call.method.to_string();
        if self.rule == Rule::Sinks && SINK_METHODS.contains(&method.as_str()) {
            for arg in &call.args {
                self.sink_argument(arg);
            }
        }
        if method == "expect" {
            // The message of a panic is developer-facing.
            self.visit_expr(&call.receiver);
            return;
        }
        visit::visit_expr_method_call(self, call);
    }

    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if self.rule == Rule::Sinks
            && SINK_FUNCTIONS
                .iter()
                .any(|(ty, name)| is_function(&call.func, ty, name))
        {
            for arg in &call.args {
                self.sink_argument(arg);
            }
        }
        visit::visit_expr_call(self, call);
    }

    fn visit_macro(&mut self, mac: &'ast Macro) {
        if SKIPPED_MACROS.contains(&last_segment(&mac.path).as_str()) {
            return;
        }
        match macro_arguments(mac) {
            Some(args) => {
                for arg in &args {
                    self.visit_expr(arg);
                }
            }
            None if self.rule == Rule::AllText || self.text_functions > 0 => self.scan_tokens(mac.tokens.clone()),
            None => {}
        }
    }

    fn visit_lit_str(&mut self, lit: &'ast LitStr) {
        if self.rule == Rule::AllText || self.text_functions > 0 {
            self.record(lit);
        }
    }
}

/// The user-visible literals of one source file.
fn scan(file: &str, source: &str, rule: Rule) -> Vec<Finding> {
    let syntax = syn::parse_file(source).unwrap_or_else(|e| panic!("{file}: {e}"));
    let mut scanner = Scanner {
        file,
        rule,
        items: Vec::new(),
        text_functions: 0,
        findings: Vec::new(),
    };
    scanner.visit_file(&syntax);
    let mut findings = scanner.findings;
    findings.sort();
    findings.dedup();
    findings
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The Rust files under `dir`, sorted, relative to the repository.
fn rust_files(dir: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let relative = path.strip_prefix(root).unwrap();
                let parts: Vec<String> = relative
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    let root = repository();
    let mut out = Vec::new();
    walk(&root, &root.join(dir), &mut out);
    out.sort();
    out
}

/// The scanned crates and how their literals are judged.
const SCANNED: &[(&str, Rule)] = &[
    ("crates/archicraft-app/src", Rule::Sinks),
    ("crates/archicraft-model/src", Rule::AllText),
];

fn repository_findings() -> Vec<Finding> {
    let mut findings = Vec::new();
    for (dir, rule) in SCANNED {
        for file in rust_files(dir) {
            let source = std::fs::read_to_string(repository().join(&file)).unwrap();
            findings.extend(scan(&file, &source, *rule));
        }
    }
    findings
}

/// A file or an item in a file: `path` or `path::Item::path`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Scope {
    file: String,
    item: Option<String>,
}

impl Scope {
    fn parse(text: &str) -> Self {
        match text.split_once(".rs::") {
            Some((file, item)) => Scope {
                file: format!("{file}.rs"),
                item: Some(item.to_owned()),
            },
            None => Scope {
                file: text.to_owned(),
                item: None,
            },
        }
    }

    fn contains(&self, finding: &Finding) -> bool {
        self.file == finding.file
            && self.item.as_ref().is_none_or(|item| {
                finding.item == *item
                    || finding
                        .item
                        .strip_prefix(item.as_str())
                        .is_some_and(|rest| rest.starts_with("::"))
            })
    }
}

impl Scope {
    /// Whether this scope covers all of `other`.
    fn contains_scope(&self, other: &Scope) -> bool {
        self.file == other.file
            && match (&self.item, &other.item) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(mine), Some(theirs)) => {
                    theirs == mine
                        || theirs
                            .strip_prefix(mine.as_str())
                            .is_some_and(|rest| rest.starts_with("::"))
                }
            }
    }
}

impl std::fmt::Display for Scope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.item {
            Some(item) => write!(f, "{}::{item}", self.file),
            None => write!(f, "{}", self.file),
        }
    }
}

/// A line of the allowlist: `scope | "literal" | reason`, where the literal
/// is written as Rust prints it with `{:?}`, or `*` for every literal in the
/// scope.
#[derive(Clone, Debug)]
struct Allowed {
    line: usize,
    scope: Scope,
    literal: Option<String>,
}

impl Allowed {
    fn matches(&self, finding: &Finding) -> bool {
        self.scope.contains(finding)
            && self
                .literal
                .as_ref()
                .is_none_or(|literal| *literal == format!("{:?}", finding.text))
    }
}

fn parse_allowlist(text: &str) -> Result<Vec<Allowed>, Vec<String>> {
    let mut entries = Vec::new();
    let mut errors = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line_number = i + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parsed = line.split_once(" | ").and_then(|(scope, rest)| {
            let (literal, reason) = rest.rsplit_once(" | ")?;
            let literal = literal.trim();
            let valid = literal == "*" || (literal.len() >= 2 && literal.starts_with('"') && literal.ends_with('"'));
            (valid && !reason.trim().is_empty()).then(|| Allowed {
                line: line_number,
                scope: Scope::parse(scope.trim()),
                literal: (literal != "*").then(|| literal.to_owned()),
            })
        });
        match parsed {
            Some(entry) => entries.push(entry),
            None => errors.push(format!(
                "string_lint_allow.txt:{line_number}: expected `scope | \"literal\" | reason` or `scope | * | reason`"
            )),
        }
    }
    if errors.is_empty() { Ok(entries) } else { Err(errors) }
}

/// Checks findings against the pending list and the allowlist; returns the
/// problems.
fn evaluate(findings: &[Finding], pending: &[Scope], allowed: &[Allowed]) -> Vec<String> {
    let mut problems = Vec::new();
    let mut used_allowed = BTreeSet::new();
    let mut used_pending = BTreeSet::new();
    let mut outside = Vec::new();
    for finding in findings {
        let allow = allowed.iter().position(|a| a.matches(finding));
        if let Some(i) = allow {
            used_allowed.insert(i);
            continue;
        }
        match pending.iter().position(|p| p.contains(finding)) {
            Some(i) => {
                used_pending.insert(i);
            }
            None => outside.push(finding),
        }
    }
    if !outside.is_empty() {
        problems.push(format!(
            "User-visible string literals outside the message catalogue (NFR-L10N-001). Move the text to \
             crates/archicraft-app/locales/en/*.ftl and use tr!, or, if it is not user-interface text, add it \
             with a reason to crates/archicraft-app/tests/string_lint_allow.txt:\n{}",
            outside.iter().map(|f| format!("  {f}")).collect::<Vec<_>>().join("\n")
        ));
    }
    for (i, scope) in pending.iter().enumerate() {
        if !used_pending.contains(&i) {
            problems.push(format!(
                "{scope} has no user-visible literals left: remove it from PENDING in string_lint.rs"
            ));
        }
    }
    for (i, entry) in allowed.iter().enumerate() {
        if !used_allowed.contains(&i) {
            problems.push(format!(
                "string_lint_allow.txt:{}: the entry matches no literal any more: remove it",
                entry.line
            ));
        }
    }
    problems
}

/// NFR-L10N-001: no user-visible literal outside the pending files, the
/// allowlist and the catalogue; every pending and allowlist entry still
/// matches a literal.
#[test]
fn no_user_visible_literals_outside_the_catalogue() {
    let allowed = parse_allowlist(ALLOWLIST).unwrap_or_else(|errors| panic!("{}", errors.join("\n")));
    let pending: Vec<Scope> = PENDING.iter().map(|p| Scope::parse(p)).collect();
    let findings = repository_findings();
    let problems = evaluate(&findings, &pending, &allowed);
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n\n"));
}

/// The About window and the Help menu title were the first strings moved to
/// the catalogue (archicraft-o4x.1); they must stay there (NFR-L10N-001).
#[test]
fn pilot_migration_is_not_pending() {
    let findings = repository_findings();
    let pending: Vec<Scope> = PENDING.iter().map(|p| Scope::parse(p)).collect();
    let about = Scope::parse("crates/archicraft-app/src/ui/dialogs.rs::Dialogs::about_window");
    assert!(!pending.iter().any(|p| p.contains_scope(&about)), "{about} is pending");
    let left: Vec<&Finding> = findings.iter().filter(|f| about.contains(f)).collect();
    assert!(left.is_empty(), "{left:?}");
    // The Help menu is part of the still pending menu bar, so its title is
    // checked by its old literal.
    let menu_bar = Scope::parse("crates/archicraft-app/src/ui/shell.rs::menu_bar");
    let help: Vec<&Finding> = findings
        .iter()
        .filter(|f| menu_bar.contains(f) && f.text == "Help")
        .collect();
    assert!(help.is_empty(), "{help:?}");
}

fn texts(findings: Vec<Finding>) -> Vec<String> {
    findings.into_iter().map(|f| f.text).collect()
}

/// NFR-L10N-001: in archicraft-app, literals passed to egui text sinks are
/// reported; `tr!`, egui `Id`s, log macros, attributes and test code are not.
#[test]
fn lint_reports_literals_passed_to_widgets() {
    let source = r#"
        use egui::{Id, RichText};

        pub fn show(ui: &mut egui::Ui, ws: &mut Workspace, path: &str) {
            ui.label("Hello world");
            ui.label(tr!("dlg-about-title"));
            ui.heading(tr!("dlg-about-heading", version = "1.0"));
            let id = Id::new("project-settings");
            ui.push_id(id, |ui| ui.label(RichText::new("Hint text").weak()));
            tracing::warn!("Something Went Wrong at {path}");
            ws.info(format!("Saved {path}"));
            ws.error(if path.is_empty() { "No path" } else { "Bad path" });
            ui.checkbox(&mut true, "");
            ui.label(format!("{path}"));
            egui::ComboBox::from_id_salt("display-mode").selected_text("Shaded".to_owned());
            let items = vec![ui.button("Inside a macro")];
        }

        impl Tool {
            pub fn label(self) -> &'static str {
                match self {
                    Tool::Wall => "Wall tool",
                    Tool::Slab => "",
                }
            }
            pub fn code(self) -> &'static str {
                match self {
                    Tool::Wall => "Wall",
                    Tool::Slab => "Slab",
                }
            }
        }

        #[derive(serde::Serialize, serde::Deserialize)]
        #[serde(rename = "Project Settings")]
        pub struct Settings {
            #[serde(rename = "Cut Plane", default)]
            pub cut: f64,
        }

        #[cfg(test)]
        mod tests {
            #[test]
            fn shows(ui: &mut egui::Ui) {
                ui.label("Only In A Test");
            }
        }
    "#;
    let findings = scan("example.rs", source, Rule::Sinks);
    assert_eq!(
        texts(findings.clone()),
        [
            "Hello world",
            "Hint text",
            "Saved {path}",
            "No path",
            "Bad path",
            "Shaded",
            "Inside a macro",
            "Wall tool"
        ]
    );
    assert_eq!(findings[0].line, 5);
    assert_eq!(findings[0].item, "show");
}

/// NFR-L10N-001: in archicraft-model every literal with a space or an
/// uppercase letter is reported, except in macros for logs, assertions,
/// panics and files, in attributes and in test code.
#[test]
fn lint_reports_text_in_the_model_crate() {
    let source = r#"
        #[derive(Debug, thiserror::Error, serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        pub enum CommandError {
            #[error("Nothing to change.")]
            NoChange,
            #[error("{0}")]
            Invalid(String),
        }

        impl Command {
            pub fn label(&self) -> String {
                format!("Create {} walls", 3)
            }
            pub fn key(&self) -> &'static str {
                "model-cmd-create-wall"
            }
        }

        fn check(x: Option<u8>) {
            tracing::info!("Loaded The Project");
            debug_assert!(x.is_some(), "Must Hold");
            let _ = x.expect("Checked Above");
            let _ = include_str!("Some File.txt");
            let _ = concat!("Some ", "Text");
            let _ = format!("{} {}", 1, 2);
        }

        macro_rules! id_type {
            ($name:ident) => {
                /// A New Identity.
                pub struct $name;
                impl $name { pub fn nil() -> Self { Self::from_str("Not A Doc Comment") } }
            };
        }

        #[cfg(test)]
        mod tests {
            const DATA: &str = "Test Data";
        }
    "#;
    let findings = scan("model.rs", source, Rule::AllText);
    assert_eq!(
        texts(findings.clone()),
        ["Nothing to change.", "Create {} walls", "Not A Doc Comment"]
    );
    assert_eq!(findings[1].item, "Command::label");
}

/// NFR-L10N-001: stale pending and allowlist entries fail, so both lists only
/// shrink, and allowlist lines without a quoted literal or a reason are
/// rejected.
#[test]
fn pending_and_allowlist_entries_must_match_something() {
    let finding = |file: &str, item: &str, text: &str| Finding {
        file: file.to_owned(),
        line: 1,
        column: 1,
        item: item.to_owned(),
        text: text.to_owned(),
    };
    let findings = [
        finding("a.rs", "Dialogs::storeys_window", "Storeys"),
        finding("b.rs", "default_project", "Ground floor"),
    ];
    let allowed =
        parse_allowlist("# comment\nb.rs::default_project | \"Ground floor\" | project data\nc.rs | * | stale entry\n")
            .unwrap();
    let pending = [Scope::parse("a.rs::Dialogs::storeys_window"), Scope::parse("d.rs")];
    let problems = evaluate(&findings, &pending, &allowed);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].starts_with("d.rs has no user-visible literals left"));
    assert!(problems[1].contains(":3: the entry matches no literal"));
    // Without its pending entry the literal is reported.
    let problems = evaluate(&findings, &[], &allowed[..1]);
    assert_eq!(problems.len(), 1);
    assert!(problems[0].contains("a.rs:1:1 (Dialogs::storeys_window) \"Storeys\""));
    // An item scope covers nested items but not items that share a prefix.
    let scope = Scope::parse("a.rs::Dialogs::storeys");
    assert!(!scope.contains(&findings[0]));
    assert!(Scope::parse("a.rs::Dialogs").contains(&findings[0]));
    assert!(parse_allowlist("a.rs | Storeys | no quotes\n").is_err());
    assert!(parse_allowlist("a.rs | \"Storeys\" |  \n").is_err());
}

/// archicraft-model returns message keys for all its user-visible text
/// (archicraft-9w3), so none of its files may be pending again
/// (NFR-L10N-001).
#[test]
fn no_model_file_is_pending() {
    let model: Vec<&&str> = PENDING
        .iter()
        .filter(|p| p.starts_with("crates/archicraft-model/"))
        .collect();
    assert!(model.is_empty(), "{model:?}");
}
