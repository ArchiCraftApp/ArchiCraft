//! Writes the `archicraft::` schema as a standalone, schema-only IFCX
//! document, the file that would be published on ifcx.dev (IFX-092). The
//! publication steps are in docs/ifcx-evolution.md.
//!
//! The header timestamp is the second argument (ISO 8601 UTC) or, without
//! it, the current time, so that a published version can be reproduced
//! byte for byte from its timestamp.
//!
//! Usage: `cargo run -p archicraft-ifcx --example archicraft_schema -- <out.ifcx> [timestamp]`

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use archicraft_ifcx::alpha::schema;
use archicraft_ifcx::alpha::writer::to_bytes;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().ok_or("usage: archicraft_schema <out.ifcx> [timestamp]")?);
    let timestamp = args.next().unwrap_or_else(|| utc_now(SystemTime::now()));
    std::fs::write(&path, to_bytes(&schema::document(&timestamp), true))?;
    println!("{}", path.display());
    Ok(())
}

/// The time as ISO 8601 UTC with seconds, for example `2026-10-20T08:15:00Z`.
fn utc_now(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}
