#!/usr/bin/env python3
"""Tests of scripts/status.py, the generated implementation-status tables and their checks.

Each test builds a small repository in a temporary directory (an SRS with live and withdrawn
requirements, docs/testing.md with a traceability table, a test file, a workflow and the
status document with its markers) and runs generate, check or merge-check on it. The last
tests run check and generate on this repository itself. Python 3 standard library only:
python3 scripts/tests/test_status.py
"""

import copy
import importlib.util
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "status.py"
# No scripts/__pycache__ in the working tree from loading the module.
sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("status", SCRIPT)
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)

SRS = """# SRS

## 4. Functional requirements

### 4.1 Project and file management

- **PRJ-001** · M — A new project shall be created from a template.
- **PRJ-002** · S — Users shall be able to save a project as a template.
- **PRJ-003** · *withdrawn by D-11 (was C)* — Something nobody needs.

### 4.2 Storeys

- **STR-001** · M — The system shall maintain an ordered list of storeys.

## 6. Non-functional requirements

#### 6.10 Legal

- **NFR-LEG-002** · C — No trademarks.

## Appendix D — Defaults

- **PRJ-001** is mentioned here but this is not a requirement line.
"""

TESTING = """# Testing

## Done-requirement traceability

| Requirement | What is guarded | Regression check (CI job: step) |
|---|---|---|
| NFR-LEG-002 | No trademarks | Repository guards: "No unreviewed trademarks (NFR-LEG-002)" |

## Fuzzing
"""

# The traceability table without a guard for NFR-LEG-002.
UNTRACED_TESTING = TESTING.replace('Repository guards: "No unreviewed trademarks (NFR-LEG-002)"', "Inspection before each release")

WORKFLOW = """name: CI
jobs:
  guards:
    name: Repository guards
    steps:
      - uses: actions/checkout@v4
      - name: Install the browser and its libraries
        run: npx playwright install --with-deps chromium
      - name: No unreviewed trademarks (NFR-LEG-002)
        run: scripts/check-legal.sh
      - name: Unit tests of the demo crate
        run: cargo test -p demo --locked  # scripts/other-tool.sh would be slower
      - run: python3 scripts/tests/test_unnamed.py
      - name: Benches of the demo crate
        env:
          MINUTES: 1
        run: |
          # A block scalar: the script is called on a continuation line, not scripts/other-tool.sh.
          scripts/bench-demo.sh "$MINUTES"
# A comment at column 0 between two jobs.
  licences:
    name: Licences and sources
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check bans
  browser:
    name: Browser installation only
    steps:
      - name: Install the browser once more
        run: npx playwright install chromium
  status:
    name: Implementation status
    steps:
      - name: Tests of the status script
        run: python3 scripts/tests/test_status.py
      - name: Every done row traced
        run: python3 scripts/status.py check
"""

# Workflows that notify or report but check nothing (archicraft-jmlb).
ANNOUNCE_WORKFLOW = """name: Announce
jobs:
  signal:
    name: Signal group
    steps:
      - name: Announce the new changes
        run: scripts/announce-signal.sh
"""

USER_REVIEW_WORKFLOW = """name: User review
jobs:
  user-review:
    name: User review check
    steps:
      - name: Report whether the change waits for the user's review
        run: scripts/loop/ci-user-review.sh
"""

TESTS_RS = """#[test]
fn prj_001_a_project_comes_from_the_template() {}

fn helper_outside_a_test_attribute() {}
"""

COMMON_RS = """pub fn common_setup_helper() {}
"""

SOURCE_RS = """pub fn template() {}

pub fn template_helper() {}

#[cfg(test)]
mod tests {
    #[test]
    // A comment between the attribute and the function.
    fn str_001_storeys_are_ordered() {}
}
"""

DOC = """# Status

_Intro._

## Counts

<!-- status:begin counts -->
<!-- status:end counts -->

## Per section

<!-- status:begin sections -->
<!-- status:end sections -->

## Requirement table

<!-- status:begin requirements -->
<!-- status:end requirements -->
"""


def row(rid, priority, section, state="missing", effort="M", evidence="", gap="Nothing yet."):
    if state == "done":
        effort, gap = "none", ""
    return {
        "id": rid,
        "priority": priority,
        "section": section,
        "status": state,
        "remaining_effort": effort,
        "evidence": evidence,
        "gap": gap,
        "depends_on": [],
    }


