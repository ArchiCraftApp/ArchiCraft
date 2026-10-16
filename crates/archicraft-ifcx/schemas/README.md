# Bundled IFCX schema files

ArchiCraft resolves IFCX imports from these local copies, so opening, validating
and saving never need the network (SRS IFX-004, ASM-1).

| File | Import URI (SRS Table 5-2) | Source |
|---|---|---|
| `ifc@v5a.ifcx` | `https://ifcx.dev/@standards.buildingsmart.org/ifc/core/ifc@v5a.ifcx` | [buildingSMART/IFCx.dev](https://github.com/buildingSMART/IFCx.dev) |
| `prop@v5a.ifcx` | `https://ifcx.dev/@standards.buildingsmart.org/ifc/core/prop@v5a.ifcx` | same |
| `ifc-mat@v1.0.0.ifcx` | `https://ifcx.dev/@standards.buildingsmart.org/ifc/ifc-mat/ifc-mat@v1.0.0.ifcx` | same |
| `usd@v1.ifcx` | `https://ifcx.dev/@openusd.org/usd@v1.ifcx` | same |

The four files are unmodified copies of the IFCx.dev repository at commit
`36010c0e1069043f964a7dff8367e3e910255ebb` (2025-11-19), the source of what
ifcx.dev serves. The IFCx.dev repository does not state a licence; check the
terms with buildingSMART before any public release.

`ifcx-container.schema.json` is a JSON Schema of the IFCX container, generated
from `schema/ifcx.tsp` in
[buildingSMART/IFC5-development](https://github.com/buildingSMART/IFC5-development)
at commit `1a63082` (the commit SRS R2 pins) with the TypeSpec compiler:

```sh
npm install @typespec/compiler@0.63.0 @typespec/json-schema@0.63.0
npx tsp compile ifcx.tsp --emit @typespec/json-schema \
  --option "@typespec/json-schema.file-type=json" \
  --option "@typespec/json-schema.emitAllModels=true" \
  --option "@typespec/json-schema.bundleId=ifcx.json"
```

It is used by tests only. Note that its `path` pattern (`</[A-Za-z0-9_/.:]+>`)
matches none of the official examples, whose paths are UUIDs; the reference
implementation does not enforce it, and the tests drop it before validating.
