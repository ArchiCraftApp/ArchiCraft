//! User-visible text from message catalogues (NFR-L10N-001).
//!
//! Every string the user reads comes from a [Fluent](https://projectfluent.org)
//! catalogue under `crates/archicraft-app/locales/<language>/`, looked up by
//! key with the [`tr!`](crate::tr) macro. English is the default and the
//! fallback for every other language (NFR-L10N-001); a German catalogue can be
//! added beside it without touching the code (NFR-L10N-002).
//!
//! ```ignore
//! ui.label(tr!("dlg-about-summary"));
//! ui.heading(tr!("dlg-about-heading", version = env!("CARGO_PKG_VERSION")));
//! ```
//!
//! # The current localizer
//!
//! egui calls the UI on one thread, so the [`Localizer`] in use is a
//! thread-local: the application installs its localizer at the start of every
//! frame ([`install`]), and tests run code with another one with [`with`].
//! Parallel UI tests on different threads can therefore use different
//! languages without interfering.
//!
//! # Key convention
//!
//! Keys are kebab-case and start with the prefix of their area; each prefix
//! belongs to one catalogue file ([`catalogue::ENGLISH`]):
//!
//! | Prefix | Area | File |
//! |---|---|---|
//! | `menu-` | menu titles and items that are not commands | `app.ftl` |
//! | `status-` | status-line messages | `app.ftl` |
//! | `browser-` | project browser | `app.ftl` |
//! | `cmd-<command-id>-label`, `cmd-<command-id>-desc` | command labels and descriptions | `commands.ftl` |
//! | `dlg-<dialog>-` | dialogs, for example `dlg-about-title` | `dialogs.ftl` |
//! | `tool-`, `prompt-` | drawing tools and their input prompts | `tools.ftl` |
//! | `model-` | keys produced by archicraft-model (command labels, errors), rendered with [`render`] | `model.ftl` |
//!
//! Keys built at runtime must start with one of [`DYNAMIC_PREFIXES`], so the
//! key-usage test (`crates/archicraft-app/tests/i18n_keys.rs`) knows that a
//! catalogue key without a literal `tr!` call is not dead.
//!
//! # Arguments, numbers and plurals
//!
//! Arguments are named like Rust identifiers (`version`, `count`). Numbers the
//! user reads are never formatted by Fluent: they are formatted with the
//! user's number format first and passed as text, while a separate numeric
//! argument drives the plural selection with Fluent's CLDR plural rules:
//!
//! ```ftl
//! status-selected = { $count ->
//!         [one] { $count_text } element selected
//!        *[other] { $count_text } elements selected
//!     }
//! ```
//!
//! Fluent's Unicode isolation marks are switched off, because egui draws
//! U+2068 and U+2069 as missing glyphs.
//!
//! # What stays literal
//!
//! egui `Id`s and salts, test anchors, log messages, file-format keywords and
//! project data (default storey and material names) are not user interface
//! text and stay literal. Everything else the user reads goes through
//! [`tr!`](crate::tr).

pub mod catalogue;
pub mod model;
#[cfg(any(test, debug_assertions))]
pub mod pseudo;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

pub use fluent_bundle::FluentArgs;
use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use catalogue::CatalogueFile;
pub use model::render;

/// Prefixes of keys that are built at runtime, such as
/// `cmd-<command-id>-label` or the keys archicraft-model returns. Catalogue
/// keys with these prefixes need no literal `tr!` call.
pub const DYNAMIC_PREFIXES: &[&str] = &["model-", "cmd-"];

/// Looks up user-visible text in the current [`Localizer`] (NFR-L10N-001).
///
/// `tr!(key)` renders a message; `tr!(key, name = value, …)` passes
/// arguments, which can be text or numbers (numbers select plural forms).
/// The key is normally a literal; a key built at runtime must start with one
/// of [`DYNAMIC_PREFIXES`]. A missing key renders as the key itself and is
/// logged once.
#[macro_export]
macro_rules! tr {
    ($key:expr $(,)?) => {
        $crate::i18n::text($key)
    };
    ($key:expr, $($name:ident = $value:expr),+ $(,)?) => {{
        let mut args = $crate::i18n::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $crate::i18n::text_with($key, &args)
    }};
}

