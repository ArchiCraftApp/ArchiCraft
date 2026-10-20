# IFCX evolution

How ArchiCraft adopts a successor of `ifcx_alpha` once buildingSMART publishes one (SRS CON-2,
IFX-090, IFX-091, NFR-MNT-004, O-1). Until then v1 stays on the versions pinned in SRS Table
5-2. Two kinds of change are kept apart:

- an **IFCX version** (`header.ifcxVersion`, today `ifcx_alpha`) is buildingSMART's container
  and standard schemas. A new one gets its own adapter module in `crates/archicraft-ifcx/src/`.
- an **ArchiCraft format version** (`formatVersion` in `archicraft::project`, `FORMAT_VERSION`
  in `alpha/schema.rs`) is the shape of ArchiCraft's own data inside one IFCX version. A change
  there is a step in `alpha/migrate.rs` and a new frozen fixture in
  `crates/archicraft-ifcx/tests/formats/` (docs/testing.md); it needs no new adapter.

## The extension point

`archicraft_ifcx::load` parses the container, looks up `header.ifcxVersion` in the adapter table
`ADAPTERS` (`crates/archicraft-ifcx/src/lib.rs`) and hands the file to that adapter's reader.
`ReadOutcome::source_version` names the version the file was read as. `save` always writes
`WRITTEN_VERSION`, the one version a release writes (IFX-090).

The cargo feature `successor-stub` proves the extension point without a published successor:
`crates/archicraft-ifcx/src/successor/` registers the mock version `ifcx_successor_mock`, whose
reader maps the header and delegates to `alpha::reader`. `tests/successor.rs` checks that a
golden file relabelled as the mock version routes to it and loads to the same project, that an
unknown version still fails naming every supported one and that saving writes `ifcx_alpha`.
None of it touches `archicraft-model` or `archicraft-geometry`, which do not depend on the IFCX
crate (`scripts/check-layering.sh`). Run it with

```sh
cargo test -p archicraft-ifcx --features successor-stub
cargo clippy -p archicraft-ifcx --all-targets --features successor-stub -- -D warnings
```

The feature is not in `default` and no release enables it. CI's Test job runs both commands
(`.github/workflows/ci.yml`).

## Runbook for a real successor

Call the new version `ifcx_beta` below; use the name buildingSMART publishes.

1. **Adapter module.** Copy `crates/archicraft-ifcx/src/alpha/` to `src/beta/` and set
   `IFCX_VERSION` in `beta/writer.rs` to the new version. Port the reader, writer and
   `beta/schema.rs` to the new container and standard schemas. The `archicraft::` namespace keeps
   its `FORMAT_VERSION` unless its own shapes change too. Add `pub mod beta;` to `lib.rs` and an
   `Adapter` entry for it to `ADAPTERS` after the alpha entry. Remove the mock successor
   (`src/successor/`, the feature and `tests/successor.rs`) in the same change, since the real
   adapter now proves the extension point.
2. **Keep the alpha reader.** `alpha/` stays with its reader, its migration chain and its entry
   in `ADAPTERS`, so every file an earlier release wrote keeps opening (IFX-090). Its writer may
   go once nothing calls it; the frozen fixtures in `tests/formats/` keep testing the reader.
3. **Table 5-2 pins and bundled schemas.** Update SRS Table 5-2 with the new import URIs (a
   decision for the user, as an SRS change). Add the new schema files to
   `crates/archicraft-ifcx/schemas/` with their source commit in `schemas/README.md`, and their
   URIs and `include_str!` entries to `src/registry.rs`. Keep the alpha files: old files import
   them. Regenerate `schemas/ifcx-container.schema.json` from the new `ifcx.tsp` if the container
   changed (the command is in `schemas/README.md`).
4. **Conformance pin.** Move `IFC5_COMMIT` in `conformance/run.sh` (and the commit in
   `conformance/README.md` and SRS R2) to the IFC5-development commit that defines the successor,
   and adapt `check.ts`, `tree-check.ts` and the viewer harness where the reference code changed.
5. **Writer switch.** Point `WRITTEN_VERSION` and `save` in `lib.rs` at the new writer. From then
   on the release writes only the new version; the About dialog shows it
   (`dlg-about-format`).
6. **Migration notice and overwrite confirmation.** A file read with a `source_version` other
   than `WRITTEN_VERSION` is converted on its next save. The app (`crates/archicraft-app/src/app.rs`,
   `open_path` and `save`) must say so when it opens such a file, and ask before the first save
   overwrites it in the new version, the way `Prompt::OverwriteNewer` guards files of a newer
   format (IFX-062). Add the message to `crates/archicraft-app/locales/` and a kittest for both.
