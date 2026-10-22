#!/usr/bin/env python3
"""Local status dashboard for the bead loop: beads, agent sessions, pull requests and history.

Serves one page on 127.0.0.1 that keeps itself up to date. It runs bd, gh, git, ps and lsof
and reads the Claude Code transcripts of this repository's sessions. It changes nothing on its
own: only the Cleanup panel does, after a preview and a confirmation on the page (stop stale
sessions, remove finished worktrees and branches, run the sweep). Python 3 standard library only.

The Roadmap page (/roadmap) shows how far the roadmap has come: the epics of docs/roadmap.md
placed by their dependencies with the share of their task estimates that is done, the
increments with their exit criteria, the SRS requirement status of docs/implementation-status.md
and the pace of the last 7 days. The Code map page (/code) shows the Rust modules of origin/main as a component map and as a
dependency graph, coloured by how recently or how much they changed, with the files that open
PRs and running sessions are changing now.

Agent state comes from two sources: whether a `claude` process runs in the session's
worktree, and where the session's transcript stands (inside a turn or after it ended).
Count snapshots for the sparklines are kept in <git common dir>/loop-dashboard/.

Usage: scripts/dashboard.py [--port 8765] [--interval 20] [--no-open]
"""

import argparse
import concurrent.futures
import json
import os
import re
import secrets
import signal
import subprocess
import tempfile
import threading
import time
import webbrowser
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent

# A session inside a turn with no transcript write for this long is "quiet", then "stale".
QUIET_AFTER = 10 * 60
STALE_AFTER = 2 * 3600
# Ended sessions stay listed this long.
ENDED_WINDOW = 24 * 3600
# Snapshot cadence for the count history (a changed count is recorded at once).
SNAPSHOT_EVERY = 5 * 60
ACTIVITY_BIN = 300

BEAD_BRANCH = re.compile(r"^bead/([a-z0-9]+-[a-z0-9]+(?:\.[0-9]+)*)(?:-|$)")
BEAD_TITLE = re.compile(r"^([a-z0-9]+-[a-z0-9]+(?:\.[0-9]+)*):")


def run(cmd, cwd=None, timeout=60):
    """Runs a command and returns its stdout; raises with stderr on failure."""
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)}: {(p.stderr or p.stdout).strip()[:300]}")
    return p.stdout


def run_json(cmd, cwd=None, timeout=60):
    out = run(cmd, cwd=cwd, timeout=timeout).strip()
    return json.loads(out) if out else []


def iso_to_epoch(s):
    if not s:
        return None
    try:
        return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
    except ValueError:
        return None


def epoch_to_iso(t):
    return datetime.fromtimestamp(t, timezone.utc).isoformat() if t else None


GIT_COMMON = Path(run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"],
                      cwd=HERE).strip())
ROOT = GIT_COMMON.parent
STATE_DIR = GIT_COMMON / "loop-dashboard"


# --- beads -------------------------------------------------------------------------------

def collect_beads():
    return {
        "beads": run_json(["bd", "list", "--all", "--brief", "--flat", "--json", "-n", "0"], cwd=ROOT),
        "ready": run_json(["bd", "ready", "--json", "-n", "0"], cwd=ROOT),
        "blocked": run_json(["bd", "blocked", "--json"], cwd=ROOT),
        "epics": run_json(["bd", "epic", "status", "--json"], cwd=ROOT),
    }


def slim(b):
    return {k: b.get(k) for k in (
        "id", "title", "status", "priority", "issue_type", "assignee", "labels",
        "estimated_minutes", "updated_at", "closed_at", "external_ref", "parent")}


# --- pull requests -----------------------------------------------------------------------

OPEN_FIELDS = ("number,title,state,isDraft,headRefName,url,createdAt,updatedAt,labels,"
               "autoMergeRequest,mergeStateStatus,mergeable,reviewDecision,statusCheckRollup,"
               "additions,deletions,changedFiles,files")
ALL_FIELDS = "number,title,state,headRefName,headRefOid,url,createdAt,mergedAt,closedAt"


# What `gh pr list` cannot say: where an open PR stands in the merge queue. The fields are those
# scripts/loop/pr-state.sh reads for one PR; pr_queue_state() decides with the same rules.
QUEUE_QUERY = """query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    pullRequests(states: OPEN, first: 50, orderBy: {field: CREATED_AT, direction: DESC}) {
      nodes {
        number
        mergeQueueEntry { position }
        timelineItems(last: 1, itemTypes: [ADDED_TO_MERGE_QUEUE_EVENT, REMOVED_FROM_MERGE_QUEUE_EVENT,
            AUTO_MERGE_ENABLED_EVENT]) {
          nodes { __typename ... on RemovedFromMergeQueueEvent { reason } }
        }
        commits(last: 1) { nodes { commit { status { context(name: "User review") { state } } } } }
      }
    }
  }
}"""


def collect_prs():
    open_prs = run_json(["gh", "pr", "list", "--state", "open", "--limit", "50",
                         "--json", OPEN_FIELDS], cwd=ROOT)
    # Without the queue's answer the PRs still show; their queue state is then "unknown" and the
    # failure goes into "queue_error", which refresh() shows among the page's errors.
    queue_error = None
    try:
        answer = run_json(["gh", "api", "graphql", "-F", "owner={owner}", "-F", "name={repo}",
                           "-f", f"query={QUEUE_QUERY}"], cwd=ROOT)
        errors = answer.get("errors") if isinstance(answer, dict) else None
        if errors and not ((answer.get("data") or {}).get("repository")):
            # A GraphQL error comes back as an answer without the repository: report GitHub's
            # messages instead of failing on the missing data.
            raise RuntimeError("gh api graphql: " + "; ".join(
                str(e.get("message") if isinstance(e, dict) else e) for e in errors))
        queue = {n["number"]: n for n in answer["data"]["repository"]["pullRequests"]["nodes"]}
    except (RuntimeError, ValueError, KeyError, TypeError, subprocess.TimeoutExpired) as e:
        queue, queue_error = None, str(e) or type(e).__name__
    for p in open_prs:
        if queue is not None:
            p["queue"] = queue.get(p["number"])
    return {
        "open": open_prs,
        "all": run_json(["gh", "pr", "list", "--state", "all", "--limit", "300",
                         "--json", ALL_FIELDS], cwd=ROOT),
        "queue_error": queue_error,
    }


def pr_bead(p):
    m = BEAD_BRANCH.match(p.get("headRefName") or "") or BEAD_TITLE.match(p.get("title") or "")
    return m.group(1) if m else None


# The status scripts/loop/ci-user-review.sh (from .github/workflows/user-review.yml) posts on a PR
# that waits for the user's review: not a check of the code.
USER_REVIEW = "User review"
# The job of that workflow, which fails while the PR waits (ci-user-review.sh exits 1).
USER_REVIEW_CHECK = "User review check"


def rollup_user_review(rollup):
    """The state of the newest "User review" status in a PR's statusCheckRollup, or None. gh
    exports a status's time as "startedAt"; "createdAt" is the fallback, as in summarize_checks()."""
    def started(c):
        return c.get("startedAt") or c.get("createdAt") or ""

    latest = None
    for c in rollup or []:
        if (c.get("name") or c.get("context")) == USER_REVIEW:
            if latest is None or started(c) >= started(latest):
                latest = c
    return (latest or {}).get("state")


def summarize_checks(rollup):
    """Latest run per check name, reduced to pass/fail/pending counts. The "User review" status is
    left out: pr_queue_state() reads it as waiting for the user, not as a failing check. So is the
    "User review check" run that posts it, but only while that status failed: a run that failed
    without posting it (git fetch or gh api failing) leaves the required status missing, and the
    PR never merges."""
    waits = rollup_user_review(rollup) in ("FAILURE", "ERROR")
    latest = {}
    for c in rollup or []:
        name = c.get("name") or c.get("context") or "?"
        if name == USER_REVIEW or (name == USER_REVIEW_CHECK and waits):
            continue
        started = c.get("startedAt") or c.get("createdAt") or ""
        if name not in latest or started > latest[name][0]:
            latest[name] = (started, c)
    summary = {"pass": 0, "fail": 0, "pending": 0, "skipped": 0, "failing": [], "running": []}
    for name, (_, c) in sorted(latest.items()):
        state = (c.get("conclusion") or c.get("state") or "").upper()
        if c.get("status") and c.get("status") != "COMPLETED":
            summary["pending"] += 1
            summary["running"].append(name)
        elif state in ("SUCCESS", "NEUTRAL"):
            summary["pass"] += 1
        elif state in ("SKIPPED", "CANCELLED", "STALE"):
            summary["skipped"] += 1
        elif state in ("PENDING", "EXPECTED", "QUEUED", "IN_PROGRESS", ""):
            summary["pending"] += 1
            summary["running"].append(name)
        else:
            summary["fail"] += 1
            summary["failing"].append(name)
    return summary


def pr_queue_state(p):
    """Where an open PR stands on its way through the merge queue, by the rules of
    scripts/loop/pr-state.sh: GitHub clears autoMergeRequest when the queue takes a PR in and when
    it drops one, so only the queue entry, the newest queue event and the status "User review"
    tell a queued, a dropped and a waiting PR apart. States: queued, waits-for-user, auto-merge,
    left-queue, open, and unknown when the queue query failed (no "queue" key).

    Without the queue's answer, the "User review" status comes from the statusCheckRollup, so a PR
    that waits for the user still does; an auto-merge PR stays "auto-merge" even if it is queued,
    and every other PR is "unknown"."""
    if "queue" not in p:
        if rollup_user_review(p.get("statusCheckRollup")) in ("FAILURE", "ERROR"):
            fallback = "waits-for-user"
        else:
            fallback = "auto-merge" if p.get("autoMergeRequest") else "unknown"
        return {"state": fallback, "position": None, "reason": None}
    q = p.get("queue") or {}
    event = ((q.get("timelineItems") or {}).get("nodes") or [None])[-1] or {}
    commit = ((q.get("commits") or {}).get("nodes") or [None])[-1] or {}
    review = ((((commit.get("commit") or {}).get("status") or {}).get("context")) or {}).get("state")
    state = {"state": "open", "position": None, "reason": None}
    if q.get("mergeQueueEntry"):
        state.update(state="queued", position=q["mergeQueueEntry"].get("position"))
    elif review in ("FAILURE", "ERROR"):
        state["state"] = "waits-for-user"
    elif p.get("autoMergeRequest"):
        state["state"] = "auto-merge"
    elif event.get("__typename") == "RemovedFromMergeQueueEvent" and event.get("reason") != "merged":
        state.update(state="left-queue", reason=event.get("reason") or "unknown")
    return state


def pr_status(view):
    """The PR's state on the page: a dot class and a short text."""
    c, q = view["checks"], view["queue"]
    if view["draft"]:
        return "bg-mute", "draft"
    if q["state"] == "queued":
        return "bg-ok", f"in the merge queue, position {q['position']}"
    if q["state"] == "left-queue":
        return "bg-bad", f"dropped from the merge queue: {q['reason']}"
    if q["state"] == "waits-for-user":
        return "bg-warn", "waiting for your review"
    if c["fail"]:
        return "bg-bad", "checks failing"
    if view["merge_state"] == "DIRTY" or view["mergeable"] == "CONFLICTING":
        return "bg-bad", "merge conflict"
    if c["pending"]:
        return "bg-run pulse", "checks running"
    if view["merge_state"] == "BEHIND":
        return "bg-warn", "behind main"
    if q["state"] == "open":
        return "bg-warn", "open, auto-merge off"
    if q["state"] == "unknown":
        return "bg-warn", "auto-merge off, merge queue unknown"
    return "bg-ok", "green, merging"


def pr_needs_you(view):
    """Why the PR is in the overview's "Needs you" list, or None: only a failed User review waits
    for the user's review; a PR the queue dropped needs a fix and land.sh again; and one that
    nothing moves on, open without auto-merge, outside the queue and not waiting for the user, is
    stuck (docs/loop.md, the merge queue)."""
    if view["draft"]:
        return None
    if view["queue"]["state"] == "waits-for-user":
        return "PR waits for your review and merge"
    if view["queue"]["state"] == "left-queue":
        return f"the merge queue dropped it: {view['queue']['reason']}"
    if view["queue"]["state"] == "open":
        return "open without auto-merge and not in the merge queue"
    return None


def open_pr_view(p):
    view = {
        "number": p["number"], "title": p["title"], "draft": p["isDraft"],
        "branch": p["headRefName"], "url": p["url"], "bead": pr_bead(p),
        "updated": p.get("updatedAt"), "labels": [l["name"] for l in p.get("labels") or []],
        "auto_merge": bool(p.get("autoMergeRequest")), "queue": pr_queue_state(p),
        "merge_state": p.get("mergeStateStatus"), "mergeable": p.get("mergeable"),
        "review": p.get("reviewDecision") or "",
        "checks": summarize_checks(p.get("statusCheckRollup")),
        "size": f"+{p.get('additions', 0)} −{p.get('deletions', 0)} · {p.get('changedFiles', 0)} files",
    }
    dot, text = pr_status(view)
    view.update(status={"dot": dot, "text": text}, needs_you=pr_needs_you(view))
    return view


# --- sessions: worktrees, processes, transcripts -----------------------------------------

def worktrees():
    out = run(["git", "worktree", "list", "--porcelain"], cwd=ROOT)
    trees, cur = [], {}
    for line in out.splitlines() + [""]:
        if not line:
            if cur:
                trees.append(cur)
            cur = {}
        elif line.startswith("worktree "):
            cur["path"] = line[9:]
        elif line.startswith("branch "):
            cur["branch"] = line[7:].removeprefix("refs/heads/")
        elif line == "detached":
            cur["branch"] = "(detached)"
        elif line.startswith("prunable"):
            cur["prunable"] = True
    for t in trees:
        m = BEAD_BRANCH.match(t.get("branch", ""))
        t["bead"] = m.group(1) if m else None
        t["name"] = Path(t["path"]).name if t["path"] != str(ROOT) else "main checkout"
    return trees


def parse_etime(s):
    """ps etime ([[dd-]hh:]mm:ss) in seconds."""
    days, _, rest = s.rpartition("-")
    parts = [int(x) for x in rest.split(":")]
    while len(parts) < 3:
        parts.insert(0, 0)
    return int(days or 0) * 86400 + parts[0] * 3600 + parts[1] * 60 + parts[2]


def claude_processes():
    """Running Claude Code processes with their working directory and start time."""
    p = subprocess.run(["lsof", "-a", "-d", "cwd", "-c", "claude", "-Fpn"],
                       capture_output=True, text=True, timeout=15)
    procs, pid = {}, None
    for line in p.stdout.splitlines():
        if line.startswith("p"):
            pid = int(line[1:])
        elif line.startswith("n") and pid:
            procs[pid] = {"pid": pid, "cwd": line[1:]}
    if not procs:
        return []
    ps = run(["ps", "-o", "pid=,etime=,pcpu=,command=", "-p", ",".join(map(str, procs))])
    now, out = time.time(), []
    for line in ps.splitlines():
        parts = line.split(None, 3)
        if len(parts) < 4 or int(parts[0]) not in procs:
            continue
        cmd = parts[3]
        if "/claude" not in cmd and not cmd.startswith("claude"):
            continue
        pr = procs[int(parts[0])]
        pr["elapsed"], pr["cpu"] = parts[1], float(parts[2])
        pr["start"] = now - parse_etime(parts[1])
        pr["kind"] = "desktop app" if "stream-json" in cmd else "terminal"
        m = re.search(r"--resume[= ](\S+)", cmd)
        pr["resume"] = m.group(1) if m else None
        out.append(pr)
    return out


