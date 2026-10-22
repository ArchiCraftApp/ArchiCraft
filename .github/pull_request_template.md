## Summary

<!-- What changed and why, in a few sentences. -->

SRS: <!-- requirement IDs this change serves, separated by spaces, for example IFX-007 NFR-ACC-002 -->

## Checks

- [ ] `scripts/loop/gate.sh` passed on this tree
- [ ] Golden-file (`crates/*/tests/golden/`) and snapshot (`crates/*/tests/snapshots/`) diffs are reviewed: every changed line follows from the intended change, or there are none (docs/testing.md)
- [ ] IFCX output follows the IFX-007 / NFR-ACC-002 reading: standard-namespace coordinates (mesh and curve points, placements) are rounded to 1e-6 m, `archicraft::` parameters keep round-trip precision, or IFCX output is unchanged
