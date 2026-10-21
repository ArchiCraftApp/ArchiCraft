//! The operating system's number locale (STR-010): the decimal separator and
//! the digit grouping character that the "Automatic" preference follows.
//!
//! It is detected once per process ([`detected`]) and cached:
//!
//! - Windows: `GetLocaleInfoEx` with `LOCALE_SDECIMAL` and `LOCALE_STHOUSAND`
//!   of the user default locale, which honours changes made in the Region
//!   settings ([`windows`]).
//! - macOS: `NSLocale.currentLocale` (`decimalSeparator`,
//!   `groupingSeparator`), which honours the Language & Region settings
//!   ([`macos`]).
//! - Linux, and wherever the operating system gives no usable answer: the
//!   locale tag from `LC_ALL`, `LC_NUMERIC` or `LANG` (POSIX precedence for
//!   numbers), else from sys-locale, mapped through the CLDR table in
//!   [`cldr_numbers`](super::cldr_numbers).
//!
//! The decimal separator is always a point or a comma, the two separators
//! number input accepts; any other answer from the operating system falls
//! back to the CLDR table.

use std::sync::OnceLock;

pub use super::cldr_numbers::CLDR_VERSION;
use super::cldr_numbers::NUMBER_SYMBOLS;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Number symbols of a locale (STR-010).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OsNumberLocale {
    /// BCP 47 tag, for example `de-AT` ([`normalize_tag`]).
    pub tag: String,
    /// `'.'` or `','`.
    pub decimal: char,
    /// Digit grouping character, for example `'.'`, `'\u{A0}'` or `'’'`.
    pub grouping: char,
}

impl OsNumberLocale {
    /// The CLDR symbols of `tag` (any of `de-AT`, `de_AT.UTF-8`, `de-Latn-AT`):
    /// the exact entry, else the entry of language and region, else that of
    /// the language. An unknown tag gets a point and a comma, CLDR's root.
    pub fn from_tag(tag: &str) -> Self {
        let tag = normalize_tag(tag);
        let (decimal, grouping) = cldr_symbols(&tag).unwrap_or(('.', ','));
        Self { tag, decimal, grouping }
    }

    /// The symbols the operating system reports for `tag`. A decimal that is
    /// not a single point or comma, or a grouping that is not a single
    /// character distinct from the decimal, is replaced by the CLDR value.
    pub fn from_os(tag: &str, decimal: &str, grouping: &str) -> Self {
        let mut locale = Self::from_tag(tag);
        if let Some(d @ ('.' | ',')) = single_char(decimal) {
            locale.decimal = d;
        }
        match single_char(grouping) {
            Some(g) if g != locale.decimal && !g.is_alphanumeric() => locale.grouping = g,
            _ if locale.grouping == locale.decimal => {
                locale.grouping = if locale.decimal == ',' { '.' } else { ',' };
            }
            _ => {}
        }
        locale
    }
}

fn single_char(text: &str) -> Option<char> {
    let mut chars = text.chars();
    chars.next().filter(|_| chars.next().is_none())
}

/// The number locale of this process, detected on first use.
pub fn detected() -> &'static OsNumberLocale {
    static DETECTED: OnceLock<OsNumberLocale> = OnceLock::new();
    DETECTED.get_or_init(|| {
        let locale = detect();
        tracing::info!(
            "number locale {}: decimal {:?}, grouping {:?}",
            locale.tag,
            locale.decimal,
            locale.grouping
        );
        locale
    })
}