/// Renders messages of one language, falling back to English.
pub struct Localizer {
    language: LanguageIdentifier,
    /// The language's bundle first, then its fallbacks.
    bundles: Vec<FluentBundle<FluentResource>>,
    /// Wrap every message in the pseudo-locale markers.
    pseudo: bool,
    /// Keys already reported as missing or failing, so each is logged once.
    reported: RefCell<BTreeSet<String>>,
}

impl std::fmt::Debug for Localizer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Localizer")
            .field("language", &self.language.to_string())
            .field("pseudo", &self.pseudo)
            .finish_non_exhaustive()
    }
}

impl Default for Localizer {
    fn default() -> Self {
        Self::english()
    }
}

impl Localizer {
    /// The English user interface, the default (NFR-L10N-001).
    pub fn english() -> Self {
        Self::from_catalogue(catalogue::english(), catalogue::ENGLISH)
    }

    /// A localizer for `language` from the given files. Problems in the files
    /// are logged; whatever could be read is used.
    pub fn from_catalogue(language: LanguageIdentifier, files: &[CatalogueFile]) -> Self {
        let (bundle, problems) = catalogue::bundle(language.clone(), files);
        for problem in &problems {
            tracing::error!("message catalogue: {problem}");
        }
        Self {
            language,
            bundles: vec![bundle],
            pseudo: false,
            reported: RefCell::new(BTreeSet::new()),
        }
    }

    /// The en-XA pseudo-locale: English with accented vowels, every message
    /// wrapped in [`pseudo::OPEN`] … [`pseudo::CLOSE`]. Debug and test builds
    /// only; it drives the runtime string audits.
    #[cfg(any(test, debug_assertions))]
    pub fn pseudo() -> Self {
        let (bundle, problems) = pseudo::bundle();
        for problem in &problems {
            tracing::error!("message catalogue: {problem}");
        }
        Self {
            language: pseudo::language(),
            bundles: vec![bundle],
            pseudo: true,
            reported: RefCell::new(BTreeSet::new()),
        }
    }

    /// The language of the user interface.
    pub fn language(&self) -> &LanguageIdentifier {
        &self.language
    }

    /// Whether `key` is defined.
    pub fn has(&self, key: &str) -> bool {
        self.bundles.iter().any(|b| b.has_message(key))
    }

    /// Renders the message `key`. A missing key renders as the key itself,
    /// so the gap is visible, and is logged once.
    pub fn format(&self, key: &str, args: Option<&FluentArgs>) -> String {
        for bundle in &self.bundles {
            let Some(pattern) = bundle.get_message(key).and_then(|m| m.value()) else {
                continue;
            };
            let mut errors = Vec::new();
            let text = bundle.format_pattern(pattern, args, &mut errors);
            if !errors.is_empty() {
                let errors: Vec<String> = errors.iter().map(ToString::to_string).collect();
                self.report(key, &format!("cannot render it: {}", errors.join("; ")));
            }
            return if self.pseudo {
                self.wrap(&text)
            } else {
                text.into_owned()
            };
        }
        self.report(key, "the key is missing");
        key.to_owned()
    }

    /// The keys reported as missing or failing so far, for audits.
    pub fn reported_keys(&self) -> Vec<String> {
        self.reported.borrow().iter().cloned().collect()
    }

    /// Logs a problem with `key` the first time it occurs.
    fn report(&self, key: &str, problem: &str) {
        if self.reported.borrow_mut().insert(key.to_owned()) {
            tracing::warn!(key, language = %self.language, "message {key}: {problem}");
        }
    }

    #[cfg(any(test, debug_assertions))]
    fn wrap(&self, text: &str) -> String {
        pseudo::wrap(text)
    }

    #[cfg(not(any(test, debug_assertions)))]
    fn wrap(&self, text: &str) -> String {
        text.to_owned()
    }
}

thread_local! {
    static CURRENT: RefCell<Rc<Localizer>> = RefCell::new(Rc::new(Localizer::english()));
}

