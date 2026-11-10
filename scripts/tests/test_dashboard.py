#!/usr/bin/env python3
"""Tests of how scripts/dashboard.py reads open pull requests on their way through the merge queue,
how it groups the showcase beads into the roadmap's demos and how it counts the requirements.

GitHub clears a PR's autoMergeRequest when the merge queue takes it in and again when the queue
drops it, so auto-merge alone cannot tell a queued PR, a dropped one and one that waits for the
user apart. Each test builds an open PR as `gh pr list` and the dashboard's GraphQL query return
it and checks what the page shows for it. Python 3 standard library only:
python3 scripts/tests/test_dashboard.py
"""

import importlib.util
import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

SCRIPT = Path(__file__).resolve().parent.parent / "dashboard.py"
spec = importlib.util.spec_from_file_location("dashboard", SCRIPT)
dashboard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dashboard)

GREEN = [{"name": "Rust", "status": "COMPLETED", "conclusion": "SUCCESS",
          "startedAt": "2026-10-21T10:00:00Z"}]


def queue_node(number, position=None, event=None, review="SUCCESS"):
    """One open PR as the dashboard's merge-queue query returns it."""
    return {
        "number": number,
        "mergeQueueEntry": {"position": position} if position else None,
        "timelineItems": {"nodes": [event] if event else []},
        "commits": {"nodes": [{"commit": {"status": {"context": {"state": review} if review else None}}}]},
    }


def open_pr(number=7, auto_merge=False, draft=False, checks=GREEN, **queue):
    """One open PR as `gh pr list --json` returns it, with its merge-queue fields merged in."""
    return {
        "number": number, "title": f"archicraft-ab{number}: Something", "isDraft": draft,
        "headRefName": f"bead/archicraft-ab{number}-something",
        "url": f"https://github.com/o/r/pull/{number}", "updatedAt": "2026-10-21T10:00:00Z",
        "labels": [], "autoMergeRequest": {"enabledAt": "2026-10-21T10:00:00Z"} if auto_merge else None,
        "mergeStateStatus": "CLEAN", "mergeable": "MERGEABLE", "reviewDecision": "",
        "statusCheckRollup": checks, "additions": 1, "deletions": 1, "changedFiles": 1,
        "queue": queue_node(number, **queue),
    }


REMOVED = {"__typename": "RemovedFromMergeQueueEvent", "reason": "failed checks"}
WAITS = "PR waits for your review and merge"
STUCK = "open without auto-merge and not in the merge queue"


