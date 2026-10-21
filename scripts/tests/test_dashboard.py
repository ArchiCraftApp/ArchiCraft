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
        """land.sh posts a failed "User review" status on a PR that changes the workflows or the settings."""
        view = dashboard.open_pr_view(open_pr(review="FAILURE"))
        self.assertEqual(view["queue"]["state"], "waits-for-user")
        self.assertEqual(view["status"]["text"], "waiting for your review")
        self.assertEqual(view["needs_you"], WAITS)
        # An error status counts as well, and it wins over an earlier commit's removal from the queue.
        view = dashboard.open_pr_view(open_pr(review="ERROR", event=REMOVED))
        self.assertEqual(view["needs_you"], WAITS)

    def test_other_open_prs_do_not_wait_for_the_user(self):
        """Without auto-merge, a queue entry or a failed User review, nothing marks a PR as the user's."""
        cases = {
            "auto-merge": open_pr(auto_merge=True),
            "open": open_pr(),
            "open ": open_pr(review=None),
        }
        for state, pr in cases.items():
            with self.subTest(state=state):
                view = dashboard.open_pr_view(pr)
                self.assertEqual(view["queue"]["state"], state.strip())
                self.assertIsNone(view["needs_you"])
        # A merged-from-the-queue removal event is not a drop, and a draft never needs the user.
        merged = {"__typename": "RemovedFromMergeQueueEvent", "reason": "merged"}
        self.assertEqual(dashboard.open_pr_view(open_pr(event=merged))["queue"]["state"], "open")
        self.assertIsNone(dashboard.open_pr_view(open_pr(draft=True, review="FAILURE"))["needs_you"])
        # Turning auto-merge on again after a drop (land.sh after a fix) starts over.
        view = dashboard.open_pr_view(open_pr(auto_merge=True, event=REMOVED))
        self.assertEqual(view["queue"]["state"], "auto-merge")
        self.assertEqual(view["status"]["text"], "green, merging")

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
