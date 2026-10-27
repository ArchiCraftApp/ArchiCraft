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

- [ ] Each target of [docs/testing.md, Fuzzing](../testing.md#fuzzing) ran for at least one
      hour on the release commit without a finding (its release rule):
      `gh workflow run fuzz.yml --ref <tag or branch> -f minutes=60`. Findings are fixed and
      promoted to `crates/archicraft-ifcx/tests/fuzz-regressions/` first.
- [ ] The run is recorded as a new row of the Campaigns table there, with the first campaign's
      row (archicraft-lnow) as the template: date, commit, each target's duration, executions
      and coverage, findings and fixes. Column "2 Fuzz" below links the job.

### 3. AS-05 crash recovery on Windows, macOS and Linux

- [ ] [AS-05 manual procedure](as-05.md) passed with the windowed release binary on Windows,
      macOS and Linux; column "3 AS-05" below records the outcome per operating system. CI runs
      the same scenario headless on all three (`as05_crash_recovery`).

### 4. Minimum and reference hardware (NFR-PERF-001, NFR-PORT-006)

- [ ] The release notes include README's [System requirements](../../README.md#system-requirements)
      section as it is on the release commit: the minimum and the reference class of
      NFR-PERF-001 and the graphics API on each operating system. The classes and the
      selection criteria of the Windows laptops are in
      [technology-choices](../technology-choices.md#hardware-classes).
- [ ] The section still matches the classes there; a change of class is a change of
      NFR-PERF-001 and needs the SRS owner first.

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
'*.ico' '*.icns' '*.webp' '*.tif' '*.tiff' docs/`.

### 6. Implementation status is current

- [ ] Every requirement whose status changed since the last release has its row in
      `docs/implementation-status.md` updated (status and gap text), and the summary at the top
      gives the current counts. Check the merged pull requests since the last release against
      the rows they cite.

### 7. Third-party notices (NFR-LEG-001)

- [ ] `scripts/third-party.sh` leaves `THIRD-PARTY.md` and `third-party/not-shipped.txt`
      unchanged on the release commit ([docs/testing.md, Third-party notices](../testing.md#third-party-notices)).
- [ ] Each package contains `LICENSE` and the notices: `ArchiCraft/THIRD-PARTY.md` in the
      Linux tarball, `ArchiCraft/THIRD-PARTY.txt` in the Windows zip and
      `ArchiCraft.app/Contents/Resources/THIRD-PARTY.md` in the macOS DMG.
- [ ] Every licence in the overview of `THIRD-PARTY.md` is on `deny.toml`'s allow-list or among
      its exceptions, or covered by a decision recorded in `docs/technology-choices.md`. Until
      archicraft-vczo.3 rechecks the font licences, Bitstream-Vera (egui's Hack font) and
      MIT-CMU (KDE protocol files of wayland-protocols-plasma) are neither; name them in the
      release notes.
- [ ] The licence of the bundled IFCX schema files is recorded in
      `crates/archicraft-ifcx/schemas/README.md`, or the release notes name it as open.

## Record

Add one row per release; keep older rows.

| Release | Commit | Date | Tester | Conformance matrix run | 1 R4 | 2 Fuzz | 3 AS-05 | 4 Hardware | 5 NFR-LEG-002 | 6 Status | 7 Notices | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
