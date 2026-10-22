//! IFCX parsing and validation on arbitrary bytes (NFR-SEC-003): the
//! container parser with the limits of SRS Appendix D.9, then the schema,
//! composition and reference checks, for files of any IFCX version.
//!
//! Both parsers run: `parse`, which holds attribute values as JSON trees,
//! and `parse_raw`, the parser of the open path, which keeps them as raw
//! text. They must accept the same inputs, reject the others with the same
//! message, and validation must find the same problems in both.

#![no_main]

use archicraft_ifcx::validate;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let tree = validate::parse(data);
    let raw = validate::parse_raw(data);
    match (tree, raw) {
        (Ok(tree), Ok(raw)) => assert_eq!(validate::validate(&raw), validate::validate(&tree)),
        (tree, raw) => assert_eq!(raw.err(), tree.err(), "the parsers disagree"),
    }
});