ROWS = [
    row("PRJ-001", "M", "4.1", "done", evidence="Done: crates/demo/src/lib.rs:1 template(), test prj_001_a_project_comes_from_the_template"),
    row("PRJ-002", "S", "4.1", "partial", "S", gap="No template store | no choice."),
    row("STR-001", "M", "4.2", "done", evidence="Done: str_001_storeys_are_ordered (crates/demo/src/lib.rs)"),
    row("NFR-LEG-002", "C", "6.10", "done", evidence="Done. Guarded by scripts/check-legal.sh."),
]


class Repo:
    """A minimal repository with the files status.py reads."""

    def __init__(self, root, rows=ROWS):
        self.root = Path(root)
        files = {
            status.SRS_PATH: SRS,
            status.TESTING_PATH: TESTING,
            status.DOC_PATH: DOC,
            ".github/workflows/ci.yml": WORKFLOW,
            ".github/workflows/announce.yml": ANNOUNCE_WORKFLOW,
            ".github/workflows/user-review.yml": USER_REVIEW_WORKFLOW,
            "scripts/announce-signal.sh": "#!/bin/sh\n",
            "scripts/loop/ci-user-review.sh": "#!/bin/sh\n",
            "scripts/status.py": "",
            "crates/demo/tests/demo.rs": TESTS_RS,
            "crates/demo/src/lib.rs": SOURCE_RS,
            "crates/demo/tests/common/mod.rs": COMMON_RS,
            "scripts/check-legal.sh": "#!/bin/sh\n",
            "scripts/bench-demo.sh": "#!/bin/sh\n",
            "scripts/tests/test_unnamed.py": "",
            "scripts/other-tool.sh": "#!/bin/sh\n",
        }
        for path, text in files.items():
            self.write(path, text)
        self.set_rows(rows)

    def write(self, path, text):
        (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        (self.root / path).write_text(text, encoding="utf-8")

    def read(self, path):
        return (self.root / path).read_text(encoding="utf-8")

    def set_rows(self, rows):
        self.write(status.JSON_PATH, status.dump_rows(rows))

    def rows(self):
        return status.load_rows(self.read(status.JSON_PATH))

    def run(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = status.main(["--root", str(self.root), *args])
        return code, out.getvalue() + err.getvalue()

    def git(self, *args):
        return subprocess.run(
            # No hooks or signing of the user's configuration: a plain, fast scratch repository.
            ["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.hooksPath=/dev/null",
             "-c", "commit.gpgsign=false", *args],
            cwd=self.root,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()

    def commit(self, message):
        self.git("add", "-A")
        self.git("commit", "-q", "--allow-empty", "-m", message)
        return self.git("rev-parse", "HEAD")


class StatusTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.repo = Repo(self.tmp.name)
        code, out = self.repo.run("generate")
        self.assertEqual(code, 0, out)

    def tearDown(self):
        self.tmp.cleanup()

    def check(self):
        return self.repo.run("check")

    def assert_check_fails(self, needle):
        code, out = self.check()
        self.assertEqual(code, 1, out)
        self.assertIn(needle, out)

    def edit(self, rid, **changes):
        rows = self.repo.rows()
        for r in rows:
            if r["id"] == rid:
                r.update(changes)
        self.repo.set_rows(rows)
        self.repo.run("generate")


class GenerateTest(StatusTest):
    def test_generate_writes_counts_sections_and_rows_between_the_markers(self):
        doc = self.repo.read(status.DOC_PATH)
        self.assertIn("| done | 2 | 0 | 1 | 3 |", doc)
        self.assertIn("| partial | 0 | 1 | 0 | 1 |", doc)
        self.assertIn("| **total** | 2 | 1 | 1 | 4 |", doc)
        self.assertIn("Remaining effort of the 1 open requirements: S 1, M 0, L 0, XL 0.", doc)
        self.assertIn("| 4.1 | Project and file management | 1/0/0 | 0/1/0 | 0/0/0 | 2 |", doc)
        self.assertIn("| 6.10 | Legal | 0/0/0 | 0/0/0 | 1/0/0 | 1 |", doc)
        self.assertIn("### 4.2 Storeys", doc)
        # The gap column escapes pipes; done rows show their evidence.
        self.assertIn("| PRJ-002 | S | partial | S | No template store \\| no choice. |", doc)
        self.assertIn("| STR-001 | M | done | none | Done: str_001_storeys_are_ordered", doc)
        self.assertIn("_Intro._", doc)
        self.assertEqual(doc.count(status.GENERATED_NOTE), 3)

    def test_rows_follow_the_srs_order_whatever_the_json_order(self):
        self.repo.set_rows(list(reversed(ROWS)))
        self.repo.run("generate")
        doc = self.repo.read(status.DOC_PATH)
        self.assertLess(doc.index("| PRJ-001 |"), doc.index("| PRJ-002 |"))
        self.assertLess(doc.index("| PRJ-002 |"), doc.index("| STR-001 |"))

    def test_generate_is_byte_stable(self):
        before = self.repo.read(status.DOC_PATH)
        self.repo.run("generate")
        self.assertEqual(self.repo.read(status.DOC_PATH), before)

    def test_a_missing_marker_is_an_error(self):
        self.repo.write(status.DOC_PATH, DOC.replace("<!-- status:end sections -->\n", ""))
        code, out = self.repo.run("generate")
        self.assertEqual(code, 2, out)
        self.assertIn("status:begin sections", out)


class CheckTest(StatusTest):
    def test_check_passes_on_a_consistent_repository(self):
        code, out = self.check()
        self.assertEqual(code, 0, out)

    def test_check_fails_on_a_live_srs_id_without_a_row(self):
        self.repo.set_rows([r for r in ROWS if r["id"] != "PRJ-002"])
        self.repo.run("generate")
        self.assert_check_fails("PRJ-002: live SRS requirement without a row")

    def test_check_fails_on_a_row_for_a_withdrawn_id(self):
        self.repo.set_rows([*ROWS, row("PRJ-003", "C", "4.1")])
        self.assert_check_fails("PRJ-003: row for a withdrawn requirement")

    def test_check_fails_on_a_row_for_an_unknown_id(self):
        self.repo.set_rows([*ROWS, row("PRJ-099", "M", "4.1")])
        self.assert_check_fails("PRJ-099: row for an ID that is not in docs/SRS.md")

    def test_check_fails_on_a_duplicate_row(self):
        self.repo.set_rows([*ROWS, ROWS[1]])
        self.assert_check_fails("PRJ-002: 2 rows")

    def test_check_fails_when_the_priority_or_section_disagrees_with_the_srs(self):
        self.edit("PRJ-002", priority="M")
        self.assert_check_fails("PRJ-002: priority M, the SRS says S")
        self.edit("PRJ-002", priority="S", section="4.2")
        self.assert_check_fails("PRJ-002: section 4.2, the SRS says 4.1")

    def test_check_fails_on_an_evidence_path_to_a_missing_file(self):
        self.edit("PRJ-001", evidence="Done: crates/demo/src/gone.rs:12 and prj_001_a_project_comes_from_the_template")
        self.assert_check_fails("PRJ-001: evidence path crates/demo/src/gone.rs does not exist")

    def test_line_numbers_and_truncated_paths_are_not_checked(self):
        self.edit(
            "PRJ-001",
            evidence="Done: crates/demo/src/lib.rs:999-1200, crates/demo/sr… prj_001_a_project_comes_from_the_template",
        )
        code, out = self.check()
        self.assertEqual(code, 0, out)

    def test_check_fails_on_a_done_row_without_a_test_or_guard(self):
        self.edit("STR-001", evidence="Done: crates/demo/src/lib.rs template()")
        self.assert_check_fails("STR-001: done row names no test or CI guard")

    def test_only_functions_marked_test_count_as_tests(self):
        self.edit("STR-001", evidence="Done: prj_001_a_project_comes_from_the_template")
        self.assertEqual(self.check()[0], 0)
        # A helper in a tests/ file, one in tests/common and a source function are no tests.
        for helper in ("helper_outside_a_test_attribute", "common_setup_helper", "template_helper"):
            self.edit("STR-001", evidence=f"Done: {helper}")
            self.assert_check_fails("STR-001: done row names no test or CI guard")

    def test_a_test_file_counts_but_not_a_helper_module_beside_it(self):
        self.edit("STR-001", evidence="Done: crates/demo/tests/demo.rs")
        self.assertEqual(self.check()[0], 0)
        # Neither a helper module in tests/ nor a source file with unit tests: name the test.
        for path in ("crates/demo/tests/common/mod.rs", "crates/demo/src/lib.rs"):
            self.edit("STR-001", evidence=f"Done: {path}")
            self.assert_check_fails("STR-001: done row names no test or CI guard")

    def test_a_python_test_file_counts_only_under_a_tests_directory(self):
        test_py = "import unittest\n\n\nclass T(unittest.TestCase):\n    def test_demo_case(self):\n        pass\n"
        self.repo.write("scripts/tests/test_demo.py", test_py)
        self.repo.write("scripts/loop/test_stray.py", test_py)
        self.edit("STR-001", evidence="Done: scripts/tests/test_demo.py")
        self.assertEqual(self.check()[0], 0)
        self.edit("STR-001", evidence="Done: scripts/loop/test_stray.py")
        self.assert_check_fails("STR-001: done row names no test or CI guard")

    def test_only_scripts_a_workflow_runs_count_as_guards(self):
        self.repo.write(status.TESTING_PATH, UNTRACED_TESTING)
        for script in ("scripts/check-legal.sh", "scripts/bench-demo.sh", "scripts/tests/test_unnamed.py"):
            self.edit("NFR-LEG-002", evidence=f"Done. Guarded by {script}.")
            code, out = self.check()
            self.assertEqual(code, 0, f"{script}: {out}")
        # It exists, but no workflow step runs it (comments in run: name it, a line and a tail).
        self.edit("NFR-LEG-002", evidence="Done. Guarded by scripts/other-tool.sh.")
        self.assert_check_fails("NFR-LEG-002: done row names no test or CI guard")

    def test_only_steps_that_run_a_guard_or_test_count_as_guards(self):
        self.repo.write(status.TESTING_PATH, UNTRACED_TESTING)
        self.edit("NFR-LEG-002", evidence="Done. CI: Unit tests of the demo crate.")
        self.assertEqual(self.check()[0], 0)
        # A job with a guard step counts, the cargo-deny action included.
        for name in ("Repository guards", "Licences and sources"):
            self.edit("NFR-LEG-002", evidence=f"Done. CI: {name}.")
            self.assertEqual(self.check()[0], 0, name)
        # A step that only installs tools, and a job of such steps, are no guards.
        for name in ("Install the browser and its libraries", "Browser installation only"):
            self.edit("NFR-LEG-002", evidence=f"Done. CI: {name}.")
            self.assert_check_fails("NFR-LEG-002: done row names no test or CI guard")

    def test_notification_review_and_status_workflows_are_no_guards(self):
        """Only the checking workflows guard a requirement: announcing changes, reporting the
        user's review and checking the traceability itself guard none (archicraft-jmlb)."""
        self.repo.write(status.TESTING_PATH, UNTRACED_TESTING)
        for evidence in (
            "scripts/announce-signal.sh",
            "scripts/loop/ci-user-review.sh",
            "scripts/status.py",
            "CI: Signal group",
            "CI: Announce the new changes",
            "CI: User review check",
            "CI: Implementation status",
            "CI: Tests of the status script",
        ):
            self.edit("NFR-LEG-002", evidence=f"Done. Guarded by {evidence}.")
            self.assert_check_fails("NFR-LEG-002: done row names no test or CI guard")

    def test_only_the_items_of_steps_are_steps(self):
        workflow = """jobs:
  package:
    name: Package
    strategy:
      matrix:
        include:
          - name: Linux
            run: scripts/other-tool.sh
    steps:
    - name: Package it
      env:
        TARGETS:
          - name: Windows
      run: scripts/package.sh
"""
        step = {"name": "Package it", "uses": "", "run": "scripts/package.sh"}
        self.assertEqual(status.workflow_jobs(workflow), [("Package", [step])])

    def test_a_ci_step_in_the_traceability_table_counts_as_a_guard(self):
        self.edit("NFR-LEG-002", evidence="Done. Nothing to see here.")
        code, out = self.check()
        self.assertEqual(code, 0, out)
        self.repo.write(
            status.TESTING_PATH,
            UNTRACED_TESTING,
        )
        self.assert_check_fails("NFR-LEG-002: done row names no test or CI guard")

    def test_a_done_row_needs_evidence_and_an_open_row_a_gap(self):
        self.edit("PRJ-002", gap="")
        self.assert_check_fails("PRJ-002: a partial row has a gap and no evidence")
        self.edit("PRJ-002", gap="Gap.", status="done", remaining_effort="none")
        self.assert_check_fails("PRJ-002: a done row has evidence and no gap")

    def test_a_row_with_missing_fields_is_reported(self):
        rows = self.repo.rows()
        del rows[1]["gap"]
        self.repo.set_rows(rows)
        self.assert_check_fails("PRJ-002: fields must be exactly")
        code, out = self.repo.run("generate")
        self.assertEqual(code, 2, out)
        self.assertIn("rows without exactly the fields", out)

    def test_effort_and_status_values_are_checked(self):
        self.edit("PRJ-002", remaining_effort="none")
        self.assert_check_fails("PRJ-002: remaining_effort 'none' for a partial row")
        self.edit("PRJ-002", remaining_effort="S", status="started")
        self.assert_check_fails("PRJ-002: status 'started' is not one of done, partial, missing")

    def test_depends_on_names_live_ids(self):
        self.edit("PRJ-002", depends_on=["STR-001"])
        self.assertEqual(self.check()[0], 0)
        self.edit("PRJ-002", depends_on=["BEM-001"])
        self.assert_check_fails("PRJ-002: depends_on must list live SRS IDs")

    def test_check_fails_on_a_hand_edit_inside_the_markers(self):
        doc = self.repo.read(status.DOC_PATH)
        self.repo.write(status.DOC_PATH, doc.replace("| PRJ-002 | S | partial | S |", "| PRJ-002 | S | done | none |"))
        self.assert_check_fails("the generated tables differ")

    def test_check_fails_on_a_json_edit_without_generate(self):
        rows = self.repo.rows()
        rows[1]["gap"] = "Changed."
        self.repo.set_rows(rows)
        self.assert_check_fails("run python3 scripts/status.py generate")

    def test_edits_outside_the_markers_are_free(self):
        doc = self.repo.read(status.DOC_PATH)
        self.repo.write(status.DOC_PATH, doc.replace("_Intro._", "_Another intro._"))
        self.assertEqual(self.check()[0], 0)


class MergeCheckTest(StatusTest):
    def setUp(self):
        super().setUp()
        self.repo.git("init", "-q", "-b", "main")
        self.base = self.repo.commit("base")

    def merge_check(self, body="", commit_message="change"):
        self.repo.commit(commit_message)
        body_file = self.repo.root / "body.md"
        body_file.write_text(body, encoding="utf-8")
        return self.repo.run("merge-check", "--base", self.base, "--body-file", str(body_file))

    def test_a_listed_id_whose_row_changed_passes(self):
        self.edit("PRJ-002", gap="Template store exists, no choice yet.")
        code, out = self.merge_check("## Summary\n\nSRS: PRJ-002\n")
        self.assertEqual(code, 0, out)

    def test_a_listed_id_whose_row_did_not_change_fails(self):
        self.edit("PRJ-002", gap="Template store exists, no choice yet.")
        code, out = self.merge_check("SRS: PRJ-002, STR-001\n")
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: listed on an SRS line but its row did not change", out)
        self.assertNotIn("PRJ-002:", out)

    def test_srs_lines_of_commit_messages_count(self):
        code, out = self.merge_check("", "archicraft-x: Do it\n\nSRS: STR-001.\n")
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: listed on an SRS line", out)

    def test_ids_in_html_comments_are_ignored(self):
        template = "SRS: <!-- requirement IDs, for example IFX-007 NFR-ACC-002 -->\n"
        code, out = self.merge_check(template)
        self.assertEqual(code, 0, out)

    def test_a_listed_id_without_a_row_fails(self):
        code, out = self.merge_check("SRS: PRJ-099\n")
        self.assertEqual(code, 1, out)
        self.assertIn("PRJ-099: listed on an SRS line but has no row", out)

    def test_a_done_row_that_regresses_fails_without_a_regresses_line(self):
        self.edit("STR-001", status="partial", remaining_effort="S", evidence="", gap="Order broke.")
        code, out = self.merge_check("SRS: STR-001\n")
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: regresses from done to partial without a 'Regresses: STR-001 <reason>' line", out)

    def test_a_regresses_line_allows_the_regression(self):
        self.edit("STR-001", status="missing", remaining_effort="M", evidence="", gap="Removed.")
        code, out = self.merge_check("SRS: STR-001\nRegresses: STR-001 storeys move to the new model in the next bead\n")
        self.assertEqual(code, 0, out)

    def test_a_regresses_line_needs_a_reason(self):
        self.edit("STR-001", status="missing", remaining_effort="M", evidence="", gap="Removed.")
        code, out = self.merge_check("Regresses: STR-001\n")
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: regresses from done to missing without a 'Regresses: STR-001 <reason>' line", out)

    def test_an_id_with_extra_digits_is_not_listed(self):
        self.assertEqual(status.listed_ids(["SRS: IFX-0071, STR-001, xPRJ-002"]), (["STR-001"], {}))
        self.assertEqual(status.listed_ids(["Regresses: STR-0011 typo"]), ([], {}))
        code, out = self.merge_check("SRS: STR-0011\n")
        self.assertEqual(code, 0, out)

    def test_an_unknown_base_is_an_error_without_a_traceback(self):
        self.repo.commit("change")
        code, out = self.repo.run("merge-check", "--base", "no-such-commit")
        self.assertEqual(code, 2, out)
        self.assertIn("status: git log no-such-commit..HEAD failed", out)

    def test_the_pull_request_that_introduces_the_rows_passes(self):
        (self.repo.root / status.JSON_PATH).rename(self.repo.root / "rows.json")
        self.base = self.repo.commit("before the status file")
        (self.repo.root / "rows.json").rename(self.repo.root / status.JSON_PATH)
        code, out = self.merge_check("SRS: PRJ-001 STR-001\n")
        self.assertEqual(code, 0, out)

    def test_without_a_base_json_a_regression_against_the_base_markdown_fails(self):
        (self.repo.root / status.JSON_PATH).rename(self.repo.root / "rows.json")
        self.base = self.repo.commit("before the status file, with the generated Markdown table")
        (self.repo.root / "rows.json").rename(self.repo.root / status.JSON_PATH)
        self.edit("STR-001", status="partial", remaining_effort="S", evidence="", gap="Lost in the import.")
        code, out = self.merge_check("SRS: PRJ-001 STR-001\n")
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: regresses from done to partial", out)
        self.assertNotIn("PRJ-001", out)

    def test_the_body_comes_from_the_github_event(self):
        self.repo.commit("no row change")
        event = self.repo.root / "event.json"
        event.write_text(json.dumps({"pull_request": {"body": "SRS: STR-001\n"}}), encoding="utf-8")
        old = os.environ.get("GITHUB_EVENT_PATH")
        os.environ["GITHUB_EVENT_PATH"] = str(event)
        try:
            code, out = self.repo.run("merge-check", "--base", self.base)
        finally:
            if old is None:
                del os.environ["GITHUB_EVENT_PATH"]
            else:
                os.environ["GITHUB_EVENT_PATH"] = old
        self.assertEqual(code, 1, out)
        self.assertIn("STR-001: listed on an SRS line but its row did not change", out)

    def test_merge_problems_compare_whole_rows(self):
        base = copy.deepcopy(ROWS)
        head = copy.deepcopy(ROWS)
        head[0]["depends_on"] = ["STR-001"]
        self.assertEqual(status.merge_problems(base, head, ["SRS: PRJ-001"]), [])


class RepositoryTest(unittest.TestCase):
    """The committed requirements.json and docs/implementation-status.md."""

    root = SCRIPT.parent.parent

    def test_check_passes_on_the_committed_state(self):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = status.main(["--root", str(self.root), "check"])
        self.assertEqual(code, 0, err.getvalue())

    def test_every_done_row_names_a_test_or_guard(self):
        rows = status.load_rows((self.root / status.JSON_PATH).read_text(encoding="utf-8"))
        done = [r for r in rows if r["status"] == "done"]
        self.assertGreaterEqual(len(done), 56)
        checks = status.find_checks(self.root)
        trace = status.traceability((self.root / status.TESTING_PATH).read_text(encoding="utf-8"))
        for r in done:
            texts = [r["evidence"], *trace.get(r["id"], [])]
            self.assertTrue(any(status.names_a_check(t, checks) for t in texts), r["id"])

    def test_the_status_scripts_are_executable(self):
        for path in (SCRIPT, Path(__file__).resolve()):
            self.assertTrue(os.access(path, os.X_OK), path)


if __name__ == "__main__":
    unittest.main()
