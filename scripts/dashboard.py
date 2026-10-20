#!/usr/bin/env python3
"""Local status dashboard for the bead loop: beads, agent sessions, pull requests and history.

Serves one page on 127.0.0.1 that keeps itself up to date. It runs bd, gh, git, ps and lsof
and reads the Claude Code transcripts of this repository's sessions. It changes nothing on its
own: only the Cleanup panel does, after a preview and a confirmation on the page (stop stale
sessions, remove finished worktrees and branches, run the sweep). Python 3 standard library only.

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
               "additions,deletions,changedFiles")
ALL_FIELDS = "number,title,state,headRefName,headRefOid,url,createdAt,mergedAt,closedAt"


def collect_prs():
    return {
        "open": run_json(["gh", "pr", "list", "--state", "open", "--limit", "50",
                          "--json", OPEN_FIELDS], cwd=ROOT),
        "all": run_json(["gh", "pr", "list", "--state", "all", "--limit", "300",
                         "--json", ALL_FIELDS], cwd=ROOT),
    }


def pr_bead(p):
    m = BEAD_BRANCH.match(p.get("headRefName") or "") or BEAD_TITLE.match(p.get("title") or "")
    return m.group(1) if m else None


def summarize_checks(rollup):
    """Latest run per check name, reduced to pass/fail/pending counts."""
    latest = {}
    for c in rollup or []:
        name = c.get("name") or c.get("context") or "?"
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


def open_pr_view(p):
    return {
        "number": p["number"], "title": p["title"], "draft": p["isDraft"],
        "branch": p["headRefName"], "url": p["url"], "bead": pr_bead(p),
        "updated": p.get("updatedAt"), "labels": [l["name"] for l in p.get("labels") or []],
        "auto_merge": bool(p.get("autoMergeRequest")),
        "merge_state": p.get("mergeStateStatus"), "mergeable": p.get("mergeable"),
        "review": p.get("reviewDecision") or "",
        "checks": summarize_checks(p.get("statusCheckRollup")),
        "size": f"+{p.get('additions', 0)} −{p.get('deletions', 0)} · {p.get('changedFiles', 0)} files",
    }


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
  const c = p.checks;
  if (p.draft) return `<span class="dot bg-mute"></span>draft`;
  if (c.fail) return `<span class="dot bg-bad"></span>checks failing`;
  if (p.merge_state === "DIRTY" || p.mergeable === "CONFLICTING") return `<span class="dot bg-bad"></span>merge conflict`;
  if (c.pending) return `<span class="dot bg-run pulse"></span>checks running`;
  if (p.merge_state === "BEHIND") return `<span class="dot bg-warn"></span>behind main`;
  if (!p.auto_merge) return `<span class="dot bg-warn"></span>waiting for your review`;
  return `<span class="dot bg-ok"></span>green, merging`;
}
function prRow(p) {
  const c = p.checks;
  const checks = `<span class="ok">${c.pass} passed</span>${c.fail?` · <span class="bad">${c.fail} failed</span>`:""}${c.pending?` · <span class="run">${c.pending} running</span>`:""}${c.skipped?` · <span class="mute">${c.skipped} skipped</span>`:""}`;
  const detail = [c.failing.length ? `<span class="bad">failing: ${esc(c.failing.join(", "))}</span>` : "",
                  c.running.length ? `<span class="mute">running: ${esc(c.running.join(", "))}</span>` : ""].filter(Boolean).join("<br>");
  return `<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td>
    <td>${esc(p.title)}<div class="small mute">${esc(p.branch)} · ${esc(p.size)} · updated ${ago(p.updated)}</div>
      <div>${chips(p.labels)}${p.auto_merge?'<span class="chip ok">auto-merge on</span>':'<span class="chip warn">auto-merge off</span>'}${p.review?`<span class="chip">${esc(p.review.toLowerCase())}</span>`:''}</div>
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
  for (const [k, v] of Object.entries(s.errors || {})) out.push(`<div class="err"><b>${esc(k)}</b> could not be read, showing its last good data: ${esc(v)}</div>`);
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
  (s.prs_open || []).filter(p => !p.auto_merge && !p.draft).forEach(p => needs.push(`<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td></td><td>${esc(p.title)}</td><td class="small mute">PR waits for your review and merge</td></tr>`));
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
