// The four schema imports ArchiCraft writes, mapped to the bundled copies in
// crates/archicraft-ifcx/schemas, so every conformance step runs offline (SRS IFX-006).

export const SCHEMA_FILES: { [uri: string]: string } = {
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/ifc@v5a.ifcx": "ifc@v5a.ifcx",
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/core/prop@v5a.ifcx": "prop@v5a.ifcx",
    "https://ifcx.dev/@standards.buildingsmart.org/ifc/ifc-mat/ifc-mat@v1.0.0.ifcx": "ifc-mat@v1.0.0.ifcx",
    "https://ifcx.dev/@openusd.org/usd@v1.ifcx": "usd@v1.ifcx",
};
