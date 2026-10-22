#!/usr/bin/env python3
"""Tests of how scripts/dashboard.py reads open pull requests on their way through the merge queue.

GitHub clears a PR's autoMergeRequest when the merge queue takes it in and again when the queue
drops it, so auto-merge alone cannot tell a queued PR, a dropped one and one that waits for the
user apart. Each test builds an open PR as `gh pr list` and the dashboard's GraphQL query return
it and checks what the page shows for it. Python 3 standard library only:
python3 scripts/tests/test_dashboard.py
"""

import importlib.util
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
        # An answer with no data at all, or an empty answer, fails the same way.
        for answer, message in (({"data": None, "errors": [{"message": "Bad credentials"}]},
                                 "Bad credentials"), ([], "")):
            with self.subTest(answer=answer), mock.patch.object(
                    dashboard, "run_json", lambda cmd, cwd=None, timeout=60, a=answer:
                    a if cmd[:3] == ["gh", "api", "graphql"] else listed):
                prs = dashboard.collect_prs()
                self.assertEqual([p["number"] for p in prs["open"]], [7, 9])
                self.assertTrue(prs["queue_error"])
                self.assertIn(message, prs["queue_error"])

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


if __name__ == "__main__":
    unittest.main()