7. **Frozen fixture.** Copy each golden file as the last alpha release wrote it into
   `tests/formats/` (for example `alpha-format-1.ifcx`), list it in
   `frozen_format_fixtures_never_change` and test that it loads without problems and saves in the
   new version.
8. **Golden files.** On a bead labelled `format-change`, re-bless the golden and corrupt files
   (`ARCHICRAFT_BLESS=1 cargo test -p archicraft-ifcx --test roundtrip` and `--test corrupt`),
   review the diff through the review-view snapshots and run conformance on them (docs/testing.md).
9. **Status.** Update the IFX-090, IFX-091 and NFR-MNT-004 rows of
   `docs/implementation-status.md` and the extension point there.

Check before merging: `cargo tree -p archicraft-model` and `cargo tree -p archicraft-geometry`
are unchanged, and the diff touches neither crate (NFR-MNT-004).

## Publishing the `archicraft::` schema

SRS IFX-092 allows publishing the ArchiCraft schema on ifcx.dev under its own namespace. It is
optional and not done yet; the code is ready for it:

- `alpha::schema::document(timestamp)` (`crates/archicraft-ifcx/src/alpha/schema.rs`) is a
  standalone, schema-only IFCX document in the shape of the bundled standard schemas: the
  declarations of `schema::declarations()`, no imports, no data, `header.id` `archicraft` and
  `header.dataVersion` the `FORMAT_VERSION` it describes.
- The example `archicraft_schema` writes it, indented:
  `cargo run -p archicraft-ifcx --example archicraft_schema -- archicraft@v1.ifcx [timestamp]`.
  Without a timestamp (ISO 8601 UTC) the header gets the current time; pass the published one to
  reproduce a published file byte for byte.
- `validate::validate_with(file, extra)` resolves imports from extra schema documents before the
  bundled ones. `tests/schema_publication.rs` uses it to check that each golden file, with its
  in-file `archicraft::` schemas replaced by an import of the document under the placeholder URI
  `https://ifcx.dev/@archicraft/archicraft@v1.ifcx`, validates without problems.
  `registry.rs` stays the production list of bundled imports.

The writer keeps declaring the schema in-file, so every saved file validates on its own (IFX-005,
IFX-061). Publishing adds a reference copy for other tools; it does not replace the in-file
declarations.

### Publication steps

1. **Namespace.** ifcx.dev paths start with a namespace named after the publisher's domain
   (`@standards.buildingsmart.org`, `@openusd.org`). Choose the namespace from a domain the
   project controls (a decision for the user); `@archicraft` is only the test's placeholder. The
   file name carries the format version: `<namespace>/archicraft@v<FORMAT_VERSION>.ifcx`.
2. **Licence of the schema text.** The licence of the published file is decided by the user
   together with the distribution model (SRS O-5). The repository's licence today is in
   `Cargo.toml` (`license` of the workspace). Choose a licence that lets other tools copy the
   declarations into their files (the in-file declarations of every saved file are copies of it),
   and state it in the publication request and in `crates/archicraft-ifcx/schemas/README.md`.
3. **Generate.** Run the example for the current `FORMAT_VERSION` with a fixed timestamp, and
   keep the command line with the timestamp in the publication request.
4. **Check.** Set the URI in `tests/schema_publication.rs` to the chosen one and run
   `cargo test -p archicraft-ifcx --test schema_publication`. `conformance/run.sh` does not apply
   to the schema document: its checks expect a model with one root node, and a schema-only file
   has none (the bundled standard schemas have none either).
5. **Publish.** ifcx.dev serves the repository buildingSMART/IFCx.dev; open a pull request there
   that adds the file under the namespace, following that repository's contribution rules.
6. **Record.** Add the published URI to SRS R3 (an SRS change for the user) and update the IFX-092
   row of `docs/implementation-status.md`. Bundling the published file in `schemas/` and
   `registry.rs` is needed only if the writer or the tests start importing it.

### Versioning

One published file per ArchiCraft format version, and a published file never changes. A change
of `FORMAT_VERSION` (a migration step in `alpha/migrate.rs`, see above) publishes
`archicraft@v<new>.ifcx` next to the old file, so files that name an old version keep resolving.
A successor IFCX version (the runbook above) that changes the declarations without a format
change publishes a new file under a name that also carries the IFCX version; agree the name with
the IFCx.dev maintainers then.