/// Makes `localizer` the current one on this thread. The application calls
/// this at the start of every frame.
pub fn install(localizer: Rc<Localizer>) {
    CURRENT.with(|current| *current.borrow_mut() = localizer);
}

/// The localizer in use on this thread (English unless another one was
/// installed).
pub fn current() -> Rc<Localizer> {
    CURRENT.with(|current| Rc::clone(&current.borrow()))
}

/// Runs `f` with `localizer` as the current one and restores the previous
/// localizer afterwards, also when `f` panics. For tests.
pub fn with<R>(localizer: Rc<Localizer>, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Rc<Localizer>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some(previous) = self.0.take() {
                install(previous);
            }
        }
    }
    let previous = CURRENT.with(|current| std::mem::replace(&mut *current.borrow_mut(), localizer));
    let _restore = Restore(Some(previous));
    f()
}

/// Renders `key` without arguments with the current localizer. Use
/// [`tr!`](crate::tr).
pub fn text(key: &str) -> String {
    current().format(key, None)
}

/// Renders `key` with arguments with the current localizer. Use
/// [`tr!`](crate::tr).
pub fn text_with(key: &str, args: &FluentArgs) -> String {
    current().format(key, Some(args))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tracing_subscriber::layer::{Context, SubscriberExt};

    use super::*;

    /// A message with plural forms, only for these tests.
    const TEST_FILE: CatalogueFile = CatalogueFile {
        name: "test.ftl",
        source: "test-elements = { $count ->\n        [one] { $count_text } element\n       *[other] { $count_text } elements\n    }\ntest-greeting = Hello { $name }, welcome to { -app }.\n-app = ArchiCraft\n",
        prefixes: &["test-"],
    };

    fn with_test_messages<R>(f: impl FnOnce() -> R) -> R {
        let mut files = catalogue::ENGLISH.to_vec();
        files.push(TEST_FILE);
        with(Rc::new(Localizer::from_catalogue(catalogue::english(), &files)), f)
    }

    #[test]
    fn every_catalogue_file_parses_without_errors() {
        for file in catalogue::ENGLISH {
            let (_, errors) = catalogue::parse(file);
            assert!(errors.is_empty(), "{errors:?}");
        }
        let (_, problems) = catalogue::bundle(catalogue::english(), catalogue::ENGLISH);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn no_key_is_defined_twice() {
        let mut seen: BTreeMap<String, &str> = BTreeMap::new();
        for file in catalogue::ENGLISH {
            for key in catalogue::keys(file) {
                if let Some(first) = seen.insert(key.clone(), file.name) {
                    panic!("{key} is defined in {first} and {}", file.name);
                }
            }
        }
        // The bundle rejects a second definition, too.
        let file = catalogue::ENGLISH
            .iter()
            .find(|f| !catalogue::keys(f).is_empty())
            .unwrap();
        let (_, problems) = catalogue::bundle(catalogue::english(), &[*file, *file]);
        assert_eq!(problems.len(), catalogue::keys(file).len(), "{problems:?}");
    }

    #[test]
    fn keys_follow_the_convention_of_their_file() {
        for file in catalogue::ENGLISH {
            for key in catalogue::keys(file) {
                assert!(
                    file.prefixes.iter().any(|p| key.starts_with(p)),
                    "{key} in {} must start with one of {:?}",
                    file.name,
                    file.prefixes
                );
                assert!(
                    key.chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{key} is not kebab-case"
                );
            }
        }
    }

    #[test]
    fn plural_forms_follow_the_count() {
        with_test_messages(|| {
            assert_eq!(tr!("test-elements", count = 1, count_text = "1"), "1 element");
            assert_eq!(tr!("test-elements", count = 3, count_text = "3"), "3 elements");
            assert_eq!(tr!("test-elements", count = 0, count_text = "0"), "0 elements");
            // The displayed number is the pre-formatted text, not Fluent's.
            assert_eq!(
                tr!("test-elements", count = 1500, count_text = "1 500"),
                "1 500 elements"
            );
        });
    }

    #[test]
    fn rendered_text_has_no_isolation_marks() {
        with_test_messages(|| {
            let text = tr!("test-greeting", name = "Ada");
            assert_eq!(text, "Hello Ada, welcome to ArchiCraft.");
            let heading = tr!("dlg-about-heading", version = "1.2.3");
            assert_eq!(heading, "ArchiCraft 1.2.3");
            for s in [text, heading] {
                assert!(!s.contains(['\u{2068}', '\u{2069}']), "{s:?}");
            }
        });
    }

    /// Counts the warnings logged while it is the default subscriber.
    struct CountWarnings(Arc<AtomicUsize>);

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for CountWarnings {
        fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
            if *event.metadata().level() == tracing::Level::WARN {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
    }

    #[test]
    fn a_missing_key_is_visible_and_logged_once() {
        // tracing caches each callsite's interest globally. While a single
        // dispatcher is registered, a callsite first hit on another test
        // thread asks only that thread's (no-op) default and caches "never",
        // which hides the warning from this thread's subscriber. A second
        // live dispatcher makes every registration ask all of them, ours too.
        let _second_dispatcher = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
        let warnings = Arc::new(AtomicUsize::new(0));
        let subscriber = tracing_subscriber::registry().with(CountWarnings(Arc::clone(&warnings)));
        let localizer = Rc::new(Localizer::english());
        tracing::subscriber::with_default(subscriber, || {
            // What a parallel test may do first: report a problem on a thread
            // without a subscriber.
            std::thread::spawn(|| Localizer::english().format("dlg-other-thread", None))
                .join()
                .unwrap();
            with(Rc::clone(&localizer), || {
                assert_eq!(tr!("dlg-no-such-message"), "dlg-no-such-message");
                assert_eq!(tr!("dlg-no-such-message", count = 2), "dlg-no-such-message");
            });
        });
        assert_eq!(warnings.load(Ordering::SeqCst), 1);
        assert_eq!(localizer.reported_keys(), ["dlg-no-such-message"]);
        assert!(!localizer.has("dlg-no-such-message"));
    }

    #[test]
    fn a_missing_argument_is_visible_and_logged() {
        let localizer = Rc::new(Localizer::english());
        let text = with(Rc::clone(&localizer), || tr!("dlg-about-heading"));
        assert!(text.starts_with("ArchiCraft"), "{text}");
        assert_eq!(localizer.reported_keys(), ["dlg-about-heading"]);
    }

    #[test]
    fn with_restores_the_previous_localizer() {
        assert_eq!(current().language().to_string(), "en");
        let result = std::panic::catch_unwind(|| {
            with(Rc::new(Localizer::pseudo()), || {
                assert_eq!(current().language().to_string(), "en-XA");
                panic!("inside");
            })
        });
        assert!(result.is_err());
        assert_eq!(current().language().to_string(), "en");
    }

    #[test]
    fn the_pseudo_locale_marks_every_message() {
        with(Rc::new(Localizer::pseudo()), || {
            let title = tr!("dlg-about-title");
            assert_eq!(title, "⟦Ábóút ÁrchíCráft⟧");
            // Arguments are passed through unchanged.
            let heading = tr!("dlg-about-heading", version = "0.1.0");
            assert_eq!(heading, "⟦ÁrchíCráft 0.1.0⟧");
            assert!(!heading.contains(['\u{2068}', '\u{2069}']));
        });
        assert_eq!(tr!("dlg-about-title"), "About ArchiCraft");
    }

    #[test]
    fn threads_have_their_own_localizer() {
        with(Rc::new(Localizer::pseudo()), || {
            let other = std::thread::spawn(|| tr!("menu-help")).join().unwrap();
            assert_eq!(other, "Help");
            assert_eq!(tr!("menu-help"), "⟦Hélp⟧");
        });
    }

    #[test]
    fn install_replaces_the_current_localizer() {
        let previous = current();
        install(Rc::new(Localizer::pseudo()));
        assert_eq!(current().language().to_string(), "en-XA");
        install(previous);
        assert_eq!(tr!("menu-help"), "Help");
    }
}