class OpenPullRequests(unittest.TestCase):
    def test_a_queued_pr_shows_as_queued_and_not_as_waiting_for_the_user(self):
        """The queue cleared autoMergeRequest when it took the PR in (docs/loop.md, the merge queue)."""
        view = dashboard.open_pr_view(open_pr(position=2))
        self.assertEqual(view["queue"], {"state": "queued", "position": 2, "reason": None})
        self.assertEqual(view["status"]["text"], "in the merge queue, position 2")
        self.assertIsNone(view["needs_you"])

    def test_a_dropped_pr_shows_as_dropped_with_the_reason(self):
        """The queue dropped it and cleared autoMergeRequest: it needs a fix, not the user's review."""
        view = dashboard.open_pr_view(open_pr(event=REMOVED))
        self.assertEqual(view["queue"], {"state": "left-queue", "position": None, "reason": "failed checks"})
        self.assertEqual(view["status"]["text"], "dropped from the merge queue: failed checks")
        self.assertEqual(view["needs_you"], "the merge queue dropped it: failed checks")
        self.assertNotIn("review", view["needs_you"])

    def test_only_a_pr_whose_user_review_status_failed_waits_for_the_user(self):
        """scripts/loop/ci-user-review.sh (from .github/workflows/user-review.yml) posts a failed
        "User review" status on a PR that changes the workflows or the settings."""
        view = dashboard.open_pr_view(open_pr(review="FAILURE"))
        self.assertEqual(view["queue"]["state"], "waits-for-user")
        self.assertEqual(view["status"]["text"], "waiting for your review")
        self.assertEqual(view["needs_you"], WAITS)
        # An error status counts as well, and it wins over an earlier commit's removal from the queue.
        view = dashboard.open_pr_view(open_pr(review="ERROR", event=REMOVED))
        self.assertEqual(view["needs_you"], WAITS)

    def test_other_open_prs_do_not_wait_for_the_user(self):
        """Without auto-merge, a queue entry or a failed User review, nothing marks a PR as waiting
        for the user's review."""
        cases = [
            ("auto-merge", open_pr(auto_merge=True)),
            ("open", open_pr()),
            ("open", open_pr(review=None)),
        ]
        for state, pr in cases:
            with self.subTest(state=state, review=pr["queue"]["commits"]):
                view = dashboard.open_pr_view(pr)
                self.assertEqual(view["queue"]["state"], state)
                self.assertNotEqual(view["needs_you"], WAITS)
                if state == "auto-merge":
                    self.assertIsNone(view["needs_you"])
        # A merged-from-the-queue removal event is not a drop, and a draft never needs the user.
        merged = {"__typename": "RemovedFromMergeQueueEvent", "reason": "merged"}
        self.assertEqual(dashboard.open_pr_view(open_pr(event=merged))["queue"]["state"], "open")
        self.assertIsNone(dashboard.open_pr_view(open_pr(draft=True, review="FAILURE"))["needs_you"])
        self.assertIsNone(dashboard.open_pr_view(open_pr(draft=True))["needs_you"])
        # Turning auto-merge on again after a drop (land.sh after a fix) starts over.
        view = dashboard.open_pr_view(open_pr(auto_merge=True, event=REMOVED))
        self.assertEqual(view["queue"]["state"], "auto-merge")
        self.assertEqual(view["status"]["text"], "green, merging")
        self.assertIsNone(view["needs_you"])

    def test_a_stuck_open_pr_needs_the_user_with_its_own_reason(self):
        """docs/loop.md, the merge queue: a PR open without auto-merge, outside the queue and not
        waiting for the user goes into the Needs you line, as stuck rather than as a review."""
        for pr in (open_pr(), open_pr(review=None)):
            with self.subTest(review=pr["queue"]["commits"]):
                view = dashboard.open_pr_view(pr)
                self.assertEqual(view["needs_you"], STUCK)
                self.assertNotIn("review", view["needs_you"])

    def test_the_failed_user_review_status_is_not_a_failing_check(self):
        """ci-user-review.sh's failed "User review" status shows as waiting for the review, not as
        failing CI."""
        rollup = GREEN + [{"__typename": "StatusContext", "context": "User review", "state": "FAILURE",
                           "startedAt": "2026-10-21T10:01:00Z"}]
        view = dashboard.open_pr_view(open_pr(review="FAILURE", checks=rollup))
        checks = view["checks"]
        self.assertEqual((checks["fail"], checks["failing"]), (0, []))
        self.assertEqual(view["status"]["text"], "waiting for your review")
        # Another failed check still counts.
        red = rollup + [{"name": "Clippy", "status": "COMPLETED", "conclusion": "FAILURE",
                         "startedAt": "2026-10-21T10:00:00Z"}]
        self.assertEqual(dashboard.summarize_checks(red)["failing"], ["Clippy"])

    def test_the_failed_user_review_check_run_is_not_a_failing_check(self):
        """The job "User review check" in .github/workflows/user-review.yml fails while the PR waits
        (ci-user-review.sh exits 1): the PR waits for the review, once in Needs you, and its card
        lists no failing check."""
        rollup = GREEN + [
            {"__typename": "CheckRun", "name": "User review check", "workflowName": "User review",
             "status": "COMPLETED", "conclusion": "FAILURE", "startedAt": "2026-10-21T10:01:00Z"},
            {"__typename": "StatusContext", "context": "User review", "state": "FAILURE",
             "startedAt": "2026-10-21T10:02:00Z"}]
        view = dashboard.open_pr_view(open_pr(review="FAILURE", checks=rollup))
        checks = view["checks"]
        self.assertEqual((checks["fail"], checks["failing"]), (0, []))
        self.assertEqual(view["status"]["text"], "waiting for your review")
        # The overview adds a "checks failing" row for every PR with checks.fail, so no fail means
        # the PR shows once, with its needs_you reason.
        self.assertEqual(view["needs_you"], WAITS)

    def test_a_failed_user_review_check_run_without_a_failed_status_is_a_failing_check(self):
        """When the job "User review check" fails before ci-user-review.sh posts the "User review"
        status (git fetch or gh api failing under set -e), the required status never arrives and
        the PR never merges: the run counts as a failing check, also next to a passed status."""
        run = {"__typename": "CheckRun", "name": "User review check", "workflowName": "User review",
               "status": "COMPLETED", "conclusion": "FAILURE", "startedAt": "2026-10-21T10:01:00Z"}
        passed = {"__typename": "StatusContext", "context": "User review", "state": "SUCCESS",
                  "startedAt": "2026-10-21T09:00:00Z"}
        for case, rollup in (("without a status", GREEN + [run]),
                             ("beside a passed status", GREEN + [run, passed])):
            with self.subTest(case=case):
                checks = dashboard.summarize_checks(rollup)
                self.assertEqual((checks["fail"], checks["failing"]), (1, ["User review check"]))
        # The card says so too, rather than "green, merging" (land.sh turns auto-merge on).
        view = dashboard.open_pr_view(open_pr(review=None, auto_merge=True, checks=GREEN + [run]))
        self.assertEqual(view["status"]["text"], "checks failing")
        # A failed status beside the failed run still means waiting for the review.
        errored = dict(passed, state="ERROR", startedAt="2026-10-21T10:02:00Z")
        checks = dashboard.summarize_checks(GREEN + [run, errored])
        self.assertEqual((checks["fail"], checks["failing"]), (0, []))

    def test_a_failed_queue_query_leaves_the_pr_list_working(self):
        """Without the merge-queue answer the PRs still show, and none is reported as stuck."""
        listed = [{k: v for k, v in open_pr(7).items() if k != "queue"},
                  {k: v for k, v in open_pr(9, auto_merge=True).items() if k != "queue"}]
        merged = [{"number": 5, "title": "x", "state": "MERGED"}]

        def fake(cmd, cwd=None, timeout=60):
            if cmd[:3] == ["gh", "api", "graphql"]:
                raise RuntimeError("gh api graphql: HTTP 502")
            return listed if "open" in cmd else merged

        with mock.patch.object(dashboard, "run_json", fake):
            prs = dashboard.collect_prs()
        self.assertEqual([p["number"] for p in prs["open"]], [7, 9])
        self.assertEqual(prs["all"], merged)
        views = {p["number"]: dashboard.open_pr_view(p) for p in prs["open"]}
        self.assertEqual(views[7]["queue"]["state"], "unknown")
        self.assertEqual(views[7]["status"]["text"], "auto-merge off, merge queue unknown")
        self.assertIsNone(views[7]["needs_you"])
        # Without the queue's answer an auto-merge PR stays "auto-merge", queued or not.
        self.assertEqual(views[9]["queue"]["state"], "auto-merge")
        self.assertIsNone(views[9]["needs_you"])
        # The failure itself is reported, so the page can show it among its errors.
        self.assertIn("HTTP 502", prs["queue_error"])
        # A GraphQL answer with errors and no data counts as failed as well.
        with mock.patch.object(dashboard, "run_json", lambda cmd, cwd=None, timeout=60:
                               {"data": {"repository": None},
                                "errors": [{"message": "Could not resolve to a Repository"},
                                           {"message": "Rate limit exceeded"}]}
                               if cmd[:3] == ["gh", "api", "graphql"] else listed):
            prs = dashboard.collect_prs()
        self.assertEqual(dashboard.open_pr_view(prs["open"][0])["queue"]["state"], "unknown")
        # GitHub's own messages, not a Python error about the missing data.
        self.assertIn("Could not resolve to a Repository", prs["queue_error"])
        self.assertIn("Rate limit exceeded", prs["queue_error"])
        self.assertNotIn("NoneType", prs["queue_error"])
        # An answer with no data at all, or an empty answer, fails the same way: the PRs still
        # show, without a queue state, and the failure is reported.
        for answer, message in (({"data": None, "errors": [{"message": "Bad credentials"}]},
                                 "Bad credentials"),
                                ([], "gh api graphql: the answer is not a JSON object"),
                                ({}, "gh api graphql: the answer has no repository"),
                                # A data field that is not an object (archicraft-pdkq).
                                ({"data": [1]}, "gh api graphql: the answer has no repository"),
                                ({"data": "x"}, "gh api graphql: the answer has no repository"),
                                # A repository field that is not an object (archicraft-3l2no).
                                ({"data": {"repository": [1]}},
                                 "gh api graphql: the answer has no repository"),
                                ({"data": {"repository": "x"}},
                                 "gh api graphql: the answer has no repository")):
            with self.subTest(answer=answer), mock.patch.object(
                    dashboard, "run_json", lambda cmd, cwd=None, timeout=60, a=answer:
                    a if cmd[:3] == ["gh", "api", "graphql"] else listed):
                prs = dashboard.collect_prs()
                self.assertEqual([p["number"] for p in prs["open"]], [7, 9])
                self.assertEqual(dashboard.open_pr_view(prs["open"][0])["queue"]["state"],
                                 "unknown")
                self.assertIn(message, prs["queue_error"])
                # Plain words, not a Python error about indexing the answer.
                self.assertNotIn("indices", prs["queue_error"])

    def test_checks_and_the_user_review_status_order_by_the_same_time(self):
        """check_time() is the one ordering of rollup entries: startedAt, else createdAt. The "User
        review" status and the other checks both pick their newest entry by it."""
        self.assertEqual(dashboard.check_time({"startedAt": "b", "createdAt": "a"}), "b")
        self.assertEqual(dashboard.check_time({"createdAt": "a"}), "a")
        self.assertEqual(dashboard.check_time({}), "")
        # The newer entry carries only createdAt, the older one only startedAt: a reader that
        # dropped the createdAt fallback would pick the older one. Both orders of the list.
        review = [{"context": "User review", "state": "FAILURE", "startedAt": "2026-10-21T10:00:00Z"},
                  {"context": "User review", "state": "SUCCESS", "createdAt": "2026-10-21T10:05:00Z"}]
        build = [{"name": "build", "status": "COMPLETED", "conclusion": "FAILURE",
                  "startedAt": "2026-10-21T10:00:00Z"},
                 {"name": "build", "status": "COMPLETED", "conclusion": "SUCCESS",
                  "createdAt": "2026-10-21T10:05:00Z"}]
        for order in (list, lambda l: list(reversed(l))):
            self.assertEqual(dashboard.rollup_user_review(order(review)), "SUCCESS")
            self.assertEqual(dashboard.summarize_checks(order(build))["pass"], 1)
            self.assertEqual(dashboard.summarize_checks(order(build))["fail"], 0)
        # Equal times: both readers take the entry that comes last in the rollup.
        tied = "2026-10-21T10:00:00Z"
        for first, last in (("FAILURE", "SUCCESS"), ("SUCCESS", "FAILURE")):
            with self.subTest(last=last):
                review = [{"context": "User review", "state": s, "startedAt": tied}
                          for s in (first, last)]
                self.assertEqual(dashboard.rollup_user_review(review), last)
                build = [{"name": "build", "status": "COMPLETED", "conclusion": s, "startedAt": tied}
                         for s in (first, last)]
                summary = dashboard.summarize_checks(build)
                self.assertEqual((summary["pass"], summary["fail"]),
                                 (1, 0) if last == "SUCCESS" else (0, 1))

    def test_without_the_queue_answer_a_failed_user_review_still_waits_for_the_user(self):
        """The fallback reads the "User review" status from the PR's statusCheckRollup, so a PR that
        waits for the user's review stays in Needs you when the queue query failed."""
        # gh exports a StatusContext's time as "startedAt"; the rollup's order is not the time order.
        rollup = GREEN + [{"__typename": "StatusContext", "context": "User review", "state": "FAILURE",
                           "startedAt": "2026-10-21T10:01:00Z"},
                          {"__typename": "StatusContext", "context": "User review", "state": "SUCCESS",
                           "startedAt": "2026-10-21T09:00:00Z"}]
        for auto_merge in (False, True):
            with self.subTest(auto_merge=auto_merge):
                pr = {k: v for k, v in open_pr(checks=rollup, auto_merge=auto_merge).items() if k != "queue"}
                view = dashboard.open_pr_view(pr)
                self.assertEqual(view["queue"]["state"], "waits-for-user")
                self.assertEqual(view["status"]["text"], "waiting for your review")
                self.assertEqual(view["needs_you"], WAITS)
        # The newest User review status decides: a later success no longer waits.
        passed = [{"__typename": "StatusContext", "context": "User review", "state": "SUCCESS",
                   "startedAt": "2026-10-21T11:00:00Z"}] + rollup
        pr = {k: v for k, v in open_pr(checks=passed).items() if k != "queue"}
        self.assertEqual(dashboard.open_pr_view(pr)["queue"]["state"], "unknown")
        # The time decides, not the position: the newest status may come last as well.
        self.assertEqual(dashboard.rollup_user_review(list(reversed(passed))), "SUCCESS")
        # A status with only "createdAt" still sorts by it.
        self.assertEqual(dashboard.rollup_user_review(
            [{"context": "User review", "state": "FAILURE", "createdAt": "2026-10-21T10:00:00Z"},
             {"context": "User review", "state": "SUCCESS", "createdAt": "2026-10-21T09:00:00Z"}]),
            "FAILURE")

    def test_a_failed_queue_query_shows_in_the_page_errors(self):
        """refresh() turns collect_prs()'s queue_error into an entry of the page's errors."""
        prs = {"open": [], "all": [], "queue_error": "gh api graphql: HTTP 502"}
        snapshots = mock.Mock(rows=[])
        with mock.patch.object(dashboard, "collect_prs", lambda: prs), \
                mock.patch.object(dashboard, "collect_beads", lambda: []), \
                mock.patch.object(dashboard, "collect_sessions", lambda: []), \
                mock.patch.object(dashboard, "assemble", lambda raw, now: {"history": {}}), \
                mock.patch.object(dashboard, "SNAPSHOTS", snapshots), \
                mock.patch.dict(dashboard.RAW, clear=True), \
                mock.patch.dict(dashboard.STATE, clear=True):
            dashboard.refresh(30)
            errors = dict(dashboard.STATE["errors"])
        self.assertEqual(errors, {dashboard.QUEUE_ERROR: "gh api graphql: HTTP 502"})
        # The page words that entry as unknown queue states, not as a source showing old data.
        self.assertIn(f'k === "{dashboard.QUEUE_ERROR}"', dashboard.PAGE)

    def test_collect_prs_joins_the_merge_queue_fields_by_pr_number(self):
        listed = [{k: v for k, v in open_pr(7).items() if k != "queue"},
                  {k: v for k, v in open_pr(9).items() if k != "queue"}]
        answer = {"data": {"repository": {"pullRequests": {"nodes": [queue_node(9, position=1)]}}}}
        calls = []

        def fake(cmd, cwd=None, timeout=60):
            calls.append(cmd)
            if cmd[:3] == ["gh", "api", "graphql"]:
                return answer
            return listed if "open" in cmd else []

        with mock.patch.object(dashboard, "run_json", fake):
            prs = dashboard.collect_prs()
        views = {p["number"]: dashboard.open_pr_view(p) for p in prs["open"]}
        self.assertEqual(views[9]["queue"]["state"], "queued")
        self.assertEqual(views[7]["queue"]["state"], "open")
        self.assertTrue(any(c[:3] == ["gh", "api", "graphql"] for c in calls))

    def test_the_page_takes_the_needs_you_reason_from_the_server(self):
        """The overview no longer treats every PR without auto-merge as waiting for the user."""
        self.assertFalse("!p.auto_merge" in dashboard.PAGE)
        self.assertTrue("p.needs_you" in dashboard.PAGE)
        self.assertFalse(WAITS in dashboard.PAGE)


