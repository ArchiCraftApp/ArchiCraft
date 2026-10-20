# Release checklist

The manual and once-per-release checks that CI does not run. Work through it on the commit
that becomes the release, after CI and the
[conformance matrix](../../.github/workflows/conformance-matrix.yml) passed on it. Other epics
add their per-release items here (the AS-01 to AS-05 procedures, the licence gate); the release
workflow (epic release-candidate) extends it.

## Before you start

1. Note the release version and the full commit hash.
2. Start the conformance matrix on that commit if its last nightly run is older than the
   commit: `gh workflow run conformance-matrix.yml --ref <tag or branch>`. All three
   `Conformance (…)` jobs and `Same PASS lines on every OS` must pass.
3. Download the screenshots of that run, one artefact per OS (`viewer-screenshots-Linux`,
   `viewer-screenshots-Windows`, `viewer-screenshots-macOS`):
   `gh run download <run id> --pattern 'viewer-screenshots-*'`.
4. Write the reference models of the release commit:
   `cargo run --locked -p archicraft-ifcx --example reference_models -- target/reference-models`.

## Items

### 1. Hosted IFC5 viewer (IFX-006 b, SRS R4)

IFX-006 asks for a correct display in the hosted IFC5 viewer as well as in the pinned one; the
pinned viewer is checked by CI, the hosted one by hand, once per release.

For every golden file (`crates/archicraft-ifcx/tests/golden/*.ifcx`) and every reference model
(`target/reference-models/*.ifcx`, compact and `-indented`):

- [ ] Open it in the hosted IFC5 viewer (SRS R4); it loads without an error message.
- [ ] The model tree shows project → site → building → the storeys → the elements, with the
      names and the storey and element counts that the CI line `PASS (b2) viewer <file>` states.
- [ ] The geometry matches the CI screenshot of the same file (its repository path without
      `.ifcx`, `-` for `/`, such as `crates-archicraft-ifcx-tests-golden-walls-and-slab.png`):
      same elements, same placement on the storeys, no missing or extra faces.
- [ ] The colours match the CI screenshot: every material colour of the file is visible on its
      elements.

Note any difference with the file name and a screenshot of the hosted viewer. A difference
blocks the release until it is understood: either our file is wrong (a bug against IFX-006) or
the hosted viewer has moved past the pinned commit (record it, and check the pinned commit in
SRS R2 is still the right one).

### 2. One-hour fuzz run (NFR-REL-007, NFR-SEC-003, SRS D.9)

- [ ] _Placeholder: the fuzz campaign task (archicraft-lnow) replaces this item with a link to
      the document it writes._ Until then: each target of docs/testing.md "Fuzzing" ran for at
      least one hour on the seeded corpus without a finding (the release rule there).

### 3. AS-05 crash recovery on Windows, macOS and Linux

- [ ] _Placeholder: archicraft-ddhu replaces this item with a link to docs/test-plan/as-05.md,
      the manual procedure for the three operating systems._ Until then: run SRS AS-05 by hand
      on each operating system and record the outcome below.

### 4. Minimum and reference hardware (NFR-PERF-001, NFR-PORT-006)

- [ ] _Placeholder: archicraft-0ca2 replaces this item with a link to the hardware classes it
      records._ Until then: the release notes name the minimum and the reference hardware of
      NFR-PERF-001 and the graphics API used on each operating system.

### 5. No content or trademarks of the reference product's maker (NFR-LEG-002)

`scripts/check-legal.sh` (CI job "Repository guards") catches trademark text outside its
allowlist and image, library-part and template files without a listed origin. This item is the
inspection that a script cannot do:

- [ ] Every entry added to `scripts/legal-allowlist.txt` since the last release is a nominative
      reference ("imports files from …"), not a product name, slogan or UI label.
- [ ] Every entry added to `scripts/asset-allowlist.txt` since the last release names an origin
      and a licence that allow shipping it, and none of them comes from the reference product
      or its maker.
- [ ] Look at the images of the application (icons, splash, toolbar), the default template, the
      library parts and the documentation added since the last release: none of them is copied
      or traced from the reference product, its library or its templates, and no UI text is
      lifted from it.
- [ ] The product name check of SRS O-17 has been done before a public release.

Useful commands: `git diff <last release tag>.. -- scripts/legal-allowlist.txt
scripts/asset-allowlist.txt` and `git diff --stat <last release tag>.. -- '*.png' '*.svg'
'*.ico' docs/`.

### 6. Implementation status is current

- [ ] Every requirement whose status changed since the last release has its row in
      `docs/implementation-status.md` updated (status and gap text), and the summary at the top
      gives the current counts. Check the merged pull requests since the last release against
      the rows they cite.

## Record

Add one row per release; keep older rows.

| Release | Commit | Date | Tester | Conformance matrix run | 1 R4 | 2 Fuzz | 3 AS-05 | 4 Hardware | 5 NFR-LEG-002 | 6 Status | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|
