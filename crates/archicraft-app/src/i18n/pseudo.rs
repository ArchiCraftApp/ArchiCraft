//! The en-XA pseudo-locale for string audits (NFR-L10N-001).
//!
//! Built at runtime from the English catalogue: the vowels of every
//! catalogue text get accents and every rendered message is wrapped in
//! [`OPEN`] … [`CLOSE`]. Text on screen without the markers did not come
//! from the catalogue, which is how the runtime audits find strings the
//! static checks cannot see. Arguments (names, numbers, paths) are
//! passed through unchanged. Debug and test builds only.

use std::borrow::Cow;

use fluent_bundle::{FluentBundle, FluentResource};
use unic_langid::LanguageIdentifier;

use super::catalogue::{self, CatalogueError};

/// Marks the start of every pseudo-localized message.
pub const OPEN: char = '⟦';
/// Marks the end of every pseudo-localized message.
pub const CLOSE: char = '⟧';

/// The pseudo-locale's language tag.
pub fn language() -> LanguageIdentifier {
    "en-XA".parse().unwrap_or_default()
}

/// The English catalogue with accented vowels.
pub fn bundle() -> (FluentBundle<FluentResource>, Vec<CatalogueError>) {
    let (mut bundle, problems) = catalogue::bundle(language(), catalogue::ENGLISH);
    bundle.set_transform(Some(accent));
    (bundle, problems)
}

/// Wraps a rendered message in the pseudo markers.
pub fn wrap(text: &str) -> String {
    format!("{OPEN}{text}{CLOSE}")
}

/// Accents the vowels of a text element of a message.
fn accent(text: &str) -> Cow<'_, str> {
    if text.chars().any(|c| accented(c) != c) {
        Cow::Owned(text.chars().map(accented).collect())
    } else {
        Cow::Borrowed(text)
    }
}

/// Latin-1 accented vowels, which egui's default font draws.
fn accented(c: char) -> char {
    match c {
        'a' => 'á',
        'e' => 'é',
        'i' => 'í',
        'o' => 'ó',
        'u' => 'ú',
        'y' => 'ý',
        'A' => 'Á',
        'E' => 'É',
        'I' => 'Í',
        'O' => 'Ó',
        'U' => 'Ú',
        'Y' => 'Ý',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accents_vowels_and_keeps_everything_else() {
        assert_eq!(accent("About ArchiCraft 1.0"), "Ábóút ÁrchíCráft 1.0");
        assert!(matches!(accent("ffs 42"), Cow::Borrowed(_)));
        assert_eq!(wrap("x"), "⟦x⟧");
    }
}
