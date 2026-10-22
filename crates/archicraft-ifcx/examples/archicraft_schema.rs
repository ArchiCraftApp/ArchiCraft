//! Writes the `archicraft::` schema as a standalone, schema-only IFCX
//! document, the file that would be published on ifcx.dev (IFX-092). The
//! publication steps are in docs/ifcx-evolution.md.
//!
//! The header timestamp is a required argument, ISO 8601 UTC with seconds
//! (`YYYY-MM-DDTHH:MM:SSZ`), so that a published version can be reproduced
//! byte for byte from its timestamp.
//!
//! Usage: `cargo run -p archicraft-ifcx --example archicraft_schema -- <out.ifcx> <timestamp>`

use std::path::PathBuf;

use archicraft_ifcx::alpha::schema;
use archicraft_ifcx::alpha::writer::to_bytes;

const USAGE: &str = "usage: archicraft_schema <out.ifcx> <timestamp YYYY-MM-DDTHH:MM:SSZ>";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(timestamp), None) = (args.next(), args.next(), args.next()) else {
        return Err(USAGE.into());
    };
    if !is_utc_timestamp(&timestamp) {
        return Err(format!("not a timestamp YYYY-MM-DDTHH:MM:SSZ: {timestamp}; {USAGE}").into());
    }
    let path = PathBuf::from(path);
    std::fs::write(&path, to_bytes(&schema::document(&timestamp), true))?;
    println!("{}", path.display());
    Ok(())
}

/// Whether `text` has the shape `YYYY-MM-DDTHH:MM:SSZ`, the header timestamp
/// of a published schema (IFX-092).
fn is_utc_timestamp(text: &str) -> bool {
    const SHAPE: &[u8] = b"dddd-dd-ddTdd:dd:ddZ";
    text.len() == SHAPE.len()
        && text.bytes().zip(SHAPE).all(|(byte, &expected)| match expected {
            b'd' => byte.is_ascii_digit(),
            _ => byte == expected,
        })
}

#[cfg(test)]
mod tests {
    use super::is_utc_timestamp;

    /// IFX-092: the published timestamp shape is accepted.
    #[test]
    fn accepts_an_iso_8601_utc_timestamp_with_seconds() {
        assert!(is_utc_timestamp("2026-10-20T08:15:00Z"));
    }

    /// IFX-092: anything else is a usage error, not a header.
    #[test]
    fn rejects_timestamps_of_another_shape() {
        for text in [
            "",
            "2026-10-20",
            "2026-10-20T08:15Z",
            "2026-10-20T08:15:00",
            "2026-10-20T08:15:00+00:00",
            "2026-10-20T08:15:00.000Z",
            "2026-10-20 08:15:00Z",
            "2026-1o-20T08:15:00Z",
            "out.ifcx",
        ] {
            assert!(!is_utc_timestamp(text), "{text}");
        }
    }
}
