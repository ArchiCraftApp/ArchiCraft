//! IFCX parsing and validation on arbitrary bytes (NFR-SEC-003): the
//! container parser with the limits of SRS Appendix D.9, then the schema,
//! composition and reference checks, for files of any IFCX version.

#![no_main]

use archicraft_ifcx::validate;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = validate::parse(data) {
        let _ = validate::validate(&file);
    }
});