def bead(id, status="open", labels=("showcase",), needs=(), parent=None, minutes=60, type="task", title=None):
    """One bead as `bd list --brief --json` returns it."""
    deps = [{"issue_id": id, "depends_on_id": d, "type": "blocks"} for d in needs]
    return {"id": id, "title": title or id, "status": status, "priority": 0, "issue_type": type,
            "labels": list(labels), "estimated_minutes": minutes, "parent": parent, "dependencies": deps}


def demo(n, verify_needs, verify_status="open", status="open"):
    """A showcase epic "Showcase <n>: ..." and its verification task <epic>.1."""
    return [bead(f"x-d{n}", status, type="epic", title=f"Showcase {n}: demo {n}", minutes=None),
            bead(f"x-d{n}.1", verify_status, needs=verify_needs, parent=f"x-d{n}")]


class ShowcaseDemos(unittest.TestCase):
    def test_a_demo_holds_the_showcase_beads_it_needs_that_no_earlier_demo_needs(self):
        beads = demo(2, ["x-b"]) + demo(1, ["x-a"]) + [
            bead("x-base", "closed"), bead("x-a", needs=["x-base"]), bead("x-b", needs=["x-a", "x-c", "x-other"]),
            bead("x-c"), bead("x-other", labels=())]
        d1, d2 = dashboard.showcase_demos(beads)
        self.assertEqual((d1["n"], d1["title"]), (1, "Demo 1"))
        self.assertEqual([t["id"] for t in d1["tasks"]], ["x-a", "x-d1.1", "x-base"])
        self.assertEqual([t["id"] for t in d2["tasks"]], ["x-b", "x-c", "x-d2.1"])
        self.assertEqual((d1["leaves_done"], d1["leaves"], d1["est_done"], d1["est_total"]), (1, 3, 60, 180))
        self.assertEqual((d1["state"], d2["state"]), ("in_progress", "planned"))

    def test_a_split_bead_counts_through_its_leaves(self):
        beads = demo(1, ["x-a"]) + [bead("x-a", "in_progress"), bead("x-a.1", "closed", parent="x-a", minutes=30),
                                    bead("x-a.2", parent="x-a", minutes=90)]
        (d,) = dashboard.showcase_demos(beads, ready={"x-a.2"})
        self.assertEqual([(t["id"], t["ready"]) for t in d["tasks"]], [("x-a.2", True), ("x-d1.1", False), ("x-a.1", False)])
        self.assertEqual((d["est_done"], d["est_total"]), (30, 180))

    def test_a_closed_verification_task_makes_the_demo_ready_to_show_until_it_is_accepted(self):
        shown, = dashboard.showcase_demos(demo(1, [], verify_status="closed"))
        accepted, = dashboard.showcase_demos(demo(1, [], verify_status="closed", status="closed"))
        self.assertEqual((shown["state"], accepted["state"]), ("demo", "accepted"))


