//! Bundled copies of the standard schema files (IFX-004, Table 5-2).

use std::sync::OnceLock;

use crate::container::IfcxFile;

pub const IFC_CORE: &str = "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/ifc@v5a.ifcx";
pub const IFC_PROP: &str = "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/prop@v5a.ifcx";
pub const IFC_MAT: &str = "https://ifcx.dev/@standards.buildingsmart.org/ifc/ifc-mat/ifc-mat@v1.0.0.ifcx";
pub const USD: &str = "https://ifcx.dev/@openusd.org/usd@v1.ifcx";

const FILES: [(&str, &str); 4] = [
    (IFC_CORE, include_str!("../schemas/ifc@v5a.ifcx")),
    (IFC_PROP, include_str!("../schemas/prop@v5a.ifcx")),
    (IFC_MAT, include_str!("../schemas/ifc-mat@v1.0.0.ifcx")),
    (USD, include_str!("../schemas/usd@v1.ifcx")),
];

fn parsed() -> &'static [(&'static str, Option<IfcxFile>)] {
    static PARSED: OnceLock<Vec<(&'static str, Option<IfcxFile>)>> = OnceLock::new();
    PARSED.get_or_init(|| {
        FILES
            .iter()
            .map(|(uri, text)| (*uri, serde_json::from_str(text).ok()))
            .collect()
    })
}

/// The bundled schema file for an import URI, if ArchiCraft ships it.
pub fn bundled(uri: &str) -> Option<&'static IfcxFile> {
    parsed()
        .iter()
        .find(|(u, _)| *u == uri)
        .and_then(|(_, file)| file.as_ref())
}

/// The raw text of every bundled schema file, by import URI.
pub fn bundled_sources() -> impl Iterator<Item = (&'static str, &'static str)> {
    FILES.iter().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_files_parse_and_declare_the_written_attributes() {
        for (uri, _) in FILES {
            assert!(bundled(uri).is_some(), "{uri}");
        }
        assert!(bundled(IFC_CORE).unwrap().schemas.contains_key("bsi::ifc::class"));
        assert!(bundled(IFC_PROP).unwrap().schemas.contains_key("bsi::ifc::prop::Name"));
        assert!(bundled(USD).unwrap().schemas.contains_key("usd::usdgeom::mesh"));
    }

    /// IFX-024, O-6: the pinned ifc@v5a and prop@v5a schemas have no
    /// standard attribute for predefined types, neither as an attribute of
    /// its own nor as a member of `bsi::ifc::class`, so ArchiCraft keeps
    /// them in its own namespace only. When a newer pin declares one, this
    /// test fails: write the type with the standard attribute too (IFX-024).
    #[test]
    fn the_pinned_schemas_have_no_predefined_type_attribute() {
        for uri in [IFC_CORE, IFC_PROP] {
            let file = bundled(uri).unwrap();
            for key in file.schemas.keys() {
                assert!(!key.to_lowercase().contains("predefined"), "{uri} declares {key}");
            }
            let (_, text) = FILES.iter().find(|(u, _)| *u == uri).unwrap();
            assert!(
                !text.to_lowercase().contains("predefined"),
                "{uri} mentions a predefined type"
            );
        }
        let class = &bundled(IFC_CORE).unwrap().schemas["bsi::ifc::class"];
        let members: Vec<&str> = class["value"]["objectRestrictions"]["values"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(members, ["code", "uri"]);
    }
}