class Transcripts:
    """Incremental reader of the Claude Code transcripts of this repository's sessions."""

    def __init__(self):
        self.files = {}
        self.lock = threading.Lock()

    def scan(self):
        base = Path.home() / ".claude" / "projects"
        prefix = re.sub(r"[^A-Za-z0-9]", "-", str(ROOT))
        seen = set()
        with self.lock:
            for d in base.iterdir() if base.is_dir() else []:
                if d.name != prefix and not d.name.startswith(prefix + "-"):
                    continue
                for f in d.glob("*.jsonl"):
                    seen.add(f)
                    self.update(f)
            for f in list(self.files):
                if f not in seen:
                    del self.files[f]
            root = str(ROOT)
            return [dict(e["s"]) for e in self.files.values()
                    if e["s"]["last"] and e["s"]["cwd"]
                    and (e["s"]["cwd"] == root or e["s"]["cwd"].startswith(root + "/"))]

    def update(self, f):
        size = f.stat().st_size
        ent = self.files.get(f)
        if ent is None or size < ent["offset"]:
            ent = self.files[f] = {"offset": 0, "s": {
                "id": f.stem, "title": None, "cwd": None, "branch": None, "first": None,
                "last": None, "busy": False, "turn_end": None, "last_msg": None, "bins": {}}}
        if size == ent["offset"]:
            return
        with open(f, "rb") as fh:
            fh.seek(ent["offset"])
            data = fh.read()
        end = data.rfind(b"\n")
        if end < 0:
            return
        ent["offset"] += end + 1
        for line in data[:end].split(b"\n"):
            try:
                self.feed(ent["s"], json.loads(line))
            except (ValueError, AttributeError, TypeError):
                continue

    @staticmethod
    def feed(s, e):
        kind = e.get("type")
        if kind == "custom-title":
            s["title"] = e.get("customTitle") or s["title"]
            return
        if kind not in ("user", "assistant", "system", "attachment") or e.get("isSidechain"):
            return
        when = iso_to_epoch(e.get("timestamp"))
        if when is None:
            return
        s["first"] = s["first"] or when
        s["last"] = max(s["last"] or 0, when)
        s["cwd"] = e.get("cwd") or s["cwd"]
        s["branch"] = e.get("gitBranch") or s["branch"]
        msg = e.get("message") if isinstance(e.get("message"), dict) else {}
        content = msg.get("content")
        if kind == "assistant":
            m = BEAD_BRANCH.match(s["branch"] or "")
            key = (int(when // ACTIVITY_BIN) * ACTIVITY_BIN, m.group(1) if m else "")
            s["bins"][key] = s["bins"].get(key, 0) + 1
            if msg.get("stop_reason") in ("end_turn", "stop_sequence"):
                s["busy"], s["turn_end"] = False, when
            else:
                s["busy"] = True
            for c in reversed(content if isinstance(content, list) else []):
                if c.get("type") == "text" and c.get("text", "").strip():
                    s["last_msg"] = c["text"].strip()[:280]
                    break
                if c.get("type") == "tool_use":
                    inp = c.get("input") or {}
                    detail = inp.get("description") or inp.get("command") or inp.get("file_path")
                    if detail:
                        s["last_msg"] = f"[{c.get('name')}] {str(detail)[:200]}"
                        break
        elif kind == "user" and not e.get("isMeta"):
            if isinstance(content, str):
                text = content
            else:
                text = next((c.get("text", "") for c in content or [] if c.get("type") == "text"), "")
                if any(c.get("type") == "tool_result" for c in content or []):
                    text = text or "tool_result"
            if "[Request interrupted" in text:
                s["busy"], s["turn_end"] = False, when
            elif text and not text.lstrip().startswith("<local-command"):
                s["busy"] = True
        elif kind == "system" and e.get("subtype") == "stop_hook_summary":
            s["busy"], s["turn_end"] = False, when


TRANSCRIPTS = Transcripts()


def loop_status(path):
    """scripts/loop/status.sh of the worktree's own branch, parsed into fields."""
    script = Path(path) / "scripts" / "loop" / "status.sh"
    if not script.exists():
        return None
    try:
        out = run(["bash", str(script)], cwd=path, timeout=60)
    except (RuntimeError, subprocess.TimeoutExpired) as e:
        return {"error": str(e)}
    fields = {}
    for line in out.splitlines():
        m = re.match(r"\s+([a-z ]+):\s+(.*)", line)
        if m:
            fields[m.group(1).strip()] = m.group(2).strip()
    return fields


def collect_sessions(with_loops=True):
    trees = worktrees()
    procs = claude_processes()
    sessions = TRANSCRIPTS.scan()
    root = str(ROOT)
    procs = [p for p in procs if p["cwd"] == root or p["cwd"].startswith(root + "/")]

    # A process belongs to the session it resumes, else to the newest unmatched session in
    # its directory, preferring one written since the process started.
    by_id = {s["id"]: s for s in sessions}
    for p in sorted(procs, key=lambda p: -p["start"]):
        s = by_id.get(p["resume"]) if p["resume"] else None
        if s is None or s.get("procs"):
            cands = sorted((s for s in sessions if s["cwd"] == p["cwd"] and not s.get("procs")),
                           key=lambda s: (s["last"] >= p["start"] - 120, s["last"]), reverse=True)
            s = cands[0] if cands else None
        if s is None:
            s = {"id": f"pid-{p['pid']}", "title": None, "cwd": p["cwd"], "branch": None,
                 "first": None, "last": None, "busy": False, "turn_end": None,
                 "last_msg": None, "bins": {}}
            sessions.append(s)
        s.setdefault("procs", []).append(p)

    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        loops = {t["path"]: pool.submit(loop_status, t["path"])
                 for t in trees if t["bead"] and with_loops}
        loops = {k: f.result() for k, f in loops.items()}
    return {"trees": trees, "sessions": sessions, "loops": loops}


def tree_of(path, trees):
    best = None
    for t in trees:
        if path and (path == t["path"] or path.startswith(t["path"] + "/")):
            if best is None or len(t["path"]) > len(best["path"]):
                best = t
    return best


# --- assembling the view -----------------------------------------------------------------

def session_state(s, now):
    if not s.get("procs"):
        return "ended"
    if not s["busy"]:
        return "waiting"
    age = now - (s["last"] or 0)
    return "working" if age < QUIET_AFTER else "quiet" if age < STALE_AFTER else "stale"


def assemble(raw, now):
    B, P, S = raw.get("beads"), raw.get("prs"), raw.get("sessions")
    beads = B["beads"] if B else []
    by_id = {b["id"]: b for b in beads}
    children = {}
    for b in beads:
        if b.get("parent"):
            children.setdefault(b["parent"], []).append(b)

    prs_by_bead = {}
    for p in sorted(P["all"] if P else [], key=lambda p: p["createdAt"], reverse=True):
        if pr_bead(p):
            prs_by_bead.setdefault(pr_bead(p), []).append(
                {"number": p["number"], "url": p["url"], "state": p["state"], "merged": p["mergedAt"]})

    # Agent sessions.
    agents = []
    for s in (S["sessions"] if S else []):
        state = session_state(s, now)
        if state == "ended" and now - (s["last"] or 0) > ENDED_WINDOW:
            continue
        tree = tree_of(s["cwd"], S["trees"])
        # The branch names the bead; a session that has not branched yet carries it in its title.
        m = (BEAD_BRANCH.match(s["branch"] or (tree or {}).get("branch") or "")
             or BEAD_TITLE.match(s["title"] or ""))
        bead = by_id.get(m.group(1)) if m else None
        orchestrator = bool(re.match(r"bead loop", s["title"] or "", re.I)
                            or "bead-loop" in (s["branch"] or ""))
        agents.append({
            "id": s["id"], "title": s["title"], "state": state,
            "worktree": tree["name"] if tree else s["cwd"], "branch": s["branch"],
            "bead": m.group(1) if m else None,
            "bead_title": bead["title"] if bead else None,
            "bead_status": bead["status"] if bead else None,
            "orchestrator": orchestrator,
            "procs": [{k: p[k] for k in ("pid", "kind", "elapsed", "cpu")} for p in s.get("procs", [])],
            "first": epoch_to_iso(s["first"]), "last": epoch_to_iso(s["last"]),
            "turn_end": epoch_to_iso(s["turn_end"]), "last_msg": s["last_msg"],
            "loop": S["loops"].get(tree["path"]) if tree and tree.get("bead") == (m.group(1) if m else None) else None,
            "pr": (prs_by_bead.get(m.group(1)) or [None])[0] if m else None,
        })
    order = {"working": 0, "quiet": 1, "stale": 2, "waiting": 3, "ended": 4}
    agents.sort(key=lambda a: (order[a["state"]], -(iso_to_epoch(a["last"]) or 0)))
    live_by_bead = {}
    for a in agents:
        if a["state"] != "ended" and a["bead"]:
            live_by_bead.setdefault(a["bead"], []).append(a)

    # Beads in progress, checked against PRs and agents.
    in_progress, split = [], []
    for b in sorted((b for b in beads if b["status"] == "in_progress"),
                    key=lambda b: (b.get("priority", 9), b["id"])):
        row = slim(b)
        if "loop-split" in (b.get("labels") or []):
            kids = sorted(children.get(b["id"], []), key=lambda c: c["id"])
            row["children"] = [{"id": c["id"], "title": c["title"], "status": c["status"],
                                "live": bool(live_by_bead.get(c["id"]))} for c in kids]
            row["flag"] = ("done" if kids and all(c["status"] == "closed" for c in kids) else None)
            split.append(row)
            continue
        prs = prs_by_bead.get(b["id"], [])
        row["pr"] = next((p for p in prs if p["state"] == "OPEN"), prs[0] if prs else None)
        row["agents"] = [{"id": a["id"], "state": a["state"], "worktree": a["worktree"]}
                         for a in live_by_bead.get(b["id"], [])]
        if row["pr"] and row["pr"]["state"] == "MERGED":
            row["flag"] = "merged"
        elif not row["agents"] and not (row["pr"] and row["pr"]["state"] == "OPEN"):
            row["flag"] = "orphan"
        else:
            row["flag"] = None
        in_progress.append(row)

    view = {"agents": agents, "in_progress": in_progress, "split": split}

    if B:
        counts = {}
        for b in beads:
            counts[b["status"]] = counts.get(b["status"], 0) + 1
        counts["ready"] = len(B["ready"])
        counts["blocked_by_deps"] = len(B["blocked"])
        counts["working_on"] = len(in_progress)
        counts["split_parents"] = len(split)
        closed = sorted((b for b in beads if b["status"] == "closed"),
                        key=lambda b: b.get("closed_at") or "", reverse=True)
        epic_rows = []
        for e in B["epics"]:
            ep = e.get("epic", {})
            if ep.get("status") == "closed":
                continue
            epic_rows.append({"id": ep.get("id"), "title": ep.get("title"),
                              "priority": ep.get("priority"), "status": ep.get("status"),
                              "total": e.get("total_children", 0),
                              "closed": e.get("closed_children", 0),
                              "eligible": e.get("eligible_for_close", False)})
        epic_rows.sort(key=lambda e: (-(e["closed"] / e["total"] if e["total"] else 0),
                                      e["priority"] or 9, e["id"]))
        by_prio = lambda b: (b.get("priority", 9), b.get("created_at", ""))
        view.update({
            "counts": counts,
            "by_priority": {str(p): sum(1 for b in beads if b["status"] != "closed"
                                        and b.get("priority") == p) for p in range(5)},
            "human": [slim(b) for b in beads
                      if b["status"] != "closed" and "human" in (b.get("labels") or [])],
            "ready": [slim(b) for b in sorted(B["ready"], key=by_prio) if b.get("issue_type") != "epic"],
            "recently_closed": [dict(slim(b), pr=(prs_by_bead.get(b["id"]) or [None])[0])
                                for b in closed[:10]],
            "epics": epic_rows,
        })

    if P:
        view["prs_open"] = [open_pr_view(p) for p in P["open"]]
        view["prs_merged"] = [{"number": p["number"], "title": p["title"], "url": p["url"],
                               "merged": p["mergedAt"]}
                              for p in sorted((p for p in P["all"] if p["mergedAt"]),
                                              key=lambda p: p["mergedAt"], reverse=True)[:8]]

    # Raw series for the history charts; the page bins them for the chosen range.
    view["history"] = {
        # A split bead's estimate passes to its children: it counts until the first child exists.
        "beads": [[iso_to_epoch(b.get("created_at")),
                   min(iso_to_epoch(c.get("created_at")) or now for c in children[b["id"]])
                   if b["id"] in children else iso_to_epoch(b.get("closed_at")),
                   b.get("estimated_minutes") or 0, b["id"], b["title"], iso_to_epoch(b.get("closed_at"))]
                  for b in beads if b.get("issue_type") != "epic"],
        "prs": [[iso_to_epoch(p["createdAt"]), iso_to_epoch(p["mergedAt"]), p["number"], p["title"]]
                for p in (P["all"] if P else [])],
        "lanes": [{"id": s["id"], "title": s["title"] or Path(s["cwd"] or "?").name,
                   "live": bool(s.get("procs")),
                   "bins": sorted([t, n, bead] for (t, bead), n in s["bins"].items())}
                  for s in (S["sessions"] if S else []) if s["bins"]],
    }
    return view


# --- count snapshots ---------------------------------------------------------------------

class Snapshots:
    KEYS = ("working_on", "ready", "blocked_by_deps", "open", "closed", "agents_working",
            "agents_live", "prs_open")

    def __init__(self):
        self.path = STATE_DIR / "history.jsonl"
        self.rows = []
        try:
            for line in self.path.read_text().splitlines():
                try:
                    self.rows.append(json.loads(line))
                except ValueError:
                    pass
        except OSError:
            pass

    def record(self, view, now):
        c = view.get("counts")
        if not c:
            return
        row = {"t": int(now)}
        row.update({k: c.get(k, 0) for k in ("working_on", "ready", "blocked_by_deps", "open", "closed")})
        row["agents_working"] = sum(a["state"] == "working" for a in view["agents"])
        row["agents_live"] = sum(a["state"] != "ended" for a in view["agents"])
        row["prs_open"] = len(view.get("prs_open", []))
        last = self.rows[-1] if self.rows else None
        if last and now - last["t"] < SNAPSHOT_EVERY and all(last.get(k) == row[k] for k in self.KEYS):
            return
        self.rows.append(row)
        try:
            STATE_DIR.mkdir(exist_ok=True)
            with open(self.path, "a") as fh:
                fh.write(json.dumps(row) + "\n")
        except OSError:
            pass


# --- code map ----------------------------------------------------------------------------
# The Rust sources of origin/main as modules (one per file), the `use` dependencies between
# them, their change history, and the files open PRs and running sessions are changing now.

CODE = {"sha": None, "static": None, "fetched": 0}
CODE_LOCK = threading.Lock()
USE_RE = re.compile(r"\b(?:pub(?:\([^)]*\))?\s+)?use\s+([^;]+);")
INLINE_RE = re.compile(r"\b(crate|super|self|archicraft_\w+)((?:::\w+)+)")
MOD_RE = re.compile(r"^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*[;{]", re.M)
ITEM_RE = re.compile(r"\bpub(?:\([^)]*\))?\s+(?:struct|enum|fn|trait|type|const|static|union|macro_rules!)\s+(\w+)")


def cat_files(sha, paths):
    """Contents of <sha>:<path> for each path, through one git cat-file --batch."""
    p = subprocess.run(["git", "cat-file", "--batch"], cwd=ROOT, capture_output=True, timeout=60,
                       input="".join(f"{sha}:{x}\n" for x in paths).encode())
    out, data, i = {}, p.stdout, 0
    for path in paths:
        nl = data.index(b"\n", i)
        header = data[i:nl].split()
        i = nl + 1
        if len(header) < 3 or header[1] != b"blob":
            continue
        size = int(header[2])
        out[path] = data[i:i + size].decode("utf-8", "replace")
        i += size + 1
    return out


def strip_comments(src):
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def expand_use(tree):
    """`a::{b, c::{d, e as f}}` -> ["a::b", "a::c::d", "a::c::e"]; `self` names the prefix."""
    tree = re.sub(r"\s+", "", tree)
    i = tree.find("{")
    if i < 0:
        return [re.sub(r"as\w+$", "", tree.split(" as ")[0])] if tree else []
    prefix, inner = tree[:i], tree[i + 1:tree.rfind("}")]
    parts, depth, cur = [], 0, ""
    for ch in inner:
        if ch == "," and depth == 0:
            parts.append(cur)
            cur = ""
            continue
        depth += ch == "{"
        depth -= ch == "}"
        cur += ch
    parts.append(cur)
    out = []
    for part in filter(None, parts):
        for sub in expand_use(part):
            out.append(prefix.rstrip(":") if sub == "self" else prefix + sub)
    return out


def module_info(path, dirs):
    """(crate, kind, module path, group) of crates/<crate>/<kind>/... ."""
    _, crate, kind, *rest = path.split("/")
    name = rest[-1][:-3]
    if kind != "src":
        return crate, kind, f"{kind}::{'::'.join(rest)[:-3]}", kind
    segs = rest[:-1] + ([] if name in ("mod",) or (not rest[:-1] and name in ("lib", "main")) else [name])
    if not rest[:-1] and name == "main":
        segs = ["main"]
    module = "::".join(segs)
    top = f"crates/{crate}/src/{segs[0]}" if segs else None
    group = segs[0] if segs and (len(segs) > 1 or top in dirs) else "src"
    return crate, kind, module, group


def build_code_static(sha):
    names = [n for n in run(["git", "ls-tree", "-r", "-z", "--name-only", sha, "crates"]).split("\0") if n]
    rs = [n for n in names if n.endswith(".rs") and n.count("/") >= 3]
    tomls = [n for n in names if re.fullmatch(r"crates/[^/]+/Cargo\.toml", n)]
    dirs = {n.rsplit("/", 1)[0] for n in rs}
    dirs |= {d.rsplit("/", i)[0] for d in list(dirs) for i in range(1, d.count("/"))}
    texts = cat_files(sha, rs + tomls)

    crates = {}
    for t in tomls:
        name = t.split("/")[1]
        deps = []
        section = None
        for line in texts.get(t, "").splitlines():
            m = re.match(r"\s*\[([^\]]+)\]", line)
            if m:
                section = m.group(1)
                continue
            m = re.match(r"\s*(archicraft-[a-z]+)\b", line)
            if m and section in ("dependencies", "dev-dependencies"):
                deps.append({"crate": m.group(1), "dev": section == "dev-dependencies"})
        crates[name] = {"name": name, "deps": deps}
    depth = {}
    def layer(c, seen=()):
        if c not in depth:
            depth[c] = 1 + max([layer(d["crate"], seen + (c,)) for d in crates.get(c, {"deps": []})["deps"]
                                if not d["dev"] and d["crate"] in crates and d["crate"] not in seen] or [-1])
        return depth[c]
    for c in crates:
        crates[c]["layer"] = layer(c)

    nodes, index = {}, {}
    for path in rs:
        crate, kind, module, group = module_info(path, dirs)
        text = texts.get(path, "")
        nodes[path] = {"id": path, "crate": crate, "kind": kind, "module": module or "(crate root)",
                       "group": group, "lines": text.count("\n") + (1 if text and not text.endswith("\n") else 0)}
        if kind == "src":
            index[(crate, module)] = path

    # Re-exports: `pub use` in a module makes an item name resolve to the module it comes from.
    def resolve(crate, segs):
        for n in range(len(segs), -1, -1):
            if (crate, "::".join(segs[:n])) in index:
                return index[(crate, "::".join(segs[:n]))], n
        return None, 0
    reexports, globs = {}, {}
    defines = {path: set(ITEM_RE.findall(strip_comments(texts[path]))) for path in nodes}
    for path, node in nodes.items():
        if node["kind"] != "src":
            continue
        cur = [] if node["module"] == "(crate root)" else node["module"].split("::")
        for m in USE_RE.finditer(strip_comments(texts[path])):
            if not m.group(0).lstrip().startswith("pub"):
                continue
            for full in expand_use(m.group(1)):
                segs = [s for s in full.split("::") if s]
                if not segs:
                    continue
                if segs[0] == "crate":
                    target, _ = resolve(node["crate"], segs[1:-1])
                elif segs[0] in ("self", "super") or (node["crate"], "::".join(cur + segs[:1])) in index:
                    base = list(cur)
                    while segs and segs[0] in ("self", "super"):
                        base = base[:-1] if segs.pop(0) == "super" else base
                    target, _ = resolve(node["crate"], base + segs[:-1])
                else:
                    continue
                if target and segs[-1] == "*":
                    globs.setdefault((node["crate"], node["module"]), []).append(target)
                elif target:
                    reexports[(node["crate"], node["module"], segs[-1])] = target

    edges = {}
    for path, node in nodes.items():
        src = strip_comments(texts[path])
        own = node["crate"] if node["kind"] == "src" else None
        cur = [] if node["kind"] != "src" or node["module"] == "(crate root)" else node["module"].split("::")
        children = set(MOD_RE.findall(src))
        refs = [p for m in USE_RE.finditer(src) for p in expand_use(m.group(1))]
        refs += [m.group(1) + m.group(2) for m in INLINE_RE.finditer(USE_RE.sub(" ", src))]
        for ref in refs:
            segs = [s for s in ref.split("::") if s and s != "*"]
            if not segs:
                continue
            head = segs[0]
            if head.startswith("archicraft_"):
                crate, rest = head.replace("_", "-"), segs[1:]
            elif own and head == "crate":
                crate, rest = own, segs[1:]
            elif own and head in ("self", "super"):
                base = list(cur)
                while segs and segs[0] in ("self", "super"):
                    base = base[:-1] if segs.pop(0) == "super" else base
                crate, rest = own, base + segs
            elif own and head in children:
                crate, rest = own, cur + segs
            else:
                continue
            if crate not in crates:
                continue
            target, used = resolve(crate, rest)
            if target and used < len(rest):
                key = (crate, nodes[target]["module"], rest[used])
                found = reexports.get(key) or next(
                    (g for g in globs.get(key[:2], []) if rest[used] in defines[g]), None)
                # A name the crate root neither re-exports from a module nor defines (an external
                # crate it re-exports, such as glam) is not a dependency on the root.
                target = found or (None if used == 0 and rest[used] not in defines[target] else target)
            if target and target != path:
                edges[(path, target)] = edges.get((path, target), 0) + 1

    # History of origin/main: per file the commits that touched it, with lines added and deleted.
    log = run(["git", "log", sha, "--no-renames", "--numstat", "-n", "1000",
               "--format=%x01%h%x02%ct%x02%s", "--", "crates"], timeout=120)
    commits, touched = [], {}
    for block in log.split("\x01")[1:]:
        head, _, body = block.partition("\n")
        short, ct, subject = head.split("\x02", 2)
        m = BEAD_TITLE.match(subject)
        pr = re.search(r"\(#(\d+)\)\s*$", subject)
        ci = len(commits)
        commits.append({"sha": short, "t": int(ct), "subject": subject, "bead": m.group(1) if m else None,
                        "pr": int(pr.group(1)) if pr else None})
        for line in body.splitlines():
            parts = line.split("\t")
            if len(parts) == 3 and parts[2] in nodes:
                add = int(parts[0]) if parts[0].isdigit() else 0
                rem = int(parts[1]) if parts[1].isdigit() else 0
                touched.setdefault(parts[2], []).append([ci, add, rem])
    for path, node in nodes.items():
        node["history"] = touched.get(path, [])

    return {"sha": sha, "crates": list(crates.values()), "nodes": list(nodes.values()),
            "edges": [[a, b, w] for (a, b), w in edges.items()], "commits": commits}


def collect_code():
    now = time.time()
    if now - CODE["fetched"] > 120:
        subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, capture_output=True, timeout=60)
        CODE["fetched"] = now
    sha = run(["git", "rev-parse", "--short", "origin/main"]).strip()
    with CODE_LOCK:
        if CODE["sha"] != sha:
            CODE["static"], CODE["sha"] = build_code_static(sha), sha

    # Files running sessions are changing in their worktrees, committed or not.
    def worktree_changes(t):
        try:
            files = set(run(["git", "diff", "--name-only", "origin/main...HEAD", "--", "crates"],
                            cwd=t["path"]).split())
            dirty = {l[3:].split(" -> ")[-1] for l in run(["git", "status", "--porcelain", "--", "crates"],
                                                          cwd=t["path"]).splitlines() if len(l) > 3}
        except RuntimeError:
            return None
        return {"path": t["path"], "name": t["name"], "branch": t.get("branch"),
                "files": sorted(files | dirty), "dirty": sorted(dirty)}
    trees = [t for t in worktrees() if t["path"] != str(ROOT) and not t.get("prunable")]
    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        changes = [c for c in pool.map(worktree_changes, trees) if c and c["files"]]
    return {"static": CODE["static"], "worktrees": changes}


def code_view():
    """The code map with what open PRs and running sessions are changing right now."""
    raw = collect_code()
    with LOCK:
        agents = STATE.get("agents") or []
        prs = (RAW.get("prs") or {}).get("open") or []
    live = {}
    for p in prs:
        for f in p.get("files") or []:
            live.setdefault(f["path"], []).append({"kind": "pr", "label": f"PR #{p['number']}", "title": p["title"],
                                                   "url": p["url"], "add": f.get("additions", 0),
                                                   "del": f.get("deletions", 0)})
    running = {}
    for a in agents:
        if a["state"] != "ended":
            running.setdefault(a["worktree"], []).append(a)
    for w in raw["worktrees"]:
        ags = running.get(w["name"])
        if not ags and not w["dirty"]:
            continue  # a finished worktree: its branch still differs from main, nothing is happening
        who = ags[0] if ags else None
        for f in w["files"]:
            live.setdefault(f, []).append({
                "kind": "session" if who else "worktree", "label": (who and (who["bead"] or who["title"])) or w["name"],
                "title": f"{w['name']} · {w['branch']}" + (f" · {STATES_TEXT.get(who['state'], who['state'])}" if who else ""),
                "uncommitted": f in w["dirty"], "state": who["state"] if who else None})
    return {**raw["static"], "live": live, "generated": epoch_to_iso(time.time())}


STATES_TEXT = {"working": "working", "quiet": "quiet", "stale": "stale", "waiting": "idle"}


# --- roadmap -----------------------------------------------------------------------------
# docs/roadmap.md (epics, increments, dependencies, engineer-days) and
# docs/implementation-status.md (one status per SRS requirement) as they are on origin/main,
# joined with the live beads: how far each epic, each increment and the SRS have come.

ROADMAP = {"sha": None, "doc": None}
REQ_ROW = re.compile(r"^\| ([A-Z]+-[A-Z0-9-]+) \| ([MSC]) \| ([a-z-]+) \|")
AS_ROW = re.compile(r"^\| (AS-\d+) \| ([a-z-]+) \|")


def parse_roadmap(text):
    epics, ids, exits = [], {}, {}
    for m in re.finditer(r"^### \d+\. `([^`]+)` \((archicraft-[a-z0-9]+)\)", text, re.M):
        ids[m.group(1)] = m.group(2)
    for m in re.finditer(r"^### Increment (\d+)[^\n]*\n\nExit criterion: ([^\n]+)", text, re.M):
        exits[m.group(1)] = m.group(2).strip()
    table = text[text.index("## Epic table"):]
    for line in table.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 8 or not cells[0].isdigit():
            continue
        msc = [int(x) for x in cells[6].split("/")] if re.fullmatch(r"\d+/\d+/\d+/\d+", cells[6]) else [0, 0, 0, 0]
        key = cells[1].strip("`")
        epics.append({"n": int(cells[0]), "key": key, "title": cells[2], "inc": int(cells[3]),
                      "prio": int(cells[4]), "days": int(cells[5]), "must": msc[0], "should": msc[1],
                      "could": msc[2], "done_reqs": msc[3], "id": ids.get(key),
                      "deps": re.findall(r"`([^`]+)`", cells[7])})
    # Earliest start in engineer-days with unlimited parallelism, as the roadmap's schedule shape.
    by_key = {e["key"]: e for e in epics}
    def start(e, seen=()):
        if "start" not in e:
            e["start"] = max([start(by_key[d], seen + (e["key"],)) + by_key[d]["days"]
                              for d in e["deps"] if d in by_key and d not in seen] or [0])
        return e["start"]
    for e in epics:
        start(e)
    return {"epics": epics, "exits": exits}


def parse_status(text):
    reqs, scenarios = {}, {}
    for line in text.splitlines():
        m = REQ_ROW.match(line)
        if m:
            reqs[m.group(1)] = (m.group(2), m.group(3))
            continue
        m = AS_ROW.match(line)
        if m:
            scenarios[m.group(1)] = m.group(2)
    return reqs, scenarios


def status_counts(reqs):
    out = {}
    for prio, status in reqs.values():
        out.setdefault(prio, {}).setdefault(status, 0)
        out[prio][status] += 1
    return out


def build_roadmap_doc(sha):
    roadmap = parse_roadmap(run(["git", "show", f"{sha}:docs/roadmap.md"]))
    reqs, scenarios = parse_status(run(["git", "show", f"{sha}:docs/implementation-status.md"]))
    history = []
    for line in run(["git", "log", "--format=%h %ct", sha, "--", "docs/implementation-status.md"]).splitlines():
        h, ct = line.split()
        try:
            r, _ = parse_status(run(["git", "show", f"{h}:docs/implementation-status.md"]))
        except RuntimeError:
            continue
        c = status_counts(r)
        history.append({"t": int(ct), "sha": h,
                        **{f"{s}_{p}": c.get(p, {}).get(s, 0) for p in "MSC" for s in ("done", "partial")}})
    return {"sha": sha, **roadmap, "reqs": status_counts(reqs), "scenarios": scenarios,
            "req_history": sorted(history, key=lambda r: r["t"])}


def roadmap_view():
    sha = run(["git", "rev-parse", "--short", "origin/main"]).strip()
    if ROADMAP["sha"] != sha:
        ROADMAP["doc"], ROADMAP["sha"] = build_roadmap_doc(sha), sha
    doc = ROADMAP["doc"]
    with LOCK:
        agents = [a for a in STATE.get("agents") or [] if a["state"] != "ended" and a["bead"]]
    raw_beads = RAW.get("beads") or collect_beads()  # before the first refresh has finished
    beads = raw_beads["beads"]
    ready = {b["id"] for b in raw_beads["ready"]}
    by_id = {b["id"]: b for b in beads}
    kids = {}
    for b in beads:
        if b.get("parent"):
            kids.setdefault(b["parent"], []).append(b)
    live = {a["bead"]: a for a in agents}
    now = time.time()

    def leaves(bid):
        out = []
        for c in kids.get(bid, []):
            out.extend(leaves(c["id"]) if c["id"] in kids else [c])
        return out
    epics = []
    for e in doc["epics"]:
        b = by_id.get(e["id"]) or {}
        ls = leaves(e["id"]) if e["id"] else []
        est = lambda x: x.get("estimated_minutes") or 0
        total = sum(est(x) for x in ls) or 0
        done = sum(est(x) for x in ls if x["status"] == "closed")
        # A task under the epic that was split counts through its children.
        tasks = sorted(kids.get(e["id"], []), key=lambda x: (x["status"] == "closed", x.get("priority", 9), x["id"]))
        task_rows = []
        for t in tasks:
            tl = leaves(t["id"]) if t["id"] in kids else [t]
            task_rows.append({"id": t["id"], "title": t["title"], "status": t["status"],
                              "est": sum(est(x) for x in tl), "done": sum(est(x) for x in tl if x["status"] == "closed"),
                              "children": len(tl) if t["id"] in kids else 0,
                              "ready": t["id"] in ready or any(x["id"] in ready for x in tl),
                              "live": [live[x["id"]]["state"] for x in tl if x["id"] in live]})
        state = ("done" if b.get("status") == "closed" else
                 "in_progress" if any(x["status"] in ("in_progress", "closed") for x in ls) else "planned")
        epics.append({**e, "status": b.get("status"), "state": state, "est_total": total, "est_done": done,
                      "leaves": len(ls), "leaves_done": sum(x["status"] == "closed" for x in ls),
                      "in_progress": sum(x["status"] == "in_progress" for x in ls),
                      "live": [{"bead": x["id"], "state": live[x["id"]]["state"]} for x in ls if x["id"] in live],
                      "tasks": task_rows})

    # Pace: estimates closed per day over the last 7 days, from the beads under any roadmap epic.
    under = {x["id"] for e in doc["epics"] if e["id"] for x in leaves(e["id"])}
    week = sum((b.get("estimated_minutes") or 0) for b in beads if b["id"] in under and b["status"] == "closed"
               and (iso_to_epoch(b.get("closed_at")) or 0) > now - 7 * 86400)
    closed_times = sorted([iso_to_epoch(b.get("closed_at")), b.get("estimated_minutes") or 0]
                          for b in beads if b["id"] in under and b["status"] == "closed" and b.get("closed_at"))
    return {"sha": sha, "epics": epics, "exits": doc["exits"], "reqs": doc["reqs"], "scenarios": doc["scenarios"],
            "req_history": doc["req_history"], "pace_minutes_per_day": week / 7, "closed": closed_times,
            "generated": epoch_to_iso(now)}


# --- cleanup -----------------------------------------------------------------------------
# Candidates are computed fresh for the preview and again right before anything runs, so an
# action only happens to something that still qualifies. Nothing runs without the page's
# confirmation; every action is logged to <git common dir>/loop-dashboard/cleanup.log.

CLEANUP_BRANCHES = re.compile(r"^(bead|claude)/")
CLEANUP_LOCK = threading.Lock()


def is_ancestor(commit, of):
    return subprocess.run(["git", "merge-base", "--is-ancestor", commit, of], cwd=ROOT,
                          capture_output=True).returncode == 0


def cleanup_log(line):
    try:
        STATE_DIR.mkdir(exist_ok=True)
        with open(STATE_DIR / "cleanup.log", "a") as fh:
            fh.write(f"{datetime.now(timezone.utc).isoformat()} {line}\n")
    except OSError:
        pass


def branch_safety(tip, pr):
    """Why deleting a branch at <tip> loses nothing, or None when it would lose commits."""
    if is_ancestor(tip, "origin/main"):
        return "no commits beyond main"
    if pr and pr["state"] in ("MERGED", "CLOSED") and pr.get("headRefOid") and is_ancestor(tip, pr["headRefOid"]):
        return f"its commits are kept in PR #{pr['number']}"
    return None


def compute_cleanup():
    subprocess.run(["git", "fetch", "--quiet", "origin", "main"], cwd=ROOT, capture_output=True, timeout=60)
    now = time.time()
    with concurrent.futures.ThreadPoolExecutor(3) as pool:
        fb, fp, fs = (pool.submit(collect_beads), pool.submit(collect_prs),
                      pool.submit(collect_sessions, False))
        B, P, S = fb.result(), fp.result(), fs.result()
    status = {b["id"]: b["status"] for b in B["beads"]}
    prs_by_branch = {}
    for p in sorted(P["all"], key=lambda p: p["createdAt"], reverse=True):
        prs_by_branch.setdefault(p["headRefName"], []).append(p)
    latest_pr = lambda br: (prs_by_branch.get(br) or [None])[0]
    open_pr = lambda br: any(p["state"] == "OPEN" for p in prs_by_branch.get(br, []))
    trees = S["trees"]
    checked_out = {t.get("branch") for t in trees}
    cands = []

    # Sessions: stale, or idle on a bead that is closed.
    stoppable = set()
    for s in S["sessions"]:
        if not s.get("procs"):
            continue
        state = session_state(s, now)
        m = BEAD_BRANCH.match(s["branch"] or "") or BEAD_TITLE.match(s["title"] or "")
        bead = m.group(1) if m else None
        orchestrator = bool(re.match(r"bead loop", s["title"] or "", re.I) or "bead-loop" in (s["branch"] or ""))
        if state == "stale":
            why = f"inside a turn with no output for {dur_text(now - (s['last'] or now))}"
        elif state in ("waiting", "quiet") and bead and status.get(bead) == "closed" and not orchestrator:
            why = f"{'idle' if state == 'waiting' else 'quiet'} on {bead}, which is closed"
        else:
            continue
        pids = [p["pid"] for p in s["procs"]]
        stoppable.update(pids)
        tree = tree_of(s["cwd"], trees)
        cands.append({"id": f"session:{s['id']}", "kind": "session", "pids": pids,
                      "label": s["title"] or s["id"], "detail": f"{tree['name'] if tree else s['cwd']} · pid {', '.join(map(str, pids))}",
                      "why": why, "action": "stop the process (the transcript stays)"})

    # Worktrees whose work is finished.
    procs_in = {}
    for s in S["sessions"]:
        for p in s.get("procs", []):
            t = tree_of(p["cwd"], trees)
            if t:
                procs_in.setdefault(t["path"], []).append(p["pid"])
    last_in = {}
    for s in S["sessions"]:
        t = tree_of(s["cwd"], trees)
        if t and s["last"]:
            last_in[t["path"]] = max(last_in.get(t["path"], 0), s["last"])
    for t in trees:
        if t["path"] == str(ROOT):
            continue
        cid = f"worktree:{t['path']}"
        if t.get("prunable"):
            cands.append({"id": cid, "kind": "worktree", "label": t["name"], "path": t["path"],
                          "detail": t["path"], "why": "its directory is gone",
                          "action": "git worktree prune", "prune": True})
            continue
        br = t.get("branch")
        if not br or br == "(detached)":
            continue
        pr, bead = latest_pr(br), t.get("bead")
        if bead and status.get(bead) == "in_progress":
            continue
        if bead and status.get(bead) == "closed":
            why = f"bead {bead} is closed"
        elif pr and pr["state"] == "MERGED":
            why = f"PR #{pr['number']} merged"
        elif pr and pr["state"] == "CLOSED":
            why = f"PR #{pr['number']} closed without merging"
        elif not pr and now - last_in.get(t["path"], 0) > ENDED_WINDOW and \
                subprocess.run(["git", "merge-base", "--is-ancestor", "HEAD", "origin/main"],
                               cwd=t["path"], capture_output=True).returncode == 0:
            why = "no commits beyond main and no session for 24 h"
        else:
            continue
        if open_pr(br):
            continue
        blocked = []
        running = procs_in.get(t["path"], [])
        if any(pid not in stoppable for pid in running):
            blocked.append("a session is running in it")
        try:
            dirty = [l for l in run(["git", "status", "--porcelain"], cwd=t["path"]).splitlines() if l]
        except RuntimeError as e:
            dirty, blocked = [], blocked + [str(e)[:120]]
        if dirty:
            blocked.append(f"{len(dirty)} uncommitted or untracked files")
        tip = run(["git", "rev-parse", "HEAD"], cwd=t["path"]).strip()
        safe = branch_safety(tip, pr)
        if not safe:
            blocked.append("commits that are neither on main nor in a merged or closed PR")
        note = " · stops its session first" if running and not blocked else ""
        cands.append({"id": cid, "kind": "worktree", "label": t["name"], "path": t["path"],
                      "branch": br, "pids": running, "detail": f"{br} · {t['path']}",
                      "why": why + (f"; {safe}" if safe and safe not in why else "") + note,
                      "action": f"git worktree remove, then delete branch {br}",
                      "blocked": "; ".join(blocked) or None})

    # Local branches without a worktree.
    kept = []
    for line in run(["git", "for-each-ref", "--format=%(refname:short) %(objectname)", "refs/heads"]).splitlines():
        br, tip = line.split(" ", 1)
        if not CLEANUP_BRANCHES.match(br) or br in checked_out or open_pr(br):
            continue
        m = BEAD_BRANCH.match(br)
        if m and status.get(m.group(1)) == "in_progress":
            continue
        safe = branch_safety(tip, latest_pr(br))
        if safe:
            cands.append({"id": f"branch:{br}", "kind": "branch", "label": br, "branch": br,
                          "detail": "local branch, no worktree", "why": safe, "action": "git branch -D"})
        else:
            kept.append(br)

    # Remote branches of finished work.
    try:
        remote = run(["git", "ls-remote", "--heads", "origin"], timeout=30).splitlines()
    except RuntimeError:
        remote = []
    for line in remote:
        oid, ref = line.split("\t", 1)
        br = ref.removeprefix("refs/heads/")
        if not CLEANUP_BRANCHES.match(br) or open_pr(br) or br in checked_out:
            continue
        pr = latest_pr(br)
        if pr and pr["state"] == "MERGED":
            why = f"PR #{pr['number']} merged"
        elif is_ancestor(oid, "origin/main"):
            why = "no commits beyond main"
        else:
            continue
        cands.append({"id": f"remote:{br}", "kind": "remote", "label": br, "branch": br,
                      "detail": "branch on GitHub", "why": why, "action": f"git push origin --delete {br}"})

    stale_refs = [l.split("[would prune] ", 1)[1] for l in
                  run(["git", "remote", "prune", "origin", "--dry-run"]).splitlines() if "[would prune]" in l]
    if stale_refs:
        cands.append({"id": "prune:origin", "kind": "remote", "label": f"{len(stale_refs)} stale remote-tracking refs",
                      "detail": ", ".join(stale_refs[:6]) + (" …" if len(stale_refs) > 6 else ""),
                      "why": "their branches are gone on GitHub", "action": "git remote prune origin"})
    return {"candidates": cands, "kept_branches": kept, "generated": epoch_to_iso(now)}


def dur_text(sec):
    return f"{round(sec / 60)} min" if sec < 3600 else f"{round(sec / 3600, 1)} h"


def run_cleanup(ids):
    """Runs the selected candidates that still qualify: sessions, then worktrees, then branches."""
    results, wanted = {}, set(ids)

    def fresh():
        return {c["id"]: c for c in compute_cleanup()["candidates"]}

    def do(cid, fn):
        try:
            msg = fn()
            results[cid] = {"ok": True, "message": msg}
            cleanup_log(f"ok   {cid}: {msg}")
        except Exception as e:
            results[cid] = {"ok": False, "message": str(e)[:400]}
            cleanup_log(f"fail {cid}: {e}")

    def stop(pids):
        for pid in pids:
            try:
                os.kill(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
        deadline = time.time() + 5
        while time.time() < deadline and any(alive(p) for p in pids):
            time.sleep(0.2)
        return f"sent SIGTERM to {', '.join(map(str, pids))}"

    with CLEANUP_LOCK:
        cands = fresh()
        for cid in sorted(wanted):
            c = cands.get(cid)
            if c and c["kind"] == "session":
                do(cid, lambda c=c: stop(c["pids"]))
        if any(r["ok"] for r in results.values()):
            cands = fresh()
        for cid in sorted(wanted):
            c = cands.get(cid)
            if not c or c["kind"] != "worktree":
                continue
            if c.get("blocked"):
                results[cid] = {"ok": False, "message": f"skipped: {c['blocked']}"}
                continue
            def remove(c=c):
                if c.get("prune"):
                    run(["git", "worktree", "prune"])
                    return "pruned"
                if c.get("pids"):
                    stop(c["pids"])
                run(["git", "worktree", "remove", c["path"]], timeout=600)
                run(["git", "branch", "-D", c["branch"]])
                return f"removed {c['path']} and branch {c['branch']}"
            do(cid, remove)
        cands = fresh()
        for cid in sorted(wanted):
            c = cands.get(cid)
            if not c or c["kind"] not in ("branch", "remote"):
                continue
            if cid == "prune:origin":
                cmd, msg = ["git", "remote", "prune", "origin"], "pruned stale remote-tracking refs"
            elif c["kind"] == "branch":
                cmd, msg = ["git", "branch", "-D", c["branch"]], f"deleted {c['branch']}"
            else:
                cmd, msg = ["git", "push", "origin", "--delete", c["branch"]], f"deleted origin/{c['branch']}"
            do(cid, lambda cmd=cmd, msg=msg: run(cmd, timeout=60) and msg or msg)
        for cid in wanted - set(results):
            results[cid] = {"ok": False, "message": "no longer a cleanup candidate, skipped"}
    REFRESH.set()
    return results


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def run_sweep():
    """The sweep as the /bead skill runs it: the copy of scripts/loop on origin/main."""
    with CLEANUP_LOCK:
        run(["git", "fetch", "--quiet", "origin", "main"], timeout=60)
        with tempfile.TemporaryDirectory() as tmp:
            archive = subprocess.run(["git", "archive", "origin/main", "scripts/loop"], cwd=ROOT,
                                     capture_output=True, check=True, timeout=60).stdout
            subprocess.run(["tar", "-x", "-C", tmp], input=archive, check=True)
            p = subprocess.run(["bash", f"{tmp}/scripts/loop/sweep.sh"], cwd=ROOT,
                               capture_output=True, text=True, timeout=600)
    out = (p.stdout + p.stderr).strip()
    cleanup_log(f"sweep exit {p.returncode}: {out[:2000]!r}")
    REFRESH.set()
    return {"ok": p.returncode == 0, "output": out or "(no output)"}


# --- refresh loop and server -------------------------------------------------------------

RAW = {}
# The page's errors entry for a failed merge-queue query (collect_prs()'s "queue_error").
QUEUE_ERROR = "merge queue"
STATE = {"generated": None, "errors": {}}
LOCK = threading.Lock()
REFRESH = threading.Event()
SNAPSHOTS = None


def refresh(interval):
    started = time.time()
    sources = {"beads": collect_beads, "prs": collect_prs, "sessions": collect_sessions}
    errors = {}
    with concurrent.futures.ThreadPoolExecutor(3) as pool:
        futures = {k: pool.submit(f) for k, f in sources.items()}
        for k, fut in futures.items():
            try:
                RAW[k] = fut.result()  # a failing source keeps its last good data
            except Exception as e:
                errors[k] = str(e)
    if (RAW.get("prs") or {}).get("queue_error"):
        errors[QUEUE_ERROR] = RAW["prs"]["queue_error"]
    now = time.time()
    view = assemble(RAW, now)
    SNAPSHOTS.record(view, now)
    view["history"]["snapshots"] = SNAPSHOTS.rows[-2000:]
    view.update({"errors": errors, "generated": epoch_to_iso(now), "took": round(now - started, 1),
                 "interval": interval, "root": str(ROOT)})
    with LOCK:
        STATE.clear()
        STATE.update(view)


def refresher(interval):
    while True:
        try:
            refresh(interval)
        except Exception as e:
            with LOCK:
                STATE["errors"] = {"refresh": repr(e)}
        REFRESH.wait(interval)
        REFRESH.clear()


TOKEN = secrets.token_hex(16)
PORT = 8765


class Handler(BaseHTTPRequestHandler):
    def local_host(self):
        # Refuses requests addressed to another host name (DNS rebinding).
        return self.headers.get("Host") in (f"127.0.0.1:{PORT}", f"localhost:{PORT}")

    def do_POST(self):
        # Changes need the page's token: another site cannot read it, so it cannot send it.
        if not self.local_host() or self.headers.get("X-Dashboard-Token") != TOKEN:
            return self.reply(403, "text/plain", b"forbidden")
        try:
            body = json.loads(self.rfile.read(int(self.headers.get("Content-Length") or 0)) or b"{}")
            if self.path == "/api/cleanup/preview":
                result = compute_cleanup()
            elif self.path == "/api/cleanup/run":
                result = run_cleanup([str(i) for i in body.get("ids", [])])
            elif self.path == "/api/sweep":
                result = run_sweep()
            else:
                return self.reply(404, "text/plain", b"not found")
        except Exception as e:
            result = {"error": str(e)[:1000]}
        self.reply(200, "application/json", json.dumps(result).encode())

    def do_GET(self):
        if not self.local_host():
            return self.reply(403, "text/plain", b"forbidden")
        if self.path.startswith("/api/state"):
            if "refresh=1" in self.path:
                REFRESH.set()
            with LOCK:
                body = json.dumps(STATE).encode()
            self.reply(200, "application/json", body)
        elif self.path.startswith("/api/code"):
            try:
                body = json.dumps(code_view()).encode()
            except Exception as e:
                body = json.dumps({"error": str(e)[:1000]}).encode()
            self.reply(200, "application/json", body)
        elif self.path.startswith("/api/roadmap"):
            try:
                body = json.dumps(roadmap_view()).encode()
            except Exception as e:
                body = json.dumps({"error": str(e)[:1000]}).encode()
            self.reply(200, "application/json", body)
        elif self.path in ("/roadmap", "/roadmap/"):
            self.reply(200, "text/html; charset=utf-8", PAGE_ROADMAP.encode())
        elif self.path in ("/code", "/code/"):
            self.reply(200, "text/html; charset=utf-8", PAGE_CODE.encode())
        elif self.path in ("/", "/index.html"):
            self.reply(200, "text/html; charset=utf-8", PAGE.replace("__TOKEN__", TOKEN).encode())
        else:
            self.reply(404, "text/plain", b"not found")

    def reply(self, code, ctype, body):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Cache-Control", "no-store")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *args):
        pass


PAGE = r"""<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ArchiCraft Loop</title>
<style>
:root{color-scheme:light;--bg:#f6f7f9;--card:#fff;--fg:#1c2128;--mute:#667085;--line:#e3e6ea;--accent:#2f6feb;
--ok:#1a7f37;--warn:#b26b00;--bad:#cf222e;--run:#8250df;--chip:#eef1f4;--grid:#eceef1;
--s1:#2a78d6;--s2:#eb6834;--q1:#cde2fb;--q2:#9ec5f4;--q3:#6da7ec;--q4:#3987e5;--q5:#256abf;--q6:#184f95}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){color-scheme:dark;--bg:#0f1216;--card:#171b21;--fg:#e6e9ee;--mute:#8b95a3;
--line:#2a3039;--accent:#5b9bff;--ok:#3fb950;--warn:#d29922;--bad:#f85149;--run:#b083f0;--chip:#232933;--grid:#242a33;
--s1:#3987e5;--s2:#d95926;--q1:#16325a;--q2:#184f95;--q3:#256abf;--q4:#3987e5;--q5:#6da7ec;--q6:#9ec5f4}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:13px/1.45 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
header{display:flex;align-items:center;gap:12px;padding:10px 16px;border-bottom:1px solid var(--line);position:sticky;top:0;background:var(--bg);z-index:5;flex-wrap:wrap}
header h1{font-size:15px;margin:0;font-weight:600}
.tabs{display:inline-flex;gap:2px;background:var(--chip);border-radius:7px;padding:2px}
.tabs a{padding:2px 10px;border-radius:5px;color:var(--mute);font-size:12px}
.tabs a.on{background:var(--card);color:var(--fg);font-weight:600}.tabs a:hover{text-decoration:none;color:var(--fg)}
.live{display:flex;align-items:center;gap:6px;font-size:12px;color:var(--mute)}
.live .dot{margin:0}
.pulse{animation:pulse 1.6s ease-in-out infinite}
@keyframes pulse{50%{opacity:.35}}
header .meta{color:var(--mute);margin-left:auto;font-size:12px}
button,.seg button{font:inherit;background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:3px 10px;cursor:pointer}
.seg{display:inline-flex}.seg button{border-radius:0;margin-left:-1px}.seg button:first-child{border-radius:6px 0 0 6px}.seg button:last-child{border-radius:0 6px 6px 0}
.seg button.on{background:var(--chip);font-weight:600}
main{padding:14px 16px;display:grid;gap:14px;grid-template-columns:repeat(auto-fit,minmax(min(100%,460px),1fr));align-items:start}
section{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 14px;min-width:0}
section.wide{grid-column:1/-1}
h2{font-size:12px;text-transform:uppercase;letter-spacing:.06em;color:var(--mute);margin:0 0 10px;display:flex;gap:8px;align-items:center}
h2 .n{background:var(--chip);color:var(--fg);border-radius:9px;padding:0 7px;font-size:11px;letter-spacing:0}
h2 .right{margin-left:auto;text-transform:none;letter-spacing:0}
h3{font-size:12px;margin:14px 0 6px;font-weight:600}
.kpis{display:flex;flex-wrap:wrap;gap:8px}
.kpi{flex:1 1 110px;background:var(--chip);border-radius:8px;padding:8px 10px;min-width:0}
.kpi b{display:block;font-size:20px;font-variant-numeric:tabular-nums}
.kpi span{color:var(--mute);font-size:11px}
.kpi svg{display:block;margin-top:2px}
table{width:100%;border-collapse:collapse}
td{padding:5px 6px;border-top:1px solid var(--line);vertical-align:top}
tr:first-child td{border-top:0}
a{color:var(--accent);text-decoration:none} a:hover{text-decoration:underline}
.id{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;white-space:nowrap}
.mute{color:var(--mute)} .small{font-size:11px} .nowrap{white-space:nowrap}
.chip{display:inline-block;background:var(--chip);border-radius:4px;padding:0 5px;font-size:11px;margin:1px 2px 1px 0;white-space:nowrap}
.p0,.p1{color:var(--bad)} .p2{color:var(--warn)} .p3,.p4{color:var(--mute)}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px;vertical-align:middle;flex:none}
.ok{color:var(--ok)} .bad{color:var(--bad)} .warn{color:var(--warn)} .run{color:var(--run)}
.bg-ok{background:var(--ok)} .bg-bad{background:var(--bad)} .bg-warn{background:var(--warn)} .bg-run{background:var(--run)} .bg-mute{background:var(--mute)}
.bar{height:6px;background:var(--chip);border-radius:3px;overflow:hidden;min-width:60px}
.bar i{display:block;height:100%;background:var(--ok)}
.agent{border-top:1px solid var(--line);padding:9px 0;display:grid;grid-template-columns:150px 1fr;gap:4px 14px}
.agent:first-of-type{border-top:0;padding-top:0}
.agent .state{font-weight:600;display:flex;align-items:center}
.agent .state small{display:block;font-weight:400;color:var(--mute);font-size:11px}
.agent .what b{font-weight:600}
.agent .last{color:var(--mute);font-size:12px;margin-top:3px;white-space:pre-wrap;overflow-wrap:anywhere;max-height:3.1em;overflow:hidden}
@media (max-width:600px){.agent{grid-template-columns:1fr}}
.flag{font-size:11px;border-radius:4px;padding:1px 6px;white-space:nowrap}
.flag.warn{background:color-mix(in srgb,var(--warn) 15%,transparent)} .flag.ok{background:color-mix(in srgb,var(--ok) 15%,transparent)}
.flag.bad{background:color-mix(in srgb,var(--bad) 13%,transparent)} .flag.run{background:color-mix(in srgb,var(--run) 13%,transparent)}
.err{background:color-mix(in srgb,var(--bad) 12%,transparent);color:var(--bad);border-radius:8px;padding:8px 12px;grid-column:1/-1;white-space:pre-wrap}
details summary{cursor:pointer;color:var(--mute);margin-top:8px}
.empty{color:var(--mute);font-style:italic}
.charts{display:grid;gap:18px;grid-template-columns:repeat(auto-fit,minmax(min(100%,420px),1fr))}
.chart h3{margin:0 0 2px}.chart > div:last-child{min-height:170px}.chart .sub{color:var(--mute);font-size:11px;margin-bottom:6px}
.legend{display:flex;gap:12px;font-size:11px;color:var(--mute);margin-bottom:4px;flex-wrap:wrap}
.legend i{display:inline-block;width:10px;height:10px;border-radius:2px;margin-right:4px;vertical-align:-1px}
svg text{fill:var(--mute);font-size:10px}
svg .grid{stroke:var(--grid);stroke-width:1}
svg .hit{fill:transparent}
svg .hit:hover{fill:color-mix(in srgb,var(--fg) 6%,transparent)}
.cl-group{margin-top:10px}.cl-group h3{display:flex;gap:8px;align-items:center;margin:6px 0}
.cl-item{display:grid;grid-template-columns:22px 1fr;gap:2px 6px;padding:5px 0;border-top:1px solid var(--line)}
.cl-item input{margin:2px 0 0;width:14px;height:14px}.cl-item.blocked{opacity:.65}
.cl-actions{display:flex;gap:8px;align-items:center;flex-wrap:wrap}
button.primary{background:var(--accent);border-color:var(--accent);color:#fff}
button.danger{background:var(--bad);border-color:var(--bad);color:#fff}
button:disabled{opacity:.5;cursor:default}
pre.out{background:var(--chip);border-radius:6px;padding:8px;white-space:pre-wrap;font-size:12px;max-height:240px;overflow:auto}
#tip{position:fixed;pointer-events:none;background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:6px 8px;font-size:12px;box-shadow:0 4px 14px rgba(0,0,0,.15);z-index:10;display:none;max-width:320px}
#tip i{display:inline-block;width:8px;height:8px;border-radius:2px;margin-right:4px}
</style></head><body>
<header><h1>ArchiCraft · bead loop</h1>
<nav class="tabs"><a href="/roadmap">Roadmap</a><a class="on" href="/">Status</a><a href="/code">Code map</a></nav>
<span class="live" id="live"><span class="dot bg-mute"></span>connecting…</span>
<span class="meta" id="meta"></span>
<button onclick="document.getElementById('cleanup-wrap').scrollIntoView({behavior:'smooth'})">Cleanup</button>
<button onclick="load(true)" title="Collect fresh data now">Refresh now</button></header>
<main id="main"></main>
<main id="cleanup-wrap" style="padding-top:0"></main>
<div id="tip"></div>
<script>
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const dur = s => s < 60 ? Math.round(s) + " s" : s < 3600 ? Math.round(s/60) + " min" : s < 86400 ? (Math.round(s/360)/10) + " h" : (Math.round(s/8640)/10) + " d";
const ago = iso => iso ? (Date.now() - new Date(iso) < 45000 ? "just now" : dur((Date.now() - new Date(iso)) / 1000) + " ago") : "";
const prio = p => `<span class="p${p}">P${p ?? "?"}</span>`;
const INC = ["increment-1","increment-2","increment-3","increment-4"];
const chips = (l, skip=INC, max=4) => { const v = (l||[]).filter(x => !skip.includes(x));
  return v.slice(0, max).map(x => `<span class="chip">${esc(x)}</span>`).join("") +
    (v.length > max ? `<span class="chip mute" title="${esc(v.slice(max).join(", "))}">+${v.length - max}</span>` : ""); };
const sec = (title, n, body, cls="", right="") => `<section class="${cls}"><h2>${title}${n!=null?` <span class="n">${n}</span>`:""}${right?`<span class="right">${right}</span>`:""}</h2>${body}</section>`;
const prRef = p => p ? `<a href="${esc(p.url)}" target="_blank">#${p.number}</a>${p.state && p.state !== "OPEN" ? ` <span class="mute small">${p.state.toLowerCase()}</span>` : ""}` : "";
const det = (key, summary, body) => `<details data-key="${key}"><summary>${summary}</summary>${body}</details>`;
const css = v => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
let S = null, range = (() => { try { return localStorage.getItem("range") || "24h"; } catch { return "24h"; } })();

function beadCells(b) {
  return `<td class="id">${esc(b.id)}</td><td>${prio(b.priority)}</td><td>${esc(b.title)}<div>${chips(b.labels)}</div>`;
}

// --- agents -----------------------------------------------------------------------------
const STATES = {
  working: ["bg-run pulse", "Working", a => `last output ${ago(a.last)}`],
  quiet:   ["bg-warn", "Quiet", a => `inside a turn, no output for ${ago(a.last).replace(" ago","")}; a long build or test, or stuck`],
  stale:   ["bg-bad", "Stale", a => `inside a turn, nothing for ${ago(a.last).replace(" ago","")}; probably interrupted`],
  waiting: ["bg-ok", "Idle", a => `turn finished ${ago(a.turn_end || a.last)}, waits for a message`],
  ended:   ["bg-mute", "Not running", a => `last activity ${ago(a.last)}`],
};
function agentBlock(a) {
  const [dot, label, sub] = STATES[a.state];
  const L = a.loop || {};
  const loop = [
    L.gate ? `<span class="chip ${/passed/.test(L.gate)?"ok":/failed/.test(L.gate)?"bad":""}">gate: ${esc(L.gate.replace(/ on this tree/,""))}</span>` : "",
    L.review ? `<span class="chip ${/^passed/.test(L.review)?"ok":""}">review: ${esc(L.review.replace(/ on this tree/,""))}</span>` : "",
    L["uncommitted code"] === "yes" ? `<span class="chip warn">uncommitted code</span>` : "",
    L.paused === "yes" ? `<span class="chip bad">paused</span>` : ""].join("");
  let what;
  if (a.orchestrator) what = `<b>Orchestrator</b> <span class="mute">runs /bead-loop, starts one bead session at a time</span>`;
  else if (a.bead) what = `<b class="id">${esc(a.bead)}</b> ${esc(a.bead_title || "")}
      ${a.bead_status === "closed" ? '<span class="flag ok">bead closed</span>' : a.bead_status ? `<span class="chip">${esc(a.bead_status.replace("_"," "))}</span>` : ""} ${prRef(a.pr)}`;
  else what = `<span class="mute">no bead</span>`;
  const procs = a.procs.map(p => `${p.kind}, up ${esc(p.elapsed)}`).join("; ");
  return `<div class="agent"><div class="state"><span class="dot ${dot}"></span><div>${label}<small>${sub(a)}</small></div></div>
    <div class="what">${what}
      <div class="small mute">${esc(a.title || "untitled session")} · ${esc(a.worktree)}${a.branch ? ` · ${esc(a.branch)}` : ""}${procs ? ` · ${procs}` : ""}</div>
      ${loop ? `<div>${loop}</div>` : ""}
      ${a.last_msg && a.state !== "ended" ? `<div class="last">${esc(a.last_msg)}</div>` : ""}</div></div>`;
}

// --- pull requests ------------------------------------------------------------------------
function prState(p) {
  return `<span class="dot ${esc(p.status.dot)}"></span>${esc(p.status.text)}`;
}
function prRow(p) {
  const c = p.checks;
  const checks = `<span class="ok">${c.pass} passed</span>${c.fail?` · <span class="bad">${c.fail} failed</span>`:""}${c.pending?` · <span class="run">${c.pending} running</span>`:""}${c.skipped?` · <span class="mute">${c.skipped} skipped</span>`:""}`;
  const detail = [c.failing.length ? `<span class="bad">failing: ${esc(c.failing.join(", "))}</span>` : "",
                  c.running.length ? `<span class="mute">running: ${esc(c.running.join(", "))}</span>` : ""].filter(Boolean).join("<br>");
  return `<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td>
    <td>${esc(p.title)}<div class="small mute">${esc(p.branch)} · ${esc(p.size)} · updated ${ago(p.updated)}</div>
      <div>${chips(p.labels)}${p.queue.state === "queued" || p.queue.state === "left-queue" ? "" : p.auto_merge?'<span class="chip ok">auto-merge on</span>':'<span class="chip warn">auto-merge off</span>'}${p.review?`<span class="chip">${esc(p.review.toLowerCase())}</span>`:''}</div>
      ${detail?`<div class="small">${detail}</div>`:""}</td>
    <td class="nowrap">${prState(p)}<div class="small">${checks}</div></td></tr>`;
}

// --- in progress --------------------------------------------------------------------------
function progressRow(b) {
  const agents = b.agents.map(a => `<span class="nowrap"><span class="dot ${STATES[a.state][0]}"></span>${STATES[a.state][1].toLowerCase()} in ${esc(a.worktree)}</span>`).join("<br>");
  const flag = b.flag === "merged" ? `<span class="flag ok">PR merged, the next sweep closes it</span>`
    : b.flag === "orphan" ? `<span class="flag warn">claimed, but no agent and no open PR</span>` : "";
  return `<tr>${beadCells(b)}${flag ? `<div>${flag}</div>` : ""}</td>
    <td class="small">${agents || '<span class="mute">no agent</span>'}${b.pr ? `<br>PR ${prRef(b.pr)}` : ""}<br><span class="mute">updated ${ago(b.updated_at)}</span></td></tr>`;
}
function splitRow(b) {
  const done = b.children.filter(c => c.status === "closed").length;
  const kids = b.children.map(c => `<div class="small nowrap">${c.status === "closed" ? '<span class="ok">✓</span>' : c.live ? '<span class="dot bg-run pulse"></span>' : c.status === "in_progress" ? '<span class="run">◐</span>' : '<span class="mute">○</span>'} <span class="id">${esc(c.id)}</span> <span class="mute">${esc(c.status.replace("_"," "))}</span></div>`).join("");
  return `<tr>${beadCells(b)}${b.flag === "done" ? '<div><span class="flag ok">all children closed, the next sweep closes it</span></div>' : ""}</td>
    <td style="width:170px"><div class="bar"><i style="width:${b.children.length ? 100*done/b.children.length : 0}%"></i></div><div class="small mute">${done}/${b.children.length} children closed</div>${kids}</td></tr>`;
}

// --- charts -------------------------------------------------------------------------------
const RANGES = {"24h": [86400, 3600, 600], "3d": [3*86400, 3*3600, 1800], "7d": [7*86400, 6*3600, 3600], "all": [null, 86400, 3*3600]};
function span(H) {
  const now = Date.now() / 1000, [len, bin, lane] = RANGES[range];
  let start = len ? now - len : Math.min(...H.beads.map(b => b[0]).filter(Boolean), ...H.prs.map(p => p[0]).filter(Boolean), now - 86400);
  start = Math.floor(start / bin) * bin;
  return {start, end: now, bin, lane};
}
const fmtT = (t, bin) => { const d = new Date(t * 1000);
  return bin >= 86400 ? d.toLocaleDateString(undefined, {month: "short", day: "numeric"})
    : d.toLocaleString(undefined, {weekday: "short", hour: "2-digit", minute: "2-digit"}); };
const fmtRange = (t, bin) => `${fmtT(t, bin)} – ${new Date((t + bin) * 1000).toLocaleTimeString(undefined, {hour: "2-digit", minute: "2-digit"})}`;
const niceMax = v => { if (v <= 4) return Math.max(1, Math.ceil(v)); const p = 10 ** Math.floor(Math.log10(v)); for (const m of [1, 2, 2.5, 5, 10]) if (m * p >= v) return m * p; return v; };
const tipAttr = html => `data-tip="${esc(html)}"`;
const sw = v => `<i style="background:var(${v})"></i>`;
function barPath(x, y, w, h, r) { r = Math.min(r, w / 2, h); if (h <= 0) return "";
  return `M${x},${y+h}V${y+r}Q${x},${y} ${x+r},${y}H${x+w-r}Q${x+w},${y} ${x+w},${y+r}V${y+h}Z`; }
function axes(W, H, m, ymax, fmtY, xs) {
  let g = "";
  for (const f of [0, .5, 1]) { const y = m.t + (H - m.t - m.b) * (1 - f);
    g += `<line class="grid" x1="${m.l}" x2="${W - m.r}" y1="${y}" y2="${y}"/><text x="${m.l - 6}" y="${y + 3}" text-anchor="end">${fmtY(ymax * f)}</text>`; }
  for (const [x, label] of xs) g += `<text x="${x}" y="${H - 4}" text-anchor="middle">${esc(label)}</text>`;
  return g;
}
function xTicks(sp, W, m, n=5) {
  const out = [], step = Math.max(1, Math.round((sp.end - sp.start) / sp.bin / n)) * sp.bin;
  for (let t = sp.start; t < sp.end; t += step) out.push([m.l + (t - sp.start) / (sp.end - sp.start) * (W - m.l - m.r), fmtT(t, sp.bin)]);
  return out;
}

function throughputChart(el, H) {
  const sp = span(H), W = el.clientWidth || 600, Hh = 170, m = {l: 30, r: 8, t: 8, b: 20};
  const n = Math.ceil((sp.end - sp.start) / sp.bin), a = Array(n).fill(0), b = Array(n).fill(0);
  const idx = t => Math.floor((t - sp.start) / sp.bin);
  H.beads.forEach(x => { if (x[5] && x[5] >= sp.start) a[idx(x[5])]++; });
  H.prs.forEach(x => { if (x[1] && x[1] >= sp.start) b[idx(x[1])]++; });
  const ymax = niceMax(Math.max(1, ...a, ...b)), pw = W - m.l - m.r, ph = Hh - m.t - m.b, band = pw / n;
  const bw = Math.max(1.5, Math.min(24, (band - 4) / 2)), y = v => m.t + ph * (1 - v / ymax);
  let marks = "", hits = "";
  for (let i = 0; i < n; i++) {
    const x0 = m.l + i * band + (band - 2 * bw - 2) / 2;
    marks += `<path d="${barPath(x0, y(a[i]), bw, ph - (y(a[i]) - m.t), 4)}" fill="var(--s1)"/><path d="${barPath(x0 + bw + 2, y(b[i]), bw, ph - (y(b[i]) - m.t), 4)}" fill="var(--s2)"/>`;
    hits += `<rect class="hit" x="${m.l + i * band}" y="${m.t}" width="${band}" height="${ph}" ${tipAttr(`<b>${fmtRange(sp.start + i * sp.bin, sp.bin)}</b><br>${sw("--s1")}Beads closed: ${a[i]}<br>${sw("--s2")}PRs merged: ${b[i]}`)}/>`;
  }
  const sum = v => v.reduce((x, y) => x + y, 0);
  el.innerHTML = `<div class="legend"><span>${sw("--s1")}Beads closed · ${sum(a)}</span><span>${sw("--s2")}PRs merged · ${sum(b)}</span></div>
    <svg width="${W}" height="${Hh}">${axes(W, Hh, m, ymax, v => Math.round(v), xTicks(sp, W, m))}${marks}${hits}</svg>`;
}

function remainingChart(el, H) {
  const sp = span(H), W = el.clientWidth || 600, Hh = 170, m = {l: 44, r: 54, t: 10, b: 20};
  const steps = Math.ceil((sp.end - sp.start) / sp.bin) + 1, pts = [];
  for (let i = 0; i < steps; i++) {
    const t = Math.min(sp.start + i * sp.bin, sp.end);
    let mins = 0, cnt = 0;
    for (const [c, d, est] of H.beads) if (c && c <= t && !(d && d <= t)) { mins += est; cnt++; }
    if (cnt || pts.length) pts.push([t, mins / 60, cnt]);  // the line starts with the first bead
  }
  if (pts.length < 2) { el.innerHTML = `<div class="empty">No beads in this range.</div>`; return; }
  const vals = pts.map(p => p[1]), lo0 = Math.min(...vals), hi0 = Math.max(...vals), pad = Math.max(1, (hi0 - lo0) * .15);
  const lo = Math.max(0, Math.floor(lo0 - pad)), hi = Math.ceil(hi0 + pad), pw = W - m.l - m.r, ph = Hh - m.t - m.b;
  const x = t => m.l + (t - sp.start) / (sp.end - sp.start) * pw, y = v => m.t + ph * (1 - (v - lo) / (hi - lo));
  let g = "";
  for (const f of [0, .5, 1]) { const v = lo + (hi - lo) * f; g += `<line class="grid" x1="${m.l}" x2="${W - m.r}" y1="${y(v)}" y2="${y(v)}"/><text x="${m.l - 6}" y="${y(v) + 3}" text-anchor="end">${Math.round(v).toLocaleString()} h</text>`; }
  for (const [tx, l] of xTicks(sp, W, m)) g += `<text x="${tx}" y="${Hh - 4}" text-anchor="middle">${esc(l)}</text>`;
  const d = pts.map((p, i) => `${i ? "L" : "M"}${x(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`).join("");
  const last = pts[pts.length - 1];
  let hits = "";
  const n = pts.length;
  pts.forEach((p, i) => { const x0 = i ? (x(pts[i-1][0]) + x(p[0])) / 2 : m.l, x1 = i < n - 1 ? (x(p[0]) + x(pts[i+1][0])) / 2 : W - m.r;
    hits += `<rect class="hit" x="${x0}" y="${m.t}" width="${Math.max(1, x1 - x0)}" height="${ph}" ${tipAttr(`<b>${fmtT(p[0], sp.bin)}</b><br>${Math.round(p[1]).toLocaleString()} h estimated in ${p[2]} open beads`)}/>`; });
  el.innerHTML = `<svg width="${W}" height="${Hh}">${g}<path d="${d}" fill="none" stroke="var(--s1)" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>
    <circle cx="${x(last[0])}" cy="${y(last[1])}" r="4" fill="var(--s1)" stroke="var(--card)" stroke-width="2"/>
    <text x="${x(last[0]) + 8}" y="${y(last[1]) + 3}" style="fill:var(--fg)">${Math.round(last[1]).toLocaleString()} h</text>${hits}</svg>`;
}

function leadTimeChart(el, H) {
  const sp = span(H), W = el.clientWidth || 600, Hh = 170, m = {l: 44, r: 10, t: 10, b: 20};
  const pts = H.prs.filter(p => p[1] && p[1] >= sp.start).map(p => [p[1], (p[1] - p[0]) / 60, p[2], p[3]]);
  if (!pts.length) { el.innerHTML = `<div class="empty">No PR merged in this range.</div>`; return; }
  const top = Math.max(...pts.map(p => p[1])), unit = top >= 120 ? 60 : 1;
  const ymax = niceMax(top / unit) * unit, pw = W - m.l - m.r, ph = Hh - m.t - m.b;
  const x = t => m.l + (t - sp.start) / (sp.end - sp.start) * pw, y = v => m.t + ph * (1 - v / ymax);
  const fmtY = v => unit === 60 ? (Math.round(v / 6) / 10) + " h" : Math.round(v) + " min";
  const dots = pts.map(p => `<circle cx="${x(p[0])}" cy="${y(p[1])}" r="4.5" fill="var(--s1)" stroke="var(--card)" stroke-width="2"/>
    <circle class="hit" cx="${x(p[0])}" cy="${y(p[1])}" r="11" ${tipAttr(`<b>#${p[2]}</b> ${esc(p[3])}<br>open ${dur(p[1] * 60)} until merged · ${fmtT(p[0], 3600)}`)}/>`).join("");
  el.innerHTML = `<svg width="${W}" height="${Hh}">${axes(W, Hh, m, ymax, fmtY, xTicks(sp, W, m))}${dots}</svg>`;
}

function lanesChart(el, H) {
  const sp = span(H), W = el.clientWidth || 600, lw = Math.min(220, W * .32), rowH = 18, cellW = (W - lw - 8) / Math.ceil((sp.end - sp.start) / sp.lane);
  const lanes = H.lanes.map(l => { const cells = {}; let total = 0, beads = new Set();
    for (const [t, n, bead] of l.bins) if (t >= sp.start) { const k = Math.floor((t - sp.start) / sp.lane); cells[k] = cells[k] || {n: 0, beads: new Set()}; cells[k].n += n; total += n; if (bead) { cells[k].beads.add(bead); beads.add(bead); } }
    return {...l, cells, total, beads: [...beads], lastT: Math.max(...l.bins.map(b => b[0]))}; }).filter(l => l.total).sort((a, b) => b.lastT - a.lastT);
  if (!lanes.length) { el.innerHTML = `<div class="empty">No agent activity in this range.</div>`; return; }
  const max = Math.max(...lanes.flatMap(l => Object.values(l.cells).map(c => c.n)));
  const level = n => `var(--q${Math.min(6, 1 + Math.floor(5 * Math.sqrt(n / max)))})`;
  const Hh = lanes.length * rowH + 22;
  let g = "";
  lanes.forEach((l, i) => {
    const y = i * rowH;
    const name = (l.title || "?").length > 34 ? l.title.slice(0, 33) + "…" : l.title;
    g += `<text x="0" y="${y + 12}" style="fill:var(--fg)">${l.live ? "● " : ""}${esc(name)}</text>`;
    g += `<rect x="${lw}" y="${y + 3}" width="${W - lw - 8}" height="${rowH - 6}" rx="2" fill="var(--chip)"/>`;
    for (const [k, c] of Object.entries(l.cells)) {
      const t0 = sp.start + k * sp.lane;
      g += `<rect x="${lw + k * cellW}" y="${y + 3}" width="${Math.max(1.5, cellW - 1)}" height="${rowH - 6}" rx="1.5" fill="${level(c.n)}" ${tipAttr(`<b>${esc(l.title)}</b><br>${fmtRange(t0, sp.lane)}<br>${c.n} assistant messages${c.beads.size ? `<br>bead ${esc([...c.beads].join(", "))}` : ""}`)}/>`;
    }
  });
  const ticks = xTicks(sp, W, {l: lw, r: 8}).map(([x, l]) => `<text x="${x}" y="${Hh - 4}" text-anchor="middle">${esc(l)}</text>`).join("");
  el.innerHTML = `<div class="legend"><span>● session running now</span><span>fewer ${[1,2,3,4,5,6].map(i => `<i style="background:var(--q${i});margin:0 1px"></i>`).join("")} more messages</span></div>
    <svg width="${W}" height="${Hh}">${g}${ticks}</svg>`;
}

function spark(rows, key) {
  if (!rows || rows.length < 2) return "";
  const v = rows.slice(-288).map(r => r[key] ?? 0), W = 96, Hh = 22, lo = Math.min(...v), hi = Math.max(...v), r = hi - lo || 1;
  const pts = v.map((x, i) => `${(i / (v.length - 1) * (W - 6) + 1).toFixed(1)},${(Hh - 3 - (x - lo) / r * (Hh - 6)).toFixed(1)}`);
  const [lx, ly] = pts[pts.length - 1].split(",");
  return `<svg width="${W}" height="${Hh}" ${tipAttr(`last ${v.length} snapshots: min ${lo}, max ${hi}`)}><polyline points="${pts.join(" ")}" fill="none" stroke="var(--mute)" stroke-width="1.5" stroke-linejoin="round"/><circle cx="${lx}" cy="${ly}" r="2.5" fill="var(--s1)"/></svg>`;
}

function drawCharts() {
  if (!S || !S.history) return;
  for (const [id, fn] of [["#c-through", throughputChart], ["#c-remain", remainingChart], ["#c-lead", leadTimeChart], ["#c-lanes", lanesChart]]) {
    const el = $(id); if (el) fn(el, S.history);
  }
}

// --- page ---------------------------------------------------------------------------------
function render(s) {
  const scroll = scrollY;  // charts are drawn after a layout pass; keep the reader's place
  const open = new Set([...document.querySelectorAll("details[open]")].map(d => d.dataset.key));
  const out = [];
  for (const [k, v] of Object.entries(s.errors || {})) out.push(k === "merge queue"
    ? `<div class="err"><b>${esc(k)}</b> could not be read, open PRs show their queue state as unknown: ${esc(v)}</div>`
    : `<div class="err"><b>${esc(k)}</b> could not be read, showing its last good data: ${esc(v)}</div>`);
  const c = s.counts || {}, snaps = s.history?.snapshots, agents = s.agents || [];
  const working = agents.filter(a => a.state === "working").length, live = agents.filter(a => a.state !== "ended").length;
  const k = (n, l, key, cls="") => `<div class="kpi"><b class="${cls}">${n ?? 0}</b><span>${l}</span>${spark(snaps, key)}</div>`;
  out.push(sec("Overview", null, `<div class="kpis">${k(working, `agents working · ${live} running`, "agents_working", working ? "run" : "")}
    ${k(c.working_on, "beads in progress", "working_on")}${k((s.prs_open||[]).length, "open PRs", "prs_open")}
    ${k(c.ready, "ready", "ready", "ok")}${k(c.blocked_by_deps, "waiting on dependencies", "blocked_by_deps")}
    ${k(c.open, "open", "open")}${k(c.closed, "closed", "closed")}</div>
    <div class="small mute" style="margin-top:8px">Open by priority: ${[0,1,2,3,4].map(p=>`${prio(p)} ${(s.by_priority||{})[p] ?? 0}`).join(" · ")}${snaps && snaps.length > 1 ? ` · sparklines: count snapshots since ${new Date(snaps[0].t*1000).toLocaleString()}` : ""}</div>`, "wide"));

  const needs = [];
  (s.human || []).forEach(b => needs.push(`<tr>${beadCells(b)}</td><td class="small mute">labelled human</td></tr>`));
  (s.prs_open || []).filter(p => p.needs_you).forEach(p => needs.push(`<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td></td><td>${esc(p.title)}</td><td class="small mute">${esc(p.needs_you)}</td></tr>`));
  (s.prs_open || []).filter(p => p.checks.fail).forEach(p => needs.push(`<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td></td><td>${esc(p.title)}</td><td class="small bad">checks failing</td></tr>`));
  agents.filter(a => a.loop && a.loop.paused === "yes").forEach(a => needs.push(`<tr><td class="id">${esc(a.bead)}</td><td></td><td>Bead session paused in ${esc(a.worktree)}</td><td class="small mute">paused by the loop</td></tr>`));
  agents.filter(a => a.state === "stale").forEach(a => needs.push(`<tr><td class="id">${esc(a.bead || "")}</td><td></td><td>${esc(a.title || a.worktree)}</td><td class="small mute">session stale for ${ago(a.last).replace(" ago","")}</td></tr>`));
  if (needs.length) out.push(sec(`<span class="bad">Needs you</span>`, needs.length, `<table>${needs.join("")}</table>`, "wide"));

  const running = agents.filter(a => a.state !== "ended"), ended = agents.filter(a => a.state === "ended");
  out.push(sec("Agent sessions", running.length,
    (running.length ? running.map(agentBlock).join("") : `<div class="empty">No Claude session is running in this repository.</div>`) +
    (ended.length ? det("ended", `${ended.length} sessions not running, active in the last 24 h`, ended.map(agentBlock).join("")) : ""), "wide",
    `<span class="small mute">Working = inside a turn · Idle = turn finished, process still open · Quiet / Stale = inside a turn with no output for 10 min / 2 h · Not running = no process</span>`));

  out.push(sec("In progress", (s.in_progress||[]).length,
    ((s.in_progress||[]).length ? `<table>${s.in_progress.map(progressRow).join("")}</table>` : `<div class="empty">No bead is being worked on.</div>`) +
    ((s.split||[]).length ? `<h3>Split into child beads <span class="mute small">(status in progress until all children close)</span></h3><table>${s.split.map(splitRow).join("")}</table>` : "")));

  const prs = s.prs_open || [];
  out.push(sec("Pull requests", prs.length,
    (prs.length ? `<table>${prs.map(prRow).join("")}</table>` : `<div class="empty">No open pull requests.</div>`) +
    ((s.prs_merged||[]).length ? det("merged", "Recently merged", `<table>${s.prs_merged.map(p =>
      `<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td>${esc(p.title)}</td><td class="mute small nowrap">${ago(p.merged)}</td></tr>`).join("")}</table>`) : "")));

  const seg = Object.keys(RANGES).map(r => `<button class="${r === range ? "on" : ""}" onclick="setRange('${r}')">${r}</button>`).join("");
  out.push(sec("History", null, `<div class="charts">
      <div class="chart"><h3>Throughput</h3><div class="sub">Beads closed and PRs merged per ${dur(RANGES[range][1])}</div><div id="c-through"></div></div>
      <div class="chart"><h3>Remaining estimate</h3><div class="sub">Estimated hours of all open beads (epics left out)</div><div id="c-remain"></div></div>
      <div class="chart"><h3>PR lead time</h3><div class="sub">Time from opening a PR to its merge, by merge time</div><div id="c-lead"></div></div>
      <div class="chart"><h3>Agent activity</h3><div class="sub">Assistant messages per ${dur(RANGES[range][2])} and session</div><div id="c-lanes"></div></div>
    </div>`, "wide", `<span class="seg">${seg}</span>`));

  const ready = s.ready || [];
  out.push(sec("Ready next", ready.length, `<table>${ready.slice(0, 12).map(b => `<tr>${beadCells(b)}</td><td class="small mute nowrap">${b.estimated_minutes ? dur(b.estimated_minutes * 60) : ""}</td></tr>`).join("")}</table>` +
    (ready.length > 12 ? det("ready", `${ready.length - 12} more`, `<table>${ready.slice(12).map(b => `<tr>${beadCells(b)}</td><td></td></tr>`).join("")}</table>`) : "")));
  const epics = s.epics || [], active = epics.filter(e => e.closed > 0 || e.status === "in_progress"), rest = epics.filter(e => !active.includes(e));
  const epicRow = e => `<tr><td class="id">${esc(e.id)}</td><td>${prio(e.priority)}</td><td>${esc(e.title)}${e.eligible?' <span class="flag ok">ready to close</span>':''}</td>
    <td style="width:130px"><div class="bar"><i style="width:${e.total?100*e.closed/e.total:0}%"></i></div><div class="small mute">${e.closed}/${e.total}</div></td></tr>`;
  out.push(sec("Epics", epics.length, (active.length ? `<table>${active.map(epicRow).join("")}</table>` : `<div class="empty">No epic has a closed child yet.</div>`) +
    (rest.length ? det("epics", `${rest.length} epics not started`, `<table>${rest.map(epicRow).join("")}</table>`) : "")));
  out.push(sec("Recently closed", null, `<table>${(s.recently_closed||[]).map(b => `<tr>${beadCells(b)}</td><td class="small mute nowrap">${prRef(b.pr)}<br>${ago(b.closed_at)}</td></tr>`).join("")}</table>`));

  $("#main").innerHTML = out.join("");
  document.querySelectorAll("details[data-key]").forEach(d => { if (open.has(d.dataset.key)) d.open = true; });
  drawCharts();
  renderCleanup();
  scrollTo(0, scroll);
}

function setRange(r) { range = r; try { localStorage.setItem("range", r); } catch {} render(S); }

let lastGen = null, lastOk = 0;
async function load(force) {
  try {
    const r = await fetch("/api/state" + (force ? "?refresh=1" : ""));
    const s = await r.json();
    lastOk = Date.now();
    if (s.generated && s.generated !== lastGen) { lastGen = s.generated; S = s; render(s); }
  } catch (e) { /* the live indicator shows it */ }
  tick();
}
function tick() {
  const el = $("#live");
  if (!S || !S.generated) { el.innerHTML = `<span class="dot bg-mute pulse"></span>collecting data…`; return; }
  const age = (Date.now() - new Date(S.generated)) / 1000, offline = Date.now() - lastOk > 15000;
  const stale = age > 2 * S.interval + 30;
  el.innerHTML = offline ? `<span class="dot bg-bad"></span><span class="bad">dashboard server not reachable</span> · data from ${ago(S.generated)}`
    : stale ? `<span class="dot bg-warn"></span>data ${dur(age)} old, collection is slow`
    : `<span class="dot bg-ok pulse"></span>live · data ${dur(age)} old · updates every ${S.interval} s`;
  $("#meta").textContent = `collection took ${S.took} s · ${S.root}`;
}
const tip = $("#tip");
document.addEventListener("mousemove", e => {
  const t = e.target.closest && e.target.closest("[data-tip]");
  if (!t) { tip.style.display = "none"; return; }
  tip.innerHTML = t.dataset.tip; tip.style.display = "block";
  const r = tip.getBoundingClientRect();
  tip.style.left = Math.min(e.clientX + 14, innerWidth - r.width - 8) + "px";
  tip.style.top = (e.clientY + 14 + r.height > innerHeight ? e.clientY - r.height - 10 : e.clientY + 14) + "px";
});
// --- cleanup ------------------------------------------------------------------------------
const TOKEN = "__TOKEN__";
const CL = {data: null, sel: new Set(), busy: null, lastRun: null, sweep: null, error: null};
const GROUPS = [["session", "Stale sessions", "Stopping a session ends its process; the transcript stays and the app can resume it."],
  ["worktree", "Finished worktrees", "Removes the worktree directory (build output included) and its local branch."],
  ["branch", "Leftover local branches", "Branches with no worktree whose commits are on main or kept in a closed PR."],
  ["remote", "Remote branches and refs", "Deletes branches on GitHub whose PR merged; prunes refs to branches GitHub already deleted."]];
async function post(path, body) {
  const r = await fetch(path, {method: "POST", headers: {"Content-Type": "application/json", "X-Dashboard-Token": TOKEN}, body: JSON.stringify(body || {})});
  if (!r.ok) throw new Error(`${r.status} ${await r.text()}`);
  const j = await r.json(); if (j.error) throw new Error(j.error); return j;
}
async function clBusy(msg, fn) {
  CL.busy = msg; CL.error = null; renderCleanup();
  try { await fn(); } catch (e) { CL.error = String(e.message || e); }
  CL.busy = null; renderCleanup();
}
const clFind = () => clBusy("Looking for cleanup candidates…", async () => {
  CL.data = await post("/api/cleanup/preview");
  const ids = new Set(CL.data.candidates.map(c => c.id)); CL.sel = new Set([...CL.sel].filter(i => ids.has(i)));
});
function clToggle(id, on) { on ? CL.sel.add(id) : CL.sel.delete(id); renderCleanup(); }
function clGroup(kind, on) { CL.data.candidates.filter(c => c.kind === kind && !c.blocked).forEach(c => on ? CL.sel.add(c.id) : CL.sel.delete(c.id)); renderCleanup(); }
function clRun() {
  const chosen = CL.data.candidates.filter(c => CL.sel.has(c.id));
  if (!chosen.length) return;
  const lines = chosen.slice(0, 25).map(c => `• ${c.label}: ${c.action}`).join("\n") + (chosen.length > 25 ? `\n… and ${chosen.length - 25} more` : "");
  if (!confirm(`Clean up ${chosen.length} item${chosen.length > 1 ? "s" : ""}? This cannot be undone.\n\n${lines}`)) return;
  clBusy(`Cleaning up ${chosen.length} items…`, async () => {
    const res = await post("/api/cleanup/run", {ids: chosen.map(c => c.id)});
    CL.lastRun = chosen.map(c => ({label: c.label, ...(res[c.id] || {ok: false, message: "no result"})}));
    CL.sel.clear(); CL.data = await post("/api/cleanup/preview"); load(true);
  });
}
function clSweep() {
  if (!confirm("Run the sweep now? It closes in-progress beads whose PR merged and split parents whose children are all closed (scripts/loop/sweep.sh as on origin/main).")) return;
  clBusy("Running the sweep…", async () => { CL.sweep = await post("/api/sweep"); load(true); });
}
function renderCleanup() {
  const el = $("#cleanup-wrap"); if (!el) return;
  const willClose = [...(S?.in_progress || []).filter(b => b.flag === "merged"), ...(S?.split || []).filter(b => b.flag === "done")];
  let body = `<div class="cl-actions"><button onclick="clFind()" ${CL.busy ? "disabled" : ""}>${CL.data ? "Look again" : "Find cleanup candidates"}</button>
    <button onclick="clSweep()" ${CL.busy ? "disabled" : ""}>Run sweep</button>
    <span class="small mute">${willClose.length ? `The sweep would close ${willClose.map(b => esc(b.id)).join(", ")}.` : "Nothing for the sweep to close right now."}</span>
    ${CL.busy ? `<span class="small"><span class="dot bg-run pulse"></span>${CL.busy}</span>` : ""}</div>`;
  if (CL.error) body += `<div class="err" style="margin-top:8px">${esc(CL.error)}</div>`;
  if (CL.sweep) body += `<h3>Sweep ${CL.sweep.ok ? '<span class="ok">done</span>' : '<span class="bad">failed</span>'}</h3><pre class="out">${esc(CL.sweep.output)}</pre>`;
  if (CL.lastRun) body += `<h3>Last cleanup</h3>` + CL.lastRun.map(r => `<div class="small">${r.ok ? '<span class="ok">✓</span>' : '<span class="bad">✗</span>'} ${esc(r.label)} <span class="mute">${esc(r.message)}</span></div>`).join("");
  if (CL.data) {
    const cands = CL.data.candidates;
    if (!cands.length) body += `<div class="empty" style="margin-top:8px">Nothing to clean up.</div>`;
    for (const [kind, title, note] of GROUPS) {
      const items = cands.filter(c => c.kind === kind); if (!items.length) continue;
      const free = items.filter(c => !c.blocked), all = free.length && free.every(c => CL.sel.has(c.id));
      body += `<div class="cl-group"><h3><input type="checkbox" ${all ? "checked" : ""} ${free.length ? "" : "disabled"} onchange="clGroup('${kind}', this.checked)">${title} <span class="n small mute">${items.length}</span></h3><div class="small mute">${note}</div>` +
        items.map(c => `<label class="cl-item ${c.blocked ? "blocked" : ""}"><input type="checkbox" ${CL.sel.has(c.id) ? "checked" : ""} ${c.blocked || CL.busy ? "disabled" : ""} onchange="clToggle(${esc(JSON.stringify(c.id))}, this.checked)">
          <div><b>${esc(c.label)}</b> <span class="small mute">${esc(c.detail || "")}</span><div class="small">${esc(c.why)}${c.blocked ? ` · <span class="bad">kept: ${esc(c.blocked)}</span>` : ` · <span class="mute">${esc(c.action)}</span>`}</div></div></label>`).join("") + `</div>`;
    }
    if (CL.data.kept_branches?.length) body += det("kept", `${CL.data.kept_branches.length} branches kept: they have commits that are neither on main nor in a merged or closed PR`, `<div class="small">${CL.data.kept_branches.map(esc).join("<br>")}</div>`);
    const n = CL.sel.size;
    body += `<div class="cl-actions" style="margin-top:12px"><button class="danger" onclick="clRun()" ${n && !CL.busy ? "" : "disabled"}>Clean up ${n} selected</button>
      <span class="small mute">Each item is checked again right before it runs. Actions are logged to loop-dashboard/cleanup.log in the git directory.</span></div>`;
  }
  el.innerHTML = sec("Cleanup", null, body, "wide");
}

let rt; addEventListener("resize", () => { clearTimeout(rt); rt = setTimeout(drawCharts, 150); });
load(); setInterval(load, 5000); setInterval(tick, 1000);
</script></body></html>
"""


BASE_CSS = PAGE[PAGE.index("<style>") + 7:PAGE.index("</style>")]

PAGE_CODE = r"""<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ArchiCraft Code Map</title>
<style>/*BASE*/
:root{--c1:#2a78d6;--c2:#eb6834;--c3:#1baf7a;--c4:#eda100}
@media (prefers-color-scheme:dark){:root:not([data-theme="light"]){--c1:#3987e5;--c2:#d95926;--c3:#199e70;--c4:#c98500}}
.wrap{display:grid;grid-template-columns:minmax(0,1fr) 330px;gap:14px;padding:14px 16px;align-items:start}
@media (max-width:980px){.wrap{grid-template-columns:1fr}}
.toolbar{display:flex;gap:10px;align-items:center;flex-wrap:wrap;margin-bottom:10px}
.toolbar label{font-size:12px;color:var(--mute);display:flex;gap:6px;align-items:center}
.toolbar input[type=search]{font:inherit;padding:3px 8px;border:1px solid var(--line);border-radius:6px;background:var(--card);color:var(--fg);width:180px}
#stage{position:relative;overflow:hidden;border-radius:8px}
#stage svg{display:block;user-select:none}
#stage.graph svg{cursor:grab}#stage.graph svg.panning{cursor:grabbing}
.tile{cursor:pointer}.tile rect{transition:opacity .15s}
.dim{opacity:.28}
.lbl{pointer-events:none;font-size:10px}
.glabel{font-size:10px;font-weight:600;fill:var(--mute);text-transform:uppercase;letter-spacing:.04em;pointer-events:none}
.chead{font-size:12px;font-weight:600;fill:var(--fg)}
.csub{font-size:10px;fill:var(--mute)}
.edge{fill:none;pointer-events:none}
.legendbar{display:flex;gap:14px;flex-wrap:wrap;font-size:11px;color:var(--mute);margin-top:8px;align-items:center}
.legendbar i{display:inline-block;width:12px;height:10px;border-radius:2px;margin-right:1px;vertical-align:-1px}
.legendbar .ring{display:inline-block;width:10px;height:10px;border-radius:50%;border:2px solid var(--run);vertical-align:-2px;margin-right:4px}
.panel h2{margin-bottom:6px}
.panel .path{font-family:ui-monospace,Menlo,monospace;font-size:11px;color:var(--mute);overflow-wrap:anywhere}
.panel ul{list-style:none;margin:4px 0 0;padding:0}
.panel li{padding:3px 0;border-top:1px solid var(--line);font-size:12px;display:flex;gap:6px;align-items:baseline}
.panel li:first-child{border-top:0}
.panel li .grow{flex:1;min-width:0;overflow-wrap:anywhere}
.panel .link{cursor:pointer;color:var(--accent)}
.stat{display:flex;gap:14px;flex-wrap:wrap;margin:6px 0}
.stat div{font-size:11px;color:var(--mute)}.stat b{display:block;font-size:16px;color:var(--fg);font-variant-numeric:tabular-nums}
</style></head><body>
<header><h1>ArchiCraft · bead loop</h1>
<nav class="tabs"><a href="/roadmap">Roadmap</a><a href="/">Status</a><a class="on" href="/code">Code map</a></nav>
<span class="live" id="live"><span class="dot bg-mute pulse"></span>loading the code base…</span>
<span class="meta" id="meta"></span></header>
<div class="wrap">
  <section>
    <div class="toolbar">
      <span class="seg" id="v-view"></span>
      <label>Colour <span class="seg" id="v-metric"></span></label>
      <label>Range <span class="seg" id="v-range"></span></label>
      <label><input type="checkbox" id="v-tests"> tests, benches</label>
      <input type="search" id="v-search" placeholder="Find a module…">
    </div>
    <div id="stage"></div>
    <div class="legendbar" id="legend"></div>
  </section>
  <section class="panel" id="panel"></section>
</div>
<div id="tip"></div>
<script>
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const dur = s => s < 60 ? Math.round(s) + " s" : s < 3600 ? Math.round(s/60) + " min" : s < 86400 ? (Math.round(s/360)/10) + " h" : (Math.round(s/8640)/10) + " d";
const ago = t => t ? dur(Date.now()/1000 - t) + " ago" : "never";
const css = v => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
const tipAttr = html => `data-tip="${esc(html)}"`;
const store = (k, v) => { try { if (v === undefined) return localStorage.getItem("code." + k); localStorage.setItem("code." + k, v); } catch {} };

let D = null, view = store("view") || "map", metric = store("metric") || "age", range = store("range") || "7d";
let showTests = store("tests") === "1", query = "", selected = null, hover = null;
const RANGES = {"24h": 86400, "3d": 3*86400, "7d": 7*86400, "all": null};
const METRICS = {age: "Last change", commits: "Commits", churn: "Lines changed"};
const CRATE_COLOR = ["--c1", "--c2", "--c3", "--c4"];
const short = c => c.replace(/^archicraft-/, "");

function seg(el, options, value, set) {
  el.innerHTML = Object.entries(options).map(([k, l]) => `<button class="${k === value ? "on" : ""}" data-k="${k}">${l}</button>`).join("");
  el.onclick = e => { const k = e.target.dataset.k; if (k) set(k); };
}
function controls() {
  seg($("#v-view"), {map: "Components", graph: "Dependency graph"}, view, k => { view = k; store("view", k); render(); });
  seg($("#v-metric"), METRICS, metric, k => { metric = k; store("metric", k); render(); });
  seg($("#v-range"), {"24h": "24h", "3d": "3d", "7d": "7d", all: "all"}, range, k => { range = k; store("range", k); render(); });
  $("#v-tests").checked = showTests;
}
$("#v-tests").onchange = e => { showTests = e.target.checked; store("tests", showTests ? "1" : "0"); graphReset = true; render(); };
$("#v-search").oninput = e => { query = e.target.value.trim().toLowerCase(); render(); };

// --- derived data -------------------------------------------------------------------------
let N = [], byId = {}, E = [], crateIdx = {};
function derive() {
  const now = Date.now() / 1000, start = RANGES[range] ? now - RANGES[range] : 0;
  crateIdx = Object.fromEntries([...D.crates].sort((a, b) => a.layer - b.layer || a.name.localeCompare(b.name)).map((c, i) => [c.name, i]));
  N = D.nodes.filter(n => showTests || n.kind === "src").map(n => {
    let commits = 0, churn = 0, last = 0;
    for (const [ci, a, d] of n.history) { const t = D.commits[ci].t; last = Math.max(last, t); if (t >= start) { commits++; churn += a + d; } }
    return {...n, commits, churn, last, live: D.live[n.id] || []};
  });
  byId = Object.fromEntries(N.map(n => [n.id, n]));
  E = D.edges.filter(([a, b]) => byId[a] && byId[b]).map(([a, b, w]) => ({a, b, w}));
  N.forEach(n => { n.out = []; n.in = []; });
  E.forEach(e => { byId[e.a].out.push(e); byId[e.b].in.push(e); });
}
const AGE_STEPS = [[3600, "< 1 h"], [6*3600, "< 6 h"], [86400, "< 1 d"], [3*86400, "< 3 d"], [7*86400, "< 7 d"], [30*86400, "< 30 d"]];
function level(n) {
  if (metric === "age") {
    if (!n.last) return 0;
    const a = Date.now() / 1000 - n.last, i = AGE_STEPS.findIndex(s => a < s[0]);
    return i < 0 ? 0 : 6 - i;
  }
  const v = metric === "commits" ? n.commits : n.churn;
  if (!v) return 0;
  const max = Math.max(1, ...N.map(m => metric === "commits" ? m.commits : m.churn));
  return Math.max(1, Math.ceil(6 * Math.sqrt(v / max)));
}
const fill = n => { const l = level(n); return l ? `var(--q${l})` : "var(--chip)"; };
const inkCache = {};
function ink(n) {
  const l = level(n), key = l + (matchMedia("(prefers-color-scheme: dark)").matches ? "d" : "l");
  if (!(key in inkCache)) {
    const hex = l ? css(`--q${l}`) : css("--chip"), m = /^#?(..)(..)(..)$/.exec(hex);
    const lum = m ? [m[1], m[2], m[3]].map(h => { const c = parseInt(h, 16) / 255; return c <= .03928 ? c / 12.92 : ((c + .055) / 1.055) ** 2.4; }).reduce((s, c, i) => s + c * [.2126, .7152, .0722][i], 0) : 1;
    inkCache[key] = lum < .35 ? "#fff" : "#1c2128";
  }
  return inkCache[key];
}
const matches = n => !query || n.module.toLowerCase().includes(query) || n.id.toLowerCase().includes(query);
function related(id) {
  if (!id || !byId[id]) return null;
  const s = new Set([id]); byId[id].out.forEach(e => s.add(e.b)); byId[id].in.forEach(e => s.add(e.a)); return s;
}
function nodeTip(n) {
  const live = n.live.map(l => `<br><span style="color:var(--run)">●</span> ${esc(l.label)}${l.uncommitted ? " (uncommitted)" : ""}`).join("");
  return `<b>${esc(short(n.crate))} · ${esc(n.module)}</b><br>${n.lines} lines · last change ${ago(n.last)}<br>${n.commits} commits, ${n.churn} lines changed in ${range}<br>uses ${n.out.length} · used by ${n.in.length}${live}`;
}

// --- squarified treemap -------------------------------------------------------------------
function squarify(items, x, y, w, h) {
  const total = items.reduce((s, i) => s + i.value, 0), out = [];
  if (!total || w <= 0 || h <= 0) return out;
  const scale = w * h / total, rest = items.map(i => ({...i, area: i.value * scale})).sort((a, b) => b.area - a.area);
  let row = [];
  const worst = (r, side) => { const s = r.reduce((a, i) => a + i.area, 0), mx = Math.max(...r.map(i => i.area)), mn = Math.min(...r.map(i => i.area));
    return Math.max(side * side * mx / (s * s), s * s / (side * side * mn)); };
  const layout = r => { const s = r.reduce((a, i) => a + i.area, 0);
    if (w >= h) { const cw = s / h; let cy = y; r.forEach(i => { const ch = i.area / cw; out.push({...i, x, y: cy, w: cw, h: ch}); cy += ch; }); x += cw; w -= cw; }
    else { const rh = s / w; let cx = x; r.forEach(i => { const cw = i.area / rh; out.push({...i, x: cx, y, w: cw, h: rh}); cx += cw; }); y += rh; h -= rh; } };
  while (rest.length) {
    const side = Math.min(w, h), next = rest[0];
    if (!row.length || worst([...row, next], side) <= worst(row, side)) { row.push(rest.shift()); }
    else { layout(row); row = []; }
  }
  if (row.length) layout(row);
  return out;
}

// --- components view ----------------------------------------------------------------------
let tiles = {};
function renderMap(stage) {
  const W = Math.max(560, stage.clientWidth), H = Math.max(520, Math.min(760, innerHeight - 220)), top = 92, gap = 10;
  const crates = [...D.crates].sort((a, b) => a.layer - b.layer);
  const lines = c => N.filter(n => n.crate === c.name).reduce((s, n) => s + n.lines, 0) || 1;
  const total = crates.reduce((s, c) => s + lines(c), 0);
  let weights = crates.map(c => Math.max(.14, lines(c) / total)); const ws = weights.reduce((a, b) => a + b, 0); weights = weights.map(w => w / ws);
  const avail = W - gap * (crates.length - 1);
  let x = 0, g = "", cols = {};
  tiles = {};
  crates.forEach((c, ci) => {
    const cw = avail * weights[ci], nodes = N.filter(n => n.crate === c.name);
    cols[c.name] = {x, w: cw};
    const commits = nodes.reduce((s, n) => s + n.commits, 0), live = nodes.filter(n => n.live.length).length;
    g += `<rect x="${x}" y="${top}" width="${cw}" height="${H - top}" rx="8" fill="var(--chip)" opacity=".55"/>`;
    g += `<rect x="${x + 10}" y="${top + 12}" width="10" height="10" rx="2" fill="var(${CRATE_COLOR[crateIdx[c.name] % 4]})"/>`;
    const fit = (t, px) => { const n = Math.floor((cw - 16) / px); return t.length > n ? t.slice(0, Math.max(1, n - 1)) + "…" : t; };
    g += `<text class="chead" x="${x + 26}" y="${top + 21}">${esc(fit(short(c.name), 7.5))}</text>`;
    g += `<text class="csub" x="${x + 10}" y="${top + 35}">${esc(fit(`${nodes.length} modules · ${lines(c).toLocaleString()} lines`, 5.6))}</text>`;
    g += `<text class="csub" x="${x + 10}" y="${top + 48}">${esc(fit(`${commits} commits in ${range}${live ? ` · ${live} changing now` : ""}`, 5.6))}</text>`;
    const groups = {}; nodes.forEach(n => (groups[n.group] = groups[n.group] || []).push(n));
    const grects = squarify(Object.entries(groups).map(([k, ns]) => ({key: k, nodes: ns, value: ns.reduce((s, n) => s + Math.max(20, n.lines), 0)})), x + 6, top + 56, cw - 12, H - top - 62);
    for (const gr of grects) {
      const lab = gr.w > 50 && gr.h > 36, gy = gr.y + (lab ? 14 : 2);
      if (lab) g += `<text class="glabel" x="${gr.x + 4}" y="${gr.y + 10}">${esc(gr.key === "src" ? "crate root files" : gr.key)}</text>`;
      for (const t of squarify(gr.nodes.map(n => ({n, value: Math.max(20, n.lines)})), gr.x + 2, gy, gr.w - 4, gr.h - (gy - gr.y) - 2)) {
        const n = t.n, tx = t.x + 1, ty = t.y + 1, tw = Math.max(1, t.w - 2), th = Math.max(1, t.h - 2);
        tiles[n.id] = {x: tx + tw / 2, y: ty + th / 2, w: tw, h: th};
        const name = n.module.split("::").pop(), live = n.live.length;
        const fits = tw > 34 && th > 15, chars = Math.floor((tw - 8) / 6);
        g += `<g class="tile" data-id="${esc(n.id)}" ${tipAttr(nodeTip(n))}><rect x="${tx}" y="${ty}" width="${tw}" height="${th}" rx="3" fill="${fill(n)}"
          ${live ? `stroke="var(--run)" stroke-width="2.5"` : n.id === selected ? `stroke="var(--fg)" stroke-width="2"` : ""}/>
          ${fits ? `<text class="lbl" x="${tx + 5}" y="${ty + 12}" fill="${ink(n)}">${esc(name.length > chars ? name.slice(0, Math.max(1, chars - 1)) + "…" : name)}</text>` : ""}
          ${fits && th > 28 && tw > 50 ? `<text class="lbl" x="${tx + 5}" y="${ty + 24}" fill="${ink(n)}" opacity=".75">${n.lines} lines</text>` : ""}
          ${live && tw > 10 && th > 10 ? `<circle cx="${tx + tw - 6}" cy="${ty + 6}" r="3" fill="var(--run)" class="${n.live.some(l => l.state === "working") ? "pulse" : ""}"/>` : ""}</g>`;
      }
    }
    x += cw + gap;
  });
  // Crate dependencies as arcs above the columns: from the user to the crate it builds on.
  let arcs = "";
  for (const c of crates) for (const d of c.deps.filter(d => !d.dev && cols[d.crate])) {
    const a = cols[c.name], b = cols[d.crate], x1 = a.x + a.w / 2 - 6, x2 = b.x + b.w / 2 + 6, lift = 8 + Math.abs(crateIdx[c.name] - crateIdx[d.crate]) * 15;
    const n = E.filter(e => byId[e.a].crate === c.name && byId[e.b].crate === d.crate).length;
    arcs += `<path d="M${x1},${top - 2} C${x1},${top - 2 - lift * 1.4} ${x2},${top - 2 - lift * 1.4} ${x2},${top - 6}" fill="none" stroke="var(--mute)" stroke-width="1.2" opacity=".55" marker-end="url(#arr)"/>
      <path d="M${x1},${top - 2} C${x1},${top - 2 - lift * 1.4} ${x2},${top - 2 - lift * 1.4} ${x2},${top - 6}" fill="none" stroke="transparent" stroke-width="10" ${tipAttr(`<b>${esc(short(c.name))} → ${esc(short(d.crate))}</b><br>${n} module dependencies`)}/>`;
  }
  stage.className = "map";
  stage.innerHTML = `<svg width="${W}" height="${H}"><defs><marker id="arr" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="6" markerHeight="6" orient="auto"><path d="M0,0L8,4L0,8z" fill="var(--mute)"/></marker></defs>
    <text class="csub" x="0" y="11">Crates from the base layer (left) to the app (right). An arrow points to the crate a crate depends on.</text>${arcs}${g}<g id="links"></g></svg>`;
  stage.querySelectorAll(".tile").forEach(el => {
    el.onmouseenter = () => { hover = el.dataset.id; mapFocus(); };
    el.onmouseleave = () => { hover = null; mapFocus(); };
    el.onclick = () => select(el.dataset.id);
  });
  mapFocus();
}
function mapFocus() {
  const id = hover || selected, rel = related(id), svg = $("#stage svg");
  if (!svg) return;
  svg.querySelectorAll(".tile").forEach(el => el.classList.toggle("dim", !!(rel ? !rel.has(el.dataset.id) : query && !matches(byId[el.dataset.id]))));
  let links = "";
  if (rel) {
    const n = byId[id], c = tiles[id];
    const curve = (p, q, col, w) => { const mx = (p.x + q.x) / 2, my = Math.min(p.y, q.y) - 30 - Math.abs(p.x - q.x) * .12;
      return `<path class="edge" d="M${p.x},${p.y} Q${mx},${my} ${q.x},${q.y}" stroke="var(${col})" stroke-width="${Math.min(3.5, 1 + w * .4)}" opacity=".8"/>`; };
    n.out.forEach(e => tiles[e.b] && (links += curve(c, tiles[e.b], "--s1", e.w)));
    n.in.forEach(e => tiles[e.a] && (links += curve(tiles[e.a], c, "--s2", e.w)));
    links += `<circle cx="${c.x}" cy="${c.y}" r="4" fill="var(--fg)" stroke="var(--card)" stroke-width="2"/>`;
  }
  $("#links").innerHTML = links;
}

// --- dependency graph ---------------------------------------------------------------------
let sim = null, graphReset = true, zoom = {k: 1, x: 0, y: 0}, rafId = null, userView = false;
function simInit(W, H) {
  const old = sim && !graphReset ? Object.fromEntries(sim.nodes.map(n => [n.id, n])) : {};
  const anchors = {}, cr = [...D.crates].sort((a, b) => a.layer - b.layer);
  cr.forEach((c, i) => { const a = (i / cr.length) * Math.PI * 2 - Math.PI / 2; anchors[c.name] = {x: Math.cos(a) * 190, y: Math.sin(a) * 150}; });
  const nodes = N.map(n => { const o = old[n.id], a = anchors[n.crate];
    return {id: n.id, crate: n.crate, group: n.crate + "/" + n.group, r: Math.min(20, 4 + Math.sqrt(n.lines) * .42),
      x: o ? o.x : a.x + (Math.random() - .5) * 120, y: o ? o.y : a.y + (Math.random() - .5) * 120, vx: 0, vy: 0, fixed: o && o.fixed}; });
  const idx = Object.fromEntries(nodes.map((n, i) => [n.id, i]));
  sim = {nodes, idx, anchors, links: E.map(e => ({s: idx[e.a], t: idx[e.b], w: e.w})), alpha: Object.keys(old).length ? .3 : 1};
  if (graphReset) { zoom = {k: Math.min(1.2, Math.min(W, H) / 640), x: W / 2, y: H / 2}; userView = false; }
  graphReset = false;
}
function simTick() {
  const {nodes, links, anchors} = sim, a = sim.alpha, n = nodes.length;
  for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
    const p = nodes[i], q = nodes[j]; let dx = q.x - p.x, dy = q.y - p.y;
    if (!dx && !dy) { dx = Math.random() - .5; dy = Math.random() - .5; }
    const d2 = Math.max(dx * dx + dy * dy, 225), d = Math.sqrt(d2), min = p.r + q.r + 6;
    let f = 190 / d2 * (p.crate === q.crate ? 1 : 1.5);
    if (d < min) f += (min - d) / d * .5;
    dx *= f * a; dy *= f * a; p.vx -= dx; p.vy -= dy; q.vx += dx; q.vy += dy;
  }
  for (const l of links) {
    const p = nodes[l.s], q = nodes[l.t], dx = q.x - p.x, dy = q.y - p.y, d = Math.sqrt(dx * dx + dy * dy) || 1;
    const rest = p.crate === q.crate ? 55 : 150, k = (p.crate === q.crate ? .012 : .003) * Math.min(3, l.w) * a, f = (d - rest) / d * k;
    p.vx += dx * f; p.vy += dy * f; q.vx -= dx * f; q.vy -= dy * f;
  }
  const cent = {};
  for (const p of nodes) { const c = cent[p.group] = cent[p.group] || {x: 0, y: 0, n: 0}; c.x += p.x; c.y += p.y; c.n++; }
  for (const p of nodes) {
    const an = anchors[p.crate], c = cent[p.group];
    p.vx += (an.x - p.x) * .03 * a + (c.x / c.n - p.x) * .03 * a; p.vy += (an.y - p.y) * .03 * a + (c.y / c.n - p.y) * .03 * a;
    if (p.fixed) { p.vx = p.vy = 0; continue; }
    p.vx *= .8; p.vy *= .8;
    const sp = Math.hypot(p.vx, p.vy); if (sp > 12) { p.vx *= 12 / sp; p.vy *= 12 / sp; }
    p.x += p.vx; p.y += p.vy;
  }
  sim.alpha = Math.max(0, a * .99 - .0003);
}
function hull(pts) {
  pts = pts.slice().sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  if (pts.length < 3) return pts;
  const cross = (o, a, b) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]), lo = [], up = [];
  for (const p of pts) { while (lo.length >= 2 && cross(lo[lo.length - 2], lo[lo.length - 1], p) <= 0) lo.pop(); lo.push(p); }
  for (const p of pts.reverse()) { while (up.length >= 2 && cross(up[up.length - 2], up[up.length - 1], p) <= 0) up.pop(); up.push(p); }
  return lo.slice(0, -1).concat(up.slice(0, -1));
}
function renderGraph(stage) {
  const W = Math.max(560, stage.clientWidth), H = Math.max(520, Math.min(760, innerHeight - 220));
  if (!sim || graphReset || sim.nodes.length !== N.length || sim.nodes.some(n => !byId[n.id])) simInit(W, H);
  stage.className = "graph";
  stage.innerHTML = `<svg width="${W}" height="${H}"><g id="vp"><g id="hulls"></g><g id="gedges"></g><g id="gnodes"></g></g></svg>`;
  const svg = stage.querySelector("svg");
  let drag = null, pan = null;
  const toWorld = (cx, cy) => { const r = svg.getBoundingClientRect(); return [(cx - r.left - zoom.x) / zoom.k, (cy - r.top - zoom.y) / zoom.k]; };
  svg.onmousedown = e => {
    const el = e.target.closest("[data-id]");
    if (el) { const p = sim.nodes[sim.idx[el.dataset.id]]; drag = {p, moved: false}; p.fixed = true; }
    else { pan = {x: e.clientX - zoom.x, y: e.clientY - zoom.y, moved: false}; svg.classList.add("panning"); }
  };
  addEventListener("mousemove", graphMove = e => {
    if (drag) { const [x, y] = toWorld(e.clientX, e.clientY); drag.p.x = x; drag.p.y = y; drag.moved = true; sim.alpha = Math.max(sim.alpha, .15); kick(); }
    else if (pan) { zoom.x = e.clientX - pan.x; zoom.y = e.clientY - pan.y; pan.moved = userView = true; draw(); }
  });
  addEventListener("mouseup", graphUp = e => {
    if (drag && !drag.moved) { drag.p.fixed = false; select(drag.p.id); }
    if (pan && !pan.moved && !e.target.closest("[data-id]")) select(null);
    drag = pan = null; svg.classList.remove("panning");
  });
  svg.onwheel = e => { e.preventDefault(); const r = svg.getBoundingClientRect(), mx = e.clientX - r.left, my = e.clientY - r.top, f = Math.exp(-e.deltaY * .0015);
    userView = true; const k = Math.min(4, Math.max(.1, zoom.k * f)); zoom.x = mx - (mx - zoom.x) * k / zoom.k; zoom.y = my - (my - zoom.y) * k / zoom.k; zoom.k = k; draw(); };
  svg.ondblclick = () => { sim.nodes.forEach(n => n.fixed = false); sim.alpha = .6; userView = false; kick(); };
  svg.onmouseover = e => { const el = e.target.closest("[data-id]"); const h = el ? el.dataset.id : null; if (h !== hover) { hover = h; draw(); } };
  kick();
}
let graphMove = null, graphUp = null;
function kick() { if (!rafId) rafId = requestAnimationFrame(loop); }
function loop() {
  rafId = null;
  if (view !== "graph" || !sim) return;
  for (let i = 0; i < 2 && sim.alpha > .005; i++) simTick();
  draw();
  if (sim.alpha > .005) kick();
}
function fit() {
  const svg = $("#stage svg"); if (!svg || userView) return;
  const W = +svg.getAttribute("width"), H = +svg.getAttribute("height"), ns = sim.nodes;
  const x0 = Math.min(...ns.map(n => n.x - n.r)) - 40, x1 = Math.max(...ns.map(n => n.x + n.r)) + 40;
  const y0 = Math.min(...ns.map(n => n.y - n.r)) - 50, y1 = Math.max(...ns.map(n => n.y + n.r)) + 30;
  const k = Math.min(1.6, W / (x1 - x0), H / (y1 - y0));
  zoom = {k, x: W / 2 - (x0 + x1) / 2 * k, y: H / 2 - (y0 + y1) / 2 * k};
}
function draw() {
  const vp = $("#vp"); if (!vp) return;
  fit();
  vp.setAttribute("transform", `translate(${zoom.x},${zoom.y}) scale(${zoom.k})`);
  const id = hover || selected, rel = related(id), {nodes, links} = sim;
  let hulls = "";
  for (const c of D.crates) {
    const ns = nodes.filter(n => n.crate === c.name); if (!ns.length) continue;
    const pts = hull(ns.flatMap(n => [[n.x - n.r, n.y], [n.x + n.r, n.y], [n.x, n.y - n.r], [n.x, n.y + n.r]]));
    const col = `var(${CRATE_COLOR[crateIdx[c.name] % 4]})`, topY = Math.min(...ns.map(n => n.y - n.r)), cx = ns.reduce((s, n) => s + n.x, 0) / ns.length;
    hulls += `<path d="M${pts.map(p => p.join(",")).join("L")}Z" fill="${col}" fill-opacity=".09" stroke="${col}" stroke-opacity=".09" stroke-width="34" stroke-linejoin="round"/>
      <text x="${cx}" y="${topY - 22}" text-anchor="middle" style="font-size:${12 / Math.sqrt(zoom.k)}px;font-weight:600;fill:var(--fg)">${esc(short(c.name))}</text>`;
  }
  let es = "";
  for (const l of links) {
    const p = nodes[l.s], q = nodes[l.t], on = rel && (p.id === id || q.id === id);
    if (rel && !on) continue;
    const col = on ? (p.id === id ? "var(--s1)" : "var(--s2)") : "var(--mute)", op = on ? .85 : p.crate === q.crate ? .14 : .22;
    es += `<line x1="${p.x}" y1="${p.y}" x2="${q.x}" y2="${q.y}" stroke="${col}" stroke-opacity="${op}" stroke-width="${(on ? 1.6 : 1) / Math.sqrt(zoom.k)}"/>`;
  }
  let ns = "";
  for (const p of nodes) {
    const n = byId[p.id], dim = rel ? !rel.has(p.id) : query && !matches(n), live = n.live.length;
    const showLabel = p.r > 10 || p.id === id || (rel && rel.has(p.id)) || live || (query && matches(n));
    ns += `<g data-id="${esc(p.id)}" class="${dim ? "dim" : ""}" style="cursor:pointer" ${tipAttr(nodeTip(n))}>
      ${live ? `<circle cx="${p.x}" cy="${p.y}" r="${p.r + 4}" fill="none" stroke="var(--run)" stroke-width="2.5" class="${n.live.some(l => l.state === "working") ? "pulse" : ""}"/>` : ""}
      <circle cx="${p.x}" cy="${p.y}" r="${p.r}" fill="${fill(n)}" stroke="${p.id === selected ? "var(--fg)" : "var(--card)"}" stroke-width="2"/>
      ${showLabel ? `<text x="${p.x}" y="${p.y + p.r + 11}" text-anchor="middle" style="font-size:${10 / Math.sqrt(zoom.k)}px;fill:var(--fg);pointer-events:none">${esc(n.module.split("::").pop())}</text>` : ""}</g>`;
  }
  $("#hulls").innerHTML = hulls; $("#gedges").innerHTML = es; $("#gnodes").innerHTML = ns;
}

// --- side panel ---------------------------------------------------------------------------
function select(id) { selected = id; renderPanel(); view === "map" ? (renderMap($("#stage"))) : draw(); }
const nodeLink = (id, extra = "") => { const n = byId[id] || D.nodes.find(m => m.id === id); return n ? `<span class="link" onclick="select(${esc(JSON.stringify(id))})">${esc(short(n.crate))} · ${esc(n.module)}</span>${extra}` : esc(id); };
function renderPanel() {
  const el = $("#panel"), now = Date.now() / 1000, start = RANGES[range] ? now - RANGES[range] : 0;
  const n = selected && byId[selected];
  if (!n) {
    const hot = [...N].filter(m => m.churn).sort((a, b) => b.churn - a.churn).slice(0, 10);
    const live = N.filter(m => m.live.length);
    const recent = D.commits.filter(c => c.t >= start).slice(0, 10);
    el.innerHTML = `<h2>Overview</h2><div class="stat"><div><b>${N.length}</b>modules</div><div><b>${N.reduce((s, m) => s + m.lines, 0).toLocaleString()}</b>lines</div><div><b>${E.length}</b>dependencies</div><div><b>${D.commits.filter(c => c.t >= start).length}</b>commits in ${range}</div></div>
      <h3>Changing now <span class="mute small">open PRs and running sessions</span></h3>${live.length ? `<ul>${live.map(m => `<li><span style="color:var(--run)">●</span><span class="grow">${nodeLink(m.id)}<br><span class="small mute">${esc(m.live.map(l => l.label + (l.uncommitted ? " (uncommitted)" : "")).join(", "))}</span></span></li>`).join("")}</ul>` : `<div class="empty small">Nothing in flight.</div>`}
      <h3>Hotspots <span class="mute small">most lines changed in ${range}</span></h3>${hot.length ? `<ul>${hot.map(m => `<li><span class="grow">${nodeLink(m.id)}</span><span class="small mute">${m.churn} lines · ${m.commits}×</span></li>`).join("")}</ul>` : `<div class="empty small">No changes in this range.</div>`}
      <h3>Recent commits</h3><ul>${recent.map(c => `<li><span class="grow">${esc(c.subject)}</span><span class="small mute nowrap">${ago(c.t)}</span></li>`).join("") || '<li class="mute">none in this range</li>'}</ul>
      <p class="small mute">Hover a module to see what it uses (blue) and what uses it (orange). Click it for details.</p>`;
    return;
  }
  const hist = n.history.map(([ci, a, d]) => ({...D.commits[ci], a, d})).sort((x, y) => y.t - x.t);
  const deps = (list, key) => list.sort((x, y) => y.w - x.w).slice(0, 14).map(e => `<li><span class="grow">${nodeLink(e[key])}</span><span class="small mute">${e.w}×</span></li>`).join("");
  el.innerHTML = `<h2>${esc(short(n.crate))} <span class="right"><button onclick="select(null)">Close</button></span></h2>
    <div style="font-size:15px;font-weight:600">${esc(n.module)}</div><div class="path">${esc(n.id)}</div>
    <div class="stat"><div><b>${n.lines}</b>lines</div><div><b>${ago(n.last).replace(" ago", "")}</b>since last change</div><div><b>${n.commits}</b>commits in ${range}</div><div><b>${n.churn}</b>lines changed</div></div>
    ${n.live.length ? `<h3>Changing now</h3><ul>${n.live.map(l => `<li><span style="color:var(--run)">●</span><span class="grow">${l.url ? `<a href="${esc(l.url)}" target="_blank">${esc(l.label)}</a>` : esc(l.label)} <span class="small mute">${esc(l.title)}${l.add != null ? ` · +${l.add} −${l.del}` : ""}${l.uncommitted ? " · uncommitted" : ""}</span></span></li>`).join("")}</ul>` : ""}
    <h3>History</h3><ul>${hist.slice(0, 10).map(c => `<li><span class="grow">${esc(c.subject)}</span><span class="small mute nowrap">+${c.a} −${c.d} · ${ago(c.t)}</span></li>`).join("") || '<li class="mute">no commits</li>'}</ul>
    <h3><span style="color:var(--s1)">■</span> Uses <span class="mute small">${n.out.length}</span></h3><ul>${deps(n.out, "b") || '<li class="mute">nothing in the workspace</li>'}</ul>
    <h3><span style="color:var(--s2)">■</span> Used by <span class="mute small">${n.in.length}</span></h3><ul>${deps(n.in, "a") || '<li class="mute">no module</li>'}</ul>`;
}

function renderLegend() {
  const steps = metric === "age" ? AGE_STEPS.map((s, i) => [6 - i, s[1]]).concat([[0, "older"]]) : [[0, "none"], [1, "few"], [6, "most"]];
  $("#legend").innerHTML = `<span>${METRICS[metric]}${metric === "age" ? "" : " in " + range}:</span>` +
    steps.map(([l, t]) => `<span><i style="background:${l ? `var(--q${l})` : "var(--chip)"}"></i>${t}</span>`).join("") +
    `<span><span class="ring"></span>changing now (open PR or running session)</span><span>size: lines of code</span>` +
    (view === "graph" ? `<span>drag to pan · scroll to zoom · drag a node to pin it · double-click to relax</span>` : "");
}

function render() {
  if (!D) return;
  controls(); derive();
  const stage = $("#stage");
  if (graphMove) { removeEventListener("mousemove", graphMove); removeEventListener("mouseup", graphUp); graphMove = graphUp = null; }
  if (view === "graph") renderGraph(stage); else renderMap(stage);
  renderLegend(); renderPanel();
}

const tip = $("#tip");
document.addEventListener("mousemove", e => {
  const t = e.target.closest && e.target.closest("[data-tip]");
  if (!t) { tip.style.display = "none"; return; }
  tip.innerHTML = t.dataset.tip; tip.style.display = "block";
  const r = tip.getBoundingClientRect();
  tip.style.left = Math.min(e.clientX + 14, innerWidth - r.width - 8) + "px";
  tip.style.top = (e.clientY + 14 + r.height > innerHeight ? e.clientY - r.height - 10 : e.clientY + 14) + "px";
});
let lastSha = null, lastLive = null;
async function load() {
  try {
    const r = await fetch("/api/code"), d = await r.json();
    if (d.error) throw new Error(d.error);
    const liveKey = JSON.stringify(d.live);
    $("#live").innerHTML = `<span class="dot bg-ok pulse"></span>live · origin/main ${esc(d.sha)} · checked ${new Date(d.generated).toLocaleTimeString()}`;
    if (d.sha !== lastSha || liveKey !== lastLive) { const first = !D; D = d; lastSha = d.sha; lastLive = liveKey; if (first) graphReset = true; render(); }
  } catch (e) { $("#live").innerHTML = `<span class="dot bg-bad"></span><span class="bad">${esc(e.message || "dashboard server not reachable")}</span>`; }
}
let rt; addEventListener("resize", () => { clearTimeout(rt); rt = setTimeout(render, 150); });
load(); setInterval(load, 20000);
</script></body></html>
""".replace("/*BASE*/", BASE_CSS)


PAGE_ROADMAP = r"""<!doctype html>
<html lang="en"><head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>ArchiCraft Roadmap</title>
<style>/*BASE*/
.hero{display:grid;grid-template-columns:minmax(260px,1.3fr) repeat(3,minmax(180px,1fr));gap:12px}
@media (max-width:1000px){.hero{grid-template-columns:1fr 1fr}}
@media (max-width:600px){.hero{grid-template-columns:1fr}}
.tile{background:var(--chip);border-radius:8px;padding:10px 12px;min-width:0}
.tile .k{font-size:11px;color:var(--mute)}
.tile .big{font-size:48px;font-weight:600;line-height:1.05;font-variant-numeric:tabular-nums}
.tile .mid{font-size:22px;font-weight:600;font-variant-numeric:tabular-nums}
.track{height:10px;background:var(--card);border-radius:5px;overflow:hidden;display:flex;gap:2px;margin:8px 0 6px}
.track i{display:block;height:100%}
.chips{display:flex;flex-wrap:wrap;gap:4px;margin-top:6px}
.as{font-size:11px;border-radius:4px;padding:1px 6px;background:var(--card);display:inline-flex;gap:4px;align-items:center}
#gantt svg text{font-size:10px}
.bar-lbl{pointer-events:none;font-size:10px}
.epic{cursor:pointer}.epic.dim{opacity:.3}
.cols{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,420px),1fr));gap:14px}
.inc{border-top:1px solid var(--line);padding:9px 0}.inc:first-of-type{border-top:0;padding-top:0}
.inc .row{display:flex;justify-content:space-between;gap:8px;align-items:baseline}
.req-row{display:grid;grid-template-columns:70px 1fr 120px;gap:8px;align-items:center;margin:6px 0;font-size:12px}
.seg-bar{height:14px;display:flex;gap:2px;border-radius:4px;overflow:hidden;background:var(--chip)}
.seg-bar i{display:block;height:100%}
.task{display:grid;grid-template-columns:16px 1fr auto;gap:6px;padding:4px 0;border-top:1px solid var(--line);font-size:12px;align-items:baseline}
.task:first-child{border-top:0}
</style></head><body>
<header><h1>ArchiCraft · bead loop</h1>
<nav class="tabs"><a class="on" href="/roadmap">Roadmap</a><a href="/">Status</a><a href="/code">Code map</a></nav>
<span class="live" id="live"><span class="dot bg-mute pulse"></span>loading the roadmap…</span>
<span class="meta" id="meta"></span></header>
<main id="main"></main>
<div id="tip"></div>
<script>
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const tipAttr = html => `data-tip="${esc(html)}"`;
const h = m => Math.round(m / 60).toLocaleString();
const pct = (a, b) => b ? Math.round(100 * a / b) : 0;
const sec = (title, body, cls = "", right = "") => `<section class="${cls}"><h2>${title}${right ? `<span class="right">${right}</span>` : ""}</h2>${body}</section>`;
let R = null, selected = null, hover = null;
const STATE = {done: ["--ok", "done"], in_progress: ["--s1", "in progress"], planned: ["--mute", "planned"]};
const AS = {done: ["bg-ok", "passes"], partial: ["bg-warn", "partial"], "not-yet-possible": ["bg-mute", "not yet possible"], missing: ["bg-mute", "missing"]};

function incStats(n) {
  const es = R.epics.filter(e => !n || e.inc === n);
  return {epics: es.length, done: es.filter(e => e.state === "done").length, active: es.filter(e => e.state === "in_progress").length,
    est: es.reduce((s, e) => s + e.est_total, 0), estDone: es.reduce((s, e) => s + e.est_done, 0),
    days: es.reduce((s, e) => s + e.days, 0), end: Math.max(0, ...es.map(e => e.start + e.days)),
    tasks: es.reduce((s, e) => s + e.leaves, 0), tasksDone: es.reduce((s, e) => s + e.leaves_done, 0),
    live: es.reduce((s, e) => s + e.live.length, 0)};
}
function reqTotals() {
  const out = {done: 0, partial: 0, missing: 0, all: 0};
  for (const p of "MSC") for (const [k, v] of Object.entries(R.reqs[p] || {})) { out[k] = (out[k] || 0) + v; out.all += v; }
  return out;
}

// --- hero -----------------------------------------------------------------------------------
function hero() {
  const i1 = incStats(1), all = incStats(0), rt = reqTotals(), M = R.reqs.M || {}, mAll = Object.values(M).reduce((a, b) => a + b, 0);
  const first = R.req_history[0], last = R.req_history[R.req_history.length - 1];
  const gainedM = first && last ? last.done_M - first.done_M : 0;
  const pace = R.pace_minutes_per_day, left = i1.est - i1.estDone, days = pace > 0 ? left / pace : null;
  const eta = days ? new Date(Date.now() + days * 86400000).toLocaleDateString(undefined, {day: "numeric", month: "short", year: "numeric"}) : null;
  const incSegs = [1, 2, 3, 4].map(n => { const s = incStats(n); return `<i style="width:${100 * s.est / all.est}%;background:linear-gradient(90deg,var(--s1) ${pct(s.estDone, s.est)}%,color-mix(in srgb,var(--mute) 25%,transparent) 0)" ${tipAttr(`<b>Increment ${n}</b><br>${pct(s.estDone, s.est)}% · ${h(s.estDone)} of ${h(s.est)} h estimated`)}></i>`; }).join("");
  const scen = Object.entries(R.scenarios).map(([k, v]) => `<span class="as" ${tipAttr(`${k}: ${(AS[v] || ["", v])[1]}`)}><span class="dot ${(AS[v] || ["bg-mute"])[0]}" style="margin:0"></span>${k}</span>`).join("");
  return sec("Where we are", `<div class="hero">
    <div class="tile"><div class="k">Increment 1 · core modeller</div><div class="big">${pct(i1.estDone, i1.est)}%</div>
      <div class="track"><i style="width:${pct(i1.estDone, i1.est)}%;background:var(--s1)"></i></div>
      <div class="small mute">${h(i1.estDone)} of ${h(i1.est)} h of estimates done · ${i1.tasksDone} of ${i1.tasks} tasks · ${i1.done} of ${i1.epics} epics closed, ${i1.active} under way</div></div>
    <div class="tile"><div class="k">Whole roadmap</div><div class="mid">${pct(all.estDone, all.est)}%</div>
      <div class="track" style="height:12px">${incSegs}</div>
      <div class="small mute">four increments, ${R.epics.length} epics, ${all.days} engineer-days planned · ${all.tasksDone} of ${all.tasks} tasks done</div></div>
    <div class="tile"><div class="k">SRS requirements done</div><div class="mid">${rt.done} <span class="small mute">of ${rt.all}</span></div>
      <div class="track" style="height:12px"><i style="width:${pct(rt.done, rt.all)}%;background:var(--ok)"></i><i style="width:${pct(rt.partial, rt.all)}%;background:var(--warn)"></i></div>
      <div class="small mute">Must: ${M.done || 0} of ${mAll} done${gainedM ? `, +${gainedM} since ${new Date(first.t * 1000).toLocaleDateString(undefined, {day: "numeric", month: "short"})}` : ""} · ${rt.partial} partial</div></div>
    <div class="tile"><div class="k">Pace</div><div class="mid">${(pace / 60).toFixed(1)} h<span class="small mute"> of estimates a day</span></div>
      <div class="small mute" style="margin-top:6px">average of the last 7 days${eta ? ` · increment 1 at this pace: about ${Math.round(days)} days, around ${eta}` : ""}</div>
      <div class="k" style="margin-top:8px">Acceptance scenarios</div><div class="chips">${scen}</div></div>
  </div>`, "wide");
}

// --- gantt ----------------------------------------------------------------------------------
let boxes = {};
function gantt(el) {
  const W = Math.max(640, el.clientWidth), lw = 170, rowH = 22, end = Math.max(...R.epics.map(e => e.start + e.days)), x = d => lw + d / end * (W - lw - 10);
  let y = 22, g = "", grid = "";
  for (let d = 0; d <= end; d += 10) grid += `<line class="grid" x1="${x(d)}" x2="${x(d)}" y1="16" y2="100%"/><text x="${x(d)}" y="11" text-anchor="middle">${d}</text>`;
  boxes = {};
  for (const n of [1, 2, 3, 4]) {
    const es = R.epics.filter(e => e.inc === n).sort((a, b) => a.start - b.start || b.days - a.days), lanes = [];
    for (const e of es) { let i = lanes.findIndex(endD => endD <= e.start); if (i < 0) { i = lanes.length; lanes.push(0); } lanes[i] = e.start + e.days; e.lane = i; }
    const bandH = lanes.length * rowH + 14, s = incStats(n);
    g += `<rect x="0" y="${y}" width="${W}" height="${bandH}" fill="${n % 2 ? "var(--chip)" : "transparent"}" opacity=".5" rx="6"/>
      <text x="8" y="${y + 16}" style="font-size:12px;font-weight:600;fill:var(--fg)">Increment ${n}</text>
      <text x="8" y="${y + 30}">${pct(s.estDone, s.est)}% · ${s.done}/${s.epics} epics</text>
      <text x="8" y="${y + 43}" ${tipAttr(`<b>Exit criterion</b><br>${esc(R.exits[n] || "")}`)} style="text-decoration:underline dotted">exit criterion</text>`;
    for (const e of es) {
      const bx = x(e.start) + 1, bw = Math.max(4, x(e.start + e.days) - x(e.start) - 2), by = y + 7 + e.lane * rowH, bh = rowH - 5;
      boxes[e.key] = {x0: bx, x1: bx + bw, y: by + bh / 2};
      const p = e.est_total ? e.est_done / e.est_total : 0, col = e.state === "done" ? "var(--ok)" : "var(--s1)";
      const name = e.key.length * 5.6 + 34 < bw ? e.key : e.key.length * 5.6 < bw ? e.key : bw > 30 ? e.key.slice(0, Math.floor(bw / 6) - 1) + "…" : "";
      g += `<g class="epic" data-key="${esc(e.key)}" ${tipAttr(`<b>${esc(e.title)}</b><br><span class="mute">${esc(e.key)} · ${esc(e.id || "")}</span><br>${e.days} engineer-days, from day ${e.start}<br>${pct(e.est_done, e.est_total)}% done · ${e.leaves_done} of ${e.leaves} tasks · ${h(e.est_done)} of ${h(e.est_total)} h${e.live.length ? `<br><span style="color:var(--run)">●</span> ${e.live.length} agent session${e.live.length > 1 ? "s" : ""} working` : ""}${e.deps.length ? `<br>needs ${esc(e.deps.join(", "))}` : ""}`)}>
        <rect x="${bx}" y="${by}" width="${bw}" height="${bh}" rx="4" fill="var(--card)" stroke="${e.key === selected ? "var(--fg)" : "var(--line)"}" stroke-width="${e.key === selected ? 2 : 1}"/>
        ${p > 0 ? `<rect x="${bx}" y="${by}" width="${Math.max(3, bw * p)}" height="${bh}" rx="4" fill="${col}" opacity="${e.state === "done" ? 1 : .85}"/>` : ""}
        <text class="bar-lbl" x="${bx + 5}" y="${by + bh / 2 + 3.5}" style="fill:${p > .45 ? "#fff" : "var(--fg)"}">${esc(name)}</text>
        ${e.live.length ? `<circle cx="${bx + bw - 6}" cy="${by + bh / 2}" r="3.5" fill="var(--run)" class="${e.live.some(l => l.state === "working") ? "pulse" : ""}"/>` : ""}</g>`;
    }
    y += bandH + 6;
  }
  el.innerHTML = `<svg width="${W}" height="${y}"><g>${grid}</g>${g}<g id="deps"></g></svg>`;
  el.querySelectorAll(".epic").forEach(n => {
    n.onmouseenter = () => { hover = n.dataset.key; depLines(); };
    n.onmouseleave = () => { hover = null; depLines(); };
    n.onclick = () => { selected = selected === n.dataset.key ? null : n.dataset.key; render(); };
  });
  depLines();
}
function depLines() {
  const k = hover || selected, el = $("#deps"); if (!el) return;
  if (!k) { el.innerHTML = ""; document.querySelectorAll(".epic").forEach(n => n.classList.remove("dim")); return; }
  const e = R.epics.find(x => x.key === k), users = R.epics.filter(x => x.deps.includes(k)).map(x => x.key), rel = new Set([k, ...e.deps, ...users]);
  document.querySelectorAll(".epic").forEach(n => n.classList.toggle("dim", !rel.has(n.dataset.key)));
  const link = (a, b, col) => { const p = boxes[a], q = boxes[b]; if (!p || !q) return "";
    const mx = (p.x1 + q.x0) / 2; return `<path d="M${p.x1},${p.y} C${mx},${p.y} ${mx},${q.y} ${q.x0},${q.y}" fill="none" stroke="var(${col})" stroke-width="1.6" opacity=".85"/>`; };
  el.innerHTML = e.deps.map(d => link(d, k, "--s2")).join("") + users.map(u => link(k, u, "--s1")).join("");
}

// --- increments and requirements --------------------------------------------------------------
function increments() {
  return sec("Increments", [1, 2, 3, 4].map(n => { const s = incStats(n);
    return `<div class="inc"><div class="row"><b>Increment ${n}</b><span class="small mute">${s.epics} epics · ${s.days} engineer-days · ends at day ${s.end}</span></div>
      <div class="track" style="background:var(--chip)"><i style="width:${pct(s.estDone, s.est)}%;background:var(--s1)"></i></div>
      <div class="small"><b>${pct(s.estDone, s.est)}%</b> <span class="mute">· ${h(s.estDone)} of ${h(s.est)} h · ${s.tasksDone} of ${s.tasks} tasks · ${s.done} epics closed, ${s.active} under way${s.live ? ` · <span style="color:var(--run)">●</span> ${s.live} agents working` : ""}</span></div>
      <div class="small mute" style="margin-top:3px">Exit: ${esc(R.exits[n] || "")}</div></div>`; }).join(""));
}
function requirements() {
  const rows = [["M", "Must"], ["S", "Should"], ["C", "Could"]].map(([p, l]) => { const c = R.reqs[p] || {}, t = (c.done || 0) + (c.partial || 0) + (c.missing || 0);
    return `<div class="req-row"><span>${l}</span><div class="seg-bar" ${tipAttr(`<b>${l}</b>: ${c.done || 0} done, ${c.partial || 0} partial, ${c.missing || 0} missing`)}>
      <i style="width:${100 * (c.done || 0) / t}%;background:var(--ok)"></i><i style="width:${100 * (c.partial || 0) / t}%;background:var(--warn)"></i></div>
      <span class="small mute">${c.done || 0} done · ${c.partial || 0} partial · ${t}</span></div>`; }).join("");
  return sec("SRS requirements", `<div class="legend"><span><i style="background:var(--ok)"></i>done</span><span><i style="background:var(--warn)"></i>partial</span><span><i style="background:var(--chip)"></i>missing</span></div>${rows}
    <h3>Done over time <span class="small mute">from docs/implementation-status.md on main</span></h3><div id="reqchart"></div>`);
}
function reqChart(el) {
  const H = R.req_history; if (H.length < 2) { el.innerHTML = `<div class="empty small">Not enough history yet.</div>`; return; }
  const W = el.clientWidth || 400, Hh = 150, m = {l: 34, r: 60, t: 10, b: 18}, t0 = H[0].t, t1 = Math.max(Date.now() / 1000, H[H.length - 1].t);
  const S = [["Must done", r => r.done_M, "--s1"], ["All done", r => r.done_M + r.done_S + r.done_C, "--s2"]];
  // The counts move by a few requirements at a time, so the axis spans the data, not 0..max.
  const vals = H.flatMap(r => S.map(s => s[1](r))), lo = Math.max(0, Math.floor(Math.min(...vals) * .9 / 5) * 5), hi = Math.ceil(Math.max(...vals) * 1.05 / 5) * 5 || 5;
  const x = t => m.l + (t - t0) / (t1 - t0) * (W - m.l - m.r), y = v => m.t + (Hh - m.t - m.b) * (1 - (v - lo) / (hi - lo));
  let g = "";
  for (const f of [0, .5, 1]) { const v = lo + (hi - lo) * f; g += `<line class="grid" x1="${m.l}" x2="${W - m.r}" y1="${y(v)}" y2="${y(v)}"/><text x="${m.l - 5}" y="${y(v) + 3}" text-anchor="end">${Math.round(v)}</text>`; }
  g += `<text x="${m.l}" y="${Hh - 3}">${new Date(t0 * 1000).toLocaleDateString(undefined, {day: "numeric", month: "short"})}</text><text x="${W - m.r}" y="${Hh - 3}" text-anchor="end">now</text>`;
  for (const [label, f, col] of S) {
    const pts = H.map(r => [x(r.t), y(f(r))]); pts.push([x(t1), y(f(H[H.length - 1]))]);
    g += `<polyline points="${pts.map(p => p.join(",")).join(" ")}" fill="none" stroke="var(${col})" stroke-width="2" stroke-linejoin="round"/>
      <circle cx="${pts[pts.length - 1][0]}" cy="${pts[pts.length - 1][1]}" r="4" fill="var(${col})" stroke="var(--card)" stroke-width="2"/>
      <text x="${pts[pts.length - 1][0] + 8}" y="${pts[pts.length - 1][1] + 3}" style="fill:var(--fg)">${f(H[H.length - 1])} ${label.split(" ")[0].toLowerCase()}</text>`;
  }
  H.forEach((r, i) => { const x0 = i ? (x(H[i - 1].t) + x(r.t)) / 2 : m.l, x1 = i < H.length - 1 ? (x(r.t) + x(H[i + 1].t)) / 2 : W - m.r;
    g += `<rect class="hit" x="${x0}" y="${m.t}" width="${Math.max(1, x1 - x0)}" height="${Hh - m.t - m.b}" ${tipAttr(`<b>${new Date(r.t * 1000).toLocaleString()}</b> · ${r.sha}<br>Must done: ${r.done_M} · all done: ${r.done_M + r.done_S + r.done_C}<br>partial: ${r.partial_M + r.partial_S + r.partial_C}`)}/>`; });
  el.innerHTML = `<div class="legend">${S.map(([l, , c]) => `<span><i style="background:var(${c})"></i>${l}</span>`).join("")}</div><svg width="${W}" height="${Hh}">${g}</svg>`;
}

// --- detail -----------------------------------------------------------------------------------
function taskRow(t) {
  const icon = t.status === "closed" ? '<span class="ok">✓</span>' : t.live.length ? '<span class="dot bg-run pulse" style="margin:0"></span>' : t.status === "in_progress" ? '<span class="run">◐</span>' : t.ready ? '<span class="ok">○</span>' : '<span class="mute">○</span>';
  return `<div class="task">${icon}<span><span class="id">${esc(t.id)}</span> ${esc(t.title)}${t.children ? ` <span class="small mute">(split into ${t.children})</span>` : ""}${t.ready && t.status !== "closed" ? ' <span class="chip ok">ready</span>' : ""}</span>
    <span class="small mute nowrap">${t.est ? `${h(t.done)}/${h(t.est)} h` : ""}</span></div>`;
}
function detail() {
  if (!selected) {
    const active = R.epics.filter(e => e.state === "in_progress");
    return sec("Under way", active.length ? active.map(e => `<h3 style="cursor:pointer" onclick="selected='${esc(e.key)}';render()">${esc(e.title)} <span class="small mute">${pct(e.est_done, e.est_total)}% · ${e.leaves_done}/${e.leaves} tasks</span></h3>` +
      e.tasks.filter(t => t.status !== "closed" && (t.status === "in_progress" || t.live.length || t.ready)).slice(0, 6).map(taskRow).join("")).join("")
      : `<div class="empty">No epic is under way yet.</div>`, "wide", `<span class="small mute">click a bar for an epic's tasks · ✓ done · ● agent working · ◐ in progress · ○ ready or open</span>`);
  }
  const e = R.epics.find(x => x.key === selected), users = R.epics.filter(x => x.deps.includes(e.key));
  const link = k => `<span class="chip" style="cursor:pointer" onclick="selected='${esc(k)}';render()">${esc(k)}</span>`;
  return sec(`${esc(e.title)}`, `<div class="small mute">${esc(e.key)} · ${esc(e.id || "")} · increment ${e.inc} · P${e.prio} · ${e.days} engineer-days from day ${e.start} · requirements M ${e.must} / S ${e.should} / C ${e.could}${e.done_reqs ? ` (+${e.done_reqs} already done)` : ""}</div>
    <div class="track" style="background:var(--chip);max-width:520px"><i style="width:${pct(e.est_done, e.est_total)}%;background:${e.state === "done" ? "var(--ok)" : "var(--s1)"}"></i></div>
    <div class="small"><b>${pct(e.est_done, e.est_total)}%</b> <span class="mute">· ${h(e.est_done)} of ${h(e.est_total)} h · ${e.leaves_done} of ${e.leaves} tasks done · ${e.in_progress} in progress</span></div>
    <div style="margin:8px 0">${e.deps.length ? `<span class="small mute">needs </span>${e.deps.map(link).join("")}` : '<span class="small mute">no prerequisites</span>'}${users.length ? ` <span class="small mute">· unblocks </span>${users.map(x => link(x.key)).join("")}` : ""}</div>
    ${e.tasks.map(taskRow).join("")}`, "wide", `<button onclick="selected=null;render()">Close</button>`);
}

function render() {
  if (!R) return;
  const scroll = scrollY;
  $("#main").innerHTML = hero() + sec("Roadmap", `<div class="small mute" style="margin-bottom:6px">Epics placed by their dependencies: earliest start in engineer-days with unlimited parallelism, as in docs/roadmap.md. The filled part of a bar is the share of its task estimates that is done. Hover a bar for what it needs (orange) and unblocks (blue).</div><div id="gantt"></div>`, "wide") +
    detail() + increments() + requirements();
  gantt($("#gantt")); reqChart($("#reqchart"));
  scrollTo(0, scroll);
}

const tip = $("#tip");
document.addEventListener("mousemove", e => {
  const t = e.target.closest && e.target.closest("[data-tip]");
  if (!t) { tip.style.display = "none"; return; }
  tip.innerHTML = t.dataset.tip; tip.style.display = "block";
  const r = tip.getBoundingClientRect();
  tip.style.left = Math.min(e.clientX + 14, innerWidth - r.width - 8) + "px";
  tip.style.top = (e.clientY + 14 + r.height > innerHeight ? e.clientY - r.height - 10 : e.clientY + 14) + "px";
});
let lastKey = null;
async function load() {
  try {
    const r = await fetch("/api/roadmap"), d = await r.json();
    if (d.error) throw new Error(d.error);
    $("#live").innerHTML = `<span class="dot bg-ok pulse"></span>live · origin/main ${esc(d.sha)} · checked ${new Date(d.generated).toLocaleTimeString()}`;
    const key = JSON.stringify([d.sha, d.epics.map(e => [e.est_done, e.state, e.live.length, e.tasks.map(t => t.status + t.live.length)])]);
    if (key !== lastKey) { lastKey = key; R = d; render(); }
  } catch (e) { $("#live").innerHTML = `<span class="dot bg-bad"></span><span class="bad">${esc(e.message || "dashboard server not reachable")}</span>`; }
}
let rt; addEventListener("resize", () => { clearTimeout(rt); rt = setTimeout(render, 150); });
load(); setInterval(load, 20000);
</script></body></html>
""".replace("/*BASE*/", BASE_CSS)


def main():
    global SNAPSHOTS, PORT
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--interval", type=int, default=20, help="seconds between data refreshes")
    ap.add_argument("--no-open", action="store_true", help="do not open a browser")
    args = ap.parse_args()

    SNAPSHOTS, PORT = Snapshots(), args.port
    threading.Thread(target=refresher, args=(args.interval,), daemon=True).start()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    url = f"http://127.0.0.1:{args.port}/"
    print(f"ArchiCraft loop dashboard on {url} (Ctrl+C stops it)")
    if not args.no_open:
        webbrowser.open(url)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