class RequirementStatus(unittest.TestCase):
    """The roadmap page counts the requirements from docs/status/requirements.json, which is their
    only tracked copy since archicraft-tvmtt, and the commits from before the JSON existed through
    the requirement table docs/implementation-status.md held then."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.git("init", "-q", "-b", "main")

    def tearDown(self):
        self.tmp.cleanup()

    def git(self, *args, when=None):
        env = {**os.environ, "GIT_AUTHOR_DATE": f"@{when} +0000", "GIT_COMMITTER_DATE": f"@{when} +0000"} if when else None
        return subprocess.run(["git", "-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "core.hooksPath=/dev/null",
                               "-c", "commit.gpgsign=false", *args], cwd=self.root, env=env, check=True,
                              capture_output=True, text=True).stdout.strip()

    def commit(self, when, files):
        for path, text in files.items():
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
            (self.root / path).write_text(text, encoding="utf-8")
        self.git("add", "-A")
        self.git("commit", "-q", "-m", f"at {when}", when=when)
        return self.git("rev-parse", "--short", "HEAD")

    @staticmethod
    def rows(*statuses):
        return json.dumps({"requirements": [{"id": rid, "priority": "M", "status": s} for rid, s in statuses]})

    def test_the_counts_come_from_requirements_json_and_older_commits_from_the_document(self):
        table = "| ID | Pri | Status | Effort | Gap / evidence |\n|---|---|---|---|---|\n"
        scenarios = "| AS-01 | partial | Notes. |\n"
        self.commit(1000, {dashboard.STATUS_DOC: table + "| PRJ-001 | M | missing | M | Gap. |\n| STR-001 | M | done | none | Done. |\n"})
        self.commit(2000, {dashboard.STATUS_JSON: self.rows(("PRJ-001", "partial"), ("STR-001", "done")),
                           dashboard.STATUS_DOC: "# Status\n\n" + scenarios})
        self.commit(3000, {dashboard.STATUS_JSON: self.rows(("PRJ-001", "done"), ("STR-001", "done"))})
        head = self.commit(4000, {dashboard.STATUS_DOC: "# Status, edited by hand\n\n" + scenarios})
        real_run = dashboard.run
        with mock.patch.object(dashboard, "run", lambda cmd, **kw: real_run(cmd, cwd=self.root)):
            reqs, history = dashboard.requirement_status(head)
        self.assertEqual(dashboard.status_counts(reqs), {"M": {"done": 2}})
        self.assertEqual([(h["t"], h["done_M"], h["partial_M"]) for h in history], [(1000, 1, 0), (2000, 1, 1), (3000, 2, 0)])


class BeadsLock(unittest.TestCase):
    """Every bd process waits for the embedded Dolt lock, so the dashboard keeps its own bd calls few."""

    def test_status_sh_runs_without_the_real_bd(self):
        with tempfile.TemporaryDirectory() as tree:
            script = Path(tree) / "scripts" / "loop" / "status.sh"
            script.parent.mkdir(parents=True)
            script.write_text('echo "  gate:   $(bd show x-a --json)"\necho "  paused: $(command -v bd)"\n')
            with mock.patch.object(dashboard, "NO_BD", dashboard.stand_in_bd()):
                fields = dashboard.loop_status(tree)
                self.assertEqual(fields, {"gate": "[]", "paused": str(dashboard.NO_BD / "bd")})
                shutil.rmtree(dashboard.NO_BD)

    def test_the_bead_queries_wait_longer_for_the_lock_than_other_commands(self):
        with mock.patch.object(dashboard, "run_json", return_value=[]) as run_json:
            dashboard.collect_beads()
        self.assertEqual([c.kwargs["timeout"] for c in run_json.call_args_list], [dashboard.BD_TIMEOUT] * 4)
        self.assertGreater(dashboard.BD_TIMEOUT, 60)


if __name__ == "__main__":
    unittest.main()
