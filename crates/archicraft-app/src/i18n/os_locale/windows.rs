//! Number locale from the Windows user default locale (STR-010):
//! `GetLocaleInfoEx` honours the changes made in the Region settings.
#![allow(unsafe_code)]

use windows_sys::Win32::Globalization::{GetLocaleInfoEx, GetUserDefaultLocaleName, LOCALE_SDECIMAL, LOCALE_STHOUSAND};
use windows_sys::Win32::System::SystemServices::LOCALE_NAME_MAX_LENGTH;

use super::OsNumberLocale;

pub(super) fn detect() -> Option<OsNumberLocale> {
    let mut name = [0u16; LOCALE_NAME_MAX_LENGTH as usize];
    // SAFETY: `name` is a writable buffer of `LOCALE_NAME_MAX_LENGTH` UTF-16
    // units, the length passed; the call writes at most that many, including
    // the terminating null, and returns the count written or 0 on failure.
    let written = unsafe { GetUserDefaultLocaleName(name.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
    let tag = utf16_until_null(&name, written)?;
    let decimal = locale_info(LOCALE_SDECIMAL)?;
    let grouping = locale_info(LOCALE_STHOUSAND).unwrap_or_default();
    Some(OsNumberLocale::from_os(&tag, &decimal, &grouping))
}

/// One string value of the user default locale.
fn locale_info(lctype: u32) -> Option<String> {
    // LOCALE_SDECIMAL and LOCALE_STHOUSAND hold at most three characters
    // plus the terminating null.
    let mut buffer = [0u16; 16];
    // SAFETY: a null locale name is LOCALE_NAME_USER_DEFAULT; `buffer` is a
    // writable buffer of 16 UTF-16 units, the length passed; the call writes
    // at most that many, including the null, and returns the count written or
    // 0 on failure (also when the value does not fit).
    let written = unsafe { GetLocaleInfoEx(std::ptr::null(), lctype, buffer.as_mut_ptr(), buffer.len() as i32) };
    utf16_until_null(&buffer, written)
}

/// The text of the first `written` units of `buffer`, without the
/// terminating null; `None` when the call failed.
fn utf16_until_null(buffer: &[u16], written: i32) -> Option<String> {
    let len = usize::try_from(written).ok().filter(|&n| n > 0 && n <= buffer.len())?;
    String::from_utf16(&buffer[..len - 1]).ok()
}