/// Asks the operating system for its number locale, without the cache.
pub fn detect() -> OsNumberLocale {
    #[cfg(target_os = "macos")]
    if let Some(locale) = macos::detect() {
        return locale;
    }
    #[cfg(windows)]
    if let Some(locale) = windows::detect() {
        return locale;
    }
    let tag = ["LC_ALL", "LC_NUMERIC", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .or_else(sys_locale::get_locale)
        .unwrap_or_default();
    OsNumberLocale::from_tag(&tag)
}

/// A POSIX locale name or BCP 47 tag as a BCP 47 tag: `de_AT.UTF-8@euro`
/// and `en_AT@rg=atzzzz` become `de-AT` and `en-AT`, `zh-hant_tw` becomes
/// `zh-Hant-TW`.
pub fn normalize_tag(raw: &str) -> String {
    let base = raw.trim().split(['.', '@']).next().unwrap_or_default();
    let subtags: Vec<String> = base
        .split(['_', '-'])
        .filter(|s| !s.is_empty())
        .enumerate()
        .map(|(i, s)| {
            let all = |f: fn(&char) -> bool| s.chars().all(|c| f(&c));
            if i == 0 {
                s.to_ascii_lowercase()
            } else if s.len() == 2 && all(char::is_ascii_alphabetic) || s.len() == 3 && all(char::is_ascii_digit) {
                s.to_ascii_uppercase()
            } else if s.len() == 4 && all(char::is_ascii_alphabetic) {
                s[..1].to_ascii_uppercase() + &s[1..].to_ascii_lowercase()
            } else {
                s.to_ascii_lowercase()
            }
        })
        .collect();
    subtags.join("-")
}

fn cldr_symbols(tag: &str) -> Option<(char, char)> {
    let lookup = |key: &str| {
        NUMBER_SYMBOLS
            .binary_search_by(|(t, _, _)| (*t).cmp(key))
            .ok()
            .map(|i| (NUMBER_SYMBOLS[i].1, NUMBER_SYMBOLS[i].2))
    };
    let mut subtags = tag.split('-');
    let language = subtags.next().filter(|l| !l.is_empty())?;
    let region = subtags.find(|s| s.len() == 2 || s.chars().all(|c| c.is_ascii_digit()));
    lookup(tag)
        .or_else(|| region.and_then(|r| lookup(&format!("{language}-{r}"))))
        .or_else(|| lookup(language))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols(tag: &str) -> (char, char) {
        let locale = OsNumberLocale::from_tag(tag);
        (locale.decimal, locale.grouping)
    }

    /// STR-010: the CLDR table gives each locale its own decimal separator
    /// and grouping character.
    #[test]
    fn cldr_table_gives_the_locale_symbols() {
        assert_eq!(symbols("de-DE"), (',', '.'));
        assert_eq!(symbols("de-AT"), (',', '\u{A0}'));
        assert_eq!(symbols("de-CH"), ('.', '’'));
        assert_eq!(symbols("fr-FR"), (',', '\u{202F}'));
        assert_eq!(symbols("en-US"), ('.', ','));
    }

    /// STR-010: an unknown tag, or none, gets CLDR's root symbols.
    #[test]
    fn unknown_tags_get_a_point_and_a_comma() {
        assert_eq!(symbols("xx-YY"), ('.', ','));
        assert_eq!(symbols("C"), ('.', ','));
        assert_eq!(symbols(""), ('.', ','));
    }

    /// STR-010: POSIX locale names and tags with a script or a region the
    /// table lacks find the entry of their language and region or language.
    #[test]
    fn posix_names_and_scripts_find_their_entry() {
        assert_eq!(OsNumberLocale::from_tag("de_AT.UTF-8@euro").tag, "de-AT");
        assert_eq!(symbols("de_AT.UTF-8@euro"), (',', '\u{A0}'));
        assert_eq!(symbols("de-Latn-CH"), ('.', '’'));
        assert_eq!(symbols("fr_SN"), (',', '\u{202F}'));
        assert_eq!(symbols("es-419"), ('.', ','));
        assert_eq!(normalize_tag("en_AT@rg=atzzzz"), "en-AT");
        assert_eq!(normalize_tag("zh-hant_tw"), "zh-Hant-TW");
    }

    /// The lookup is a binary search: the generated table must stay sorted.
    #[test]
    fn cldr_table_is_sorted_and_unique() {
        assert!(NUMBER_SYMBOLS.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(NUMBER_SYMBOLS.len() >= 80);
    }

    /// STR-010: the operating system's answer wins where it is usable.
    #[test]
    fn os_symbols_override_the_table_when_usable() {
        let custom = OsNumberLocale::from_os("de-AT", ".", "'");
        assert_eq!((custom.decimal, custom.grouping), ('.', '\''));
        let odd = OsNumberLocale::from_os("de-DE", "\u{66B}", "");
        assert_eq!((odd.decimal, odd.grouping), (',', '.'));
        let clash = OsNumberLocale::from_os("en-US", ",", ",");
        assert_eq!((clash.decimal, clash.grouping), (',', '.'));
    }
}
