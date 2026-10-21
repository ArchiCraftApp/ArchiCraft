//! Number locale from `NSLocale.currentLocale` (STR-010).
//!
//! objc2-foundation declares these methods safe, so no `unsafe` is needed.

use objc2_foundation::NSLocale;

use super::OsNumberLocale;

pub(super) fn detect() -> Option<OsNumberLocale> {
    let locale = NSLocale::currentLocale();
    let tag = locale.localeIdentifier().to_string();
    if tag.is_empty() {
        return None;
    }
    Some(OsNumberLocale::from_os(
        &tag,
        &locale.decimalSeparator().to_string(),
        &locale.groupingSeparator().to_string(),
    ))
}
