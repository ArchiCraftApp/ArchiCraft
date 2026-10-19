#!/usr/bin/env python3
"""Local status dashboard for the bead loop: beads, running agents and pull requests.

Serves one page on 127.0.0.1 that refreshes itself. Read-only: it runs bd, gh, git, ps and
lsof and never changes a bead, a branch or a PR. Python 3 standard library only.

Usage: scripts/dashboard.py [--port 8765] [--interval 20] [--no-open]
"""

import argparse
import concurrent.futures
import json
import os
import re
import subprocess
import threading
import time
import webbrowser
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

HERE = Path(__file__).resolve().parent


def run(cmd, cwd=None, timeout=60):
    """Runs a command and returns its stdout; raises with stderr on failure."""
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    if p.returncode != 0:
        raise RuntimeError(f"{' '.join(cmd)}: {(p.stderr or p.stdout).strip()[:300]}")
    return p.stdout


def run_json(cmd, cwd=None, timeout=60):
    out = run(cmd, cwd=cwd, timeout=timeout).strip()
    return json.loads(out) if out else []


def repo_root():
    """The main checkout, also when the script runs from a worktree."""
    common = run(["git", "rev-parse", "--path-format=absolute", "--git-common-dir"], cwd=HERE)
    return Path(common.strip()).parent


ROOT = repo_root()


# --- beads -------------------------------------------------------------------------------

def collect_beads():
    beads = run_json(["bd", "list", "--all", "--brief", "--flat", "--json", "-n", "0"], cwd=ROOT)
    ready = run_json(["bd", "ready", "--json", "-n", "0"], cwd=ROOT)
    blocked = run_json(["bd", "blocked", "--json"], cwd=ROOT)
    epics = run_json(["bd", "epic", "status", "--json"], cwd=ROOT)

    slim = lambda b: {k: b.get(k) for k in (
        "id", "title", "status", "priority", "issue_type", "assignee", "labels",
        "estimated_minutes", "updated_at", "closed_at", "external_ref", "parent")}
    counts = {}
    for b in beads:
        counts[b["status"]] = counts.get(b["status"], 0) + 1
    counts["ready"] = len(ready)
    counts["blocked_by_deps"] = len(blocked)

    by_prio = lambda b: (b.get("priority", 9), b.get("created_at", ""))
    in_progress = sorted((slim(b) for b in beads if b["status"] == "in_progress"), key=by_prio)
    human = [slim(b) for b in beads
             if b["status"] != "closed" and "human" in (b.get("labels") or [])]
    closed = sorted((b for b in beads if b["status"] == "closed"),
                    key=lambda b: b.get("closed_at") or b.get("updated_at") or "", reverse=True)
    ready_tasks = [slim(b) for b in sorted(ready, key=by_prio) if b.get("issue_type") != "epic"]

    epic_rows = []
    for e in epics:
        ep = e.get("epic", {})
        if ep.get("status") == "closed":
            continue
        epic_rows.append({
            "id": ep.get("id"), "title": ep.get("title"), "priority": ep.get("priority"),
            "status": ep.get("status"), "labels": ep.get("labels") or [],
            "total": e.get("total_children", 0), "closed": e.get("closed_children", 0),
            "eligible": e.get("eligible_for_close", False),
        })
    epic_rows.sort(key=lambda e: (-(e["closed"] / e["total"] if e["total"] else 0),
                                  e["priority"] or 9, e["id"]))

    return {
        "counts": counts,
        "in_progress": in_progress,
        "human": human,
        "ready": ready_tasks,
        "recently_closed": [slim(b) for b in closed[:10]],
        "epics": epic_rows,
        "by_priority": {str(p): sum(1 for b in beads if b["status"] != "closed"
                                    and b.get("priority") == p) for p in range(5)},
    }


# --- worktrees and agents ----------------------------------------------------------------

BEAD_BRANCH = re.compile(r"^bead/([a-z0-9]+-[a-z0-9]+(?:\.[0-9]+)*)(?:-|$)")


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
        elif line.startswith("HEAD "):
            cur["head"] = line[5:12]
        elif line == "detached":
            cur["branch"] = "(detached)"
    for t in trees:
        m = BEAD_BRANCH.match(t.get("branch", ""))
        t["bead"] = m.group(1) if m else None
        t["name"] = Path(t["path"]).name if t["path"] != str(ROOT) else "main checkout"
    return trees


def claude_processes():
    """Claude Code processes, keyed by pid, with their working directory."""
    try:
        out = run(["lsof", "-a", "-d", "cwd", "-c", "claude", "-Fpn"], timeout=15)
    except RuntimeError as e:
        # lsof exits 1 when nothing matches.
        if "lsof" in str(e) and not str(e).split(":", 1)[1].strip():
            return {}
        raise
    procs, pid = {}, None
    for line in out.splitlines():
        if line.startswith("p"):
            pid = int(line[1:])
        elif line.startswith("n") and pid:
            procs[pid] = {"pid": pid, "cwd": line[1:]}
    if not procs:
        return {}
    ps = run(["ps", "-o", "pid=,etime=,pcpu=,command=", "-p", ",".join(map(str, procs))])
    for line in ps.splitlines():
        parts = line.split(None, 3)
        if len(parts) < 4 or int(parts[0]) not in procs:
            continue
        p = procs[int(parts[0])]
        p["elapsed"], p["cpu"] = parts[1], float(parts[2])
        p["kind"] = "desktop app" if "stream-json" in parts[3] else "terminal"
        if "/claude" not in parts[3] and not parts[3].startswith("claude"):
            p["skip"] = True
    return {k: v for k, v in procs.items() if not v.get("skip")}


def transcript_activity(path):
    """Newest Claude transcript for a working directory: last write time and last message."""
    d = Path.home() / ".claude" / "projects" / re.sub(r"[^A-Za-z0-9]", "-", path)
    files = sorted(d.glob("*.jsonl"), key=lambda f: f.stat().st_mtime, reverse=True) if d.is_dir() else []
    if not files:
        return None
    f = files[0]
    info = {"last_write": datetime.fromtimestamp(f.stat().st_mtime, timezone.utc).isoformat(),
            "session": f.stem}
    try:
        with open(f, "rb") as fh:
            fh.seek(max(0, f.stat().st_size - 200_000))
            lines = fh.read().decode("utf-8", "replace").splitlines()
    except OSError:
        return info
    for line in reversed(lines):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        if e.get("type") == "summary" and "title" not in info:
            info["title"] = e.get("summary")
        if e.get("type") != "assistant" or "last" in info:
            continue
        for c in reversed(e.get("message", {}).get("content") or []):
            if c.get("type") == "text" and c.get("text", "").strip():
                info["last"] = c["text"].strip()[:280]
                break
            if c.get("type") == "tool_use":
                inp = c.get("input") or {}
                detail = inp.get("description") or inp.get("command") or inp.get("file_path")
                if detail:
                    info["last"] = f"[{c.get('name')}] {str(detail)[:200]}"
                    break
        if "last" in info:
            break
    return info


def loop_status(tree):
    """scripts/loop/status.sh of the worktree's own branch, parsed into fields."""
    script = Path(tree["path"]) / "scripts" / "loop" / "status.sh"
    if not script.exists():
        return None
    try:
        out = run(["bash", str(script)], cwd=tree["path"], timeout=60)
    except (RuntimeError, subprocess.TimeoutExpired) as e:
        return {"error": str(e)}
    fields = {}
    for line in out.splitlines():
        m = re.match(r"\s+([a-z ]+):\s+(.*)", line)
        if m:
            fields[m.group(1).strip()] = m.group(2).strip()
    return fields


def collect_agents():
    trees = worktrees()
    procs = claude_processes()
    # Longest matching worktree path wins (the main checkout contains all the others).
    for p in procs.values():
        best = None
        for t in trees:
            if p["cwd"] == t["path"] or p["cwd"].startswith(t["path"] + "/"):
                if best is None or len(t["path"]) > len(best["path"]):
                    best = t
        if best:
            best.setdefault("procs", []).append(p)

    with concurrent.futures.ThreadPoolExecutor(8) as pool:
        loops = {t["path"]: pool.submit(loop_status, t) for t in trees if t["bead"]}
        acts = {t["path"]: pool.submit(transcript_activity, t["path"]) for t in trees}
        for t in trees:
            t["loop"] = loops[t["path"]].result() if t["path"] in loops else None
            t["activity"] = acts[t["path"]].result()
            t["procs"] = t.get("procs", [])
    trees.sort(key=lambda t: (not t["procs"], not t["bead"],
                              -(datetime.fromisoformat(t["activity"]["last_write"]).timestamp()
                                if t["activity"] else 0)))
    return trees


# --- pull requests -----------------------------------------------------------------------

PR_FIELDS = ("number,title,state,isDraft,headRefName,url,createdAt,updatedAt,mergedAt,labels,"
             "autoMergeRequest,mergeStateStatus,mergeable,reviewDecision,statusCheckRollup,"
             "additions,deletions,changedFiles")


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
        state = (c.get("conclusion") or c.get("state") or c.get("status") or "").upper()
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


def slim_pr(p):
    m = BEAD_BRANCH.match(p.get("headRefName") or "")
    return {
        "number": p["number"], "title": p["title"], "state": p["state"], "draft": p["isDraft"],
        "branch": p["headRefName"], "url": p["url"], "bead": m.group(1) if m else None,
        "created": p.get("createdAt"), "updated": p.get("updatedAt"), "merged": p.get("mergedAt"),
        "labels": [l["name"] for l in p.get("labels") or []],
        "auto_merge": bool(p.get("autoMergeRequest")),
        "merge_state": p.get("mergeStateStatus"), "mergeable": p.get("mergeable"),
        "review": p.get("reviewDecision") or "",
        "checks": summarize_checks(p.get("statusCheckRollup")),
        "size": f"+{p.get('additions', 0)} −{p.get('deletions', 0)} · {p.get('changedFiles', 0)} files",
    }


def collect_prs():
    open_prs = run_json(["gh", "pr", "list", "--state", "open", "--limit", "50",
                         "--json", PR_FIELDS], cwd=ROOT)
    merged = run_json(["gh", "pr", "list", "--state", "merged", "--limit", "6",
                       "--json", PR_FIELDS], cwd=ROOT)
    return {"open": [slim_pr(p) for p in open_prs], "merged": [slim_pr(p) for p in merged]}


# --- state cache and server --------------------------------------------------------------

STATE = {"generated": None, "beads": None, "agents": None, "prs": None, "errors": {}}
LOCK = threading.Lock()
REFRESH = threading.Event()


def refresh():
    started = time.time()
    sources = {"beads": collect_beads, "agents": collect_agents, "prs": collect_prs}
    with concurrent.futures.ThreadPoolExecutor(3) as pool:
        futures = {k: pool.submit(f) for k, f in sources.items()}
        results, errors = {}, {}
        for k, fut in futures.items():
            try:
                results[k] = fut.result()
            except Exception as e:  # one failing source must not blank the others
                errors[k] = str(e)
    with LOCK:
        STATE.update(results)
        STATE["errors"] = errors
        STATE["generated"] = datetime.now(timezone.utc).isoformat()
        STATE["took"] = round(time.time() - started, 1)
        STATE["root"] = str(ROOT)


def refresher(interval):
    while True:
        try:
            refresh()
        except Exception as e:
            with LOCK:
                STATE["errors"] = {"refresh": str(e)}
        REFRESH.wait(interval)
        REFRESH.clear()


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path.startswith("/api/state"):
            if "refresh=1" in self.path:
                REFRESH.set()
            with LOCK:
                body = json.dumps(STATE).encode()
            self.reply(200, "application/json", body)
        elif self.path in ("/", "/index.html"):
            self.reply(200, "text/html; charset=utf-8", PAGE.encode())
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
:root{--bg:#f6f7f9;--card:#fff;--fg:#1c2128;--mute:#667085;--line:#e3e6ea;--accent:#2f6feb;
--ok:#1a7f37;--warn:#b26b00;--bad:#cf222e;--run:#8250df;--chip:#eef1f4}
@media (prefers-color-scheme:dark){:root{--bg:#0f1216;--card:#171b21;--fg:#e6e9ee;--mute:#8b95a3;
--line:#2a3039;--accent:#5b9bff;--ok:#3fb950;--warn:#d29922;--bad:#f85149;--run:#b083f0;--chip:#232933}}
*{box-sizing:border-box}
body{margin:0;background:var(--bg);color:var(--fg);font:13px/1.45 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}
header{display:flex;align-items:center;gap:12px;padding:12px 18px;border-bottom:1px solid var(--line);position:sticky;top:0;background:var(--bg);z-index:2}
header h1{font-size:15px;margin:0;font-weight:600}
header .meta{color:var(--mute);margin-left:auto;font-size:12px}
button{font:inherit;background:var(--card);color:var(--fg);border:1px solid var(--line);border-radius:6px;padding:3px 10px;cursor:pointer}
main{padding:14px 18px;display:grid;gap:14px;grid-template-columns:repeat(auto-fit,minmax(min(100%,460px),1fr));align-items:start}
section{background:var(--card);border:1px solid var(--line);border-radius:10px;padding:12px 14px;min-width:0}
section.wide{grid-column:1/-1}
h2{font-size:12px;text-transform:uppercase;letter-spacing:.06em;color:var(--mute);margin:0 0 10px;display:flex;gap:8px;align-items:center}
h2 .n{background:var(--chip);color:var(--fg);border-radius:9px;padding:0 7px;font-size:11px;letter-spacing:0}
.kpis{display:flex;flex-wrap:wrap;gap:8px}
.kpi{flex:1 1 90px;background:var(--chip);border-radius:8px;padding:8px 10px}
.kpi b{display:block;font-size:20px;font-variant-numeric:tabular-nums}
.kpi span{color:var(--mute);font-size:11px}
table{width:100%;border-collapse:collapse}
td{padding:5px 6px;border-top:1px solid var(--line);vertical-align:top}
tr:first-child td{border-top:0}
a{color:var(--accent);text-decoration:none} a:hover{text-decoration:underline}
.id{font-family:ui-monospace,SFMono-Regular,Menlo,monospace;font-size:12px;white-space:nowrap}
.mute{color:var(--mute)} .small{font-size:11px}
.chip{display:inline-block;background:var(--chip);border-radius:4px;padding:0 5px;font-size:11px;margin:1px 2px 1px 0;white-space:nowrap}
.p0,.p1{color:var(--bad)} .p2{color:var(--warn)} .p3,.p4{color:var(--mute)}
.dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px;vertical-align:middle}
.ok{color:var(--ok)} .bad{color:var(--bad)} .warn{color:var(--warn)} .run{color:var(--run)}
.bg-ok{background:var(--ok)} .bg-bad{background:var(--bad)} .bg-warn{background:var(--warn)} .bg-run{background:var(--run)} .bg-mute{background:var(--mute)}
.bar{height:6px;background:var(--chip);border-radius:3px;overflow:hidden;min-width:60px}
.bar i{display:block;height:100%;background:var(--ok)}
.agent{border-top:1px solid var(--line);padding:8px 0}
.agent:first-of-type{border-top:0;padding-top:0}
.agent .top{display:flex;gap:8px;align-items:baseline;flex-wrap:wrap}
.agent .last{color:var(--mute);font-size:12px;margin-top:3px;white-space:pre-wrap;overflow-wrap:anywhere;max-height:3.2em;overflow:hidden}
.err{background:color-mix(in srgb,var(--bad) 12%,transparent);color:var(--bad);border-radius:8px;padding:8px 12px;grid-column:1/-1;white-space:pre-wrap}
details summary{cursor:pointer;color:var(--mute);margin-top:6px}
.empty{color:var(--mute);font-style:italic}
</style></head><body>
<header><h1>ArchiCraft · bead loop</h1><span class="meta" id="meta">loading…</span>
<button onclick="load(true)">Refresh</button></header>
<main id="main"></main>
<script>
const $ = s => document.querySelector(s);
const esc = s => String(s ?? "").replace(/[&<>"']/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[c]));
const ago = iso => { if (!iso) return ""; const s = (Date.now() - new Date(iso)) / 1000;
  if (s < 60) return "just now"; if (s < 3600) return Math.round(s/60) + " min ago";
  if (s < 86400) return Math.round(s/3600) + " h ago"; return Math.round(s/86400) + " d ago"; };
const prio = p => `<span class="p${p}">P${p ?? "?"}</span>`;
const chips = (l, skip=[], max=4) => { const v = (l||[]).filter(x => !skip.includes(x));
  return v.slice(0, max).map(x => `<span class="chip">${esc(x)}</span>`).join("") +
    (v.length > max ? `<span class="chip mute" title="${esc(v.slice(max).join(", "))}">+${v.length - max}</span>` : ""); };
const sec = (title, n, body, cls="") => `<section class="${cls}"><h2>${title}${n!=null?` <span class="n">${n}</span>`:""}</h2>${body}</section>`;
const prLink = ref => { const m = /\/pull\/(\d+)/.exec(ref||""); return m ? `<a href="${esc(ref)}" target="_blank">#${m[1]}</a>` : ""; };

function beadRow(b, extra="") {
  return `<tr><td class="id">${esc(b.id)}</td><td>${prio(b.priority)}</td>
    <td>${esc(b.title)}<div>${chips(b.labels,["increment-1","increment-2","increment-3","increment-4"])}</div></td>
    <td class="mute small" style="white-space:nowrap">${extra}</td></tr>`;
}

function prState(p) {
  const c = p.checks;
  let dot = "bg-mute", label = "open";
  if (p.draft) { label = "draft"; }
  else if (c.fail) { dot = "bg-bad"; label = "checks failing"; }
  else if (c.pending) { dot = "bg-run"; label = "checks running"; }
  else if (p.merge_state === "DIRTY" || p.mergeable === "CONFLICTING") { dot = "bg-bad"; label = "merge conflict"; }
  else if (p.merge_state === "BEHIND") { dot = "bg-warn"; label = "behind main"; }
  else if (!p.auto_merge) { dot = "bg-warn"; label = "waiting for you (no auto-merge)"; }
  else { dot = "bg-ok"; label = "green, merging"; }
  return `<span class="dot ${dot}"></span>${label}`;
}

function prRow(p) {
  const c = p.checks;
  const checks = `<span class="ok">${c.pass}✓</span> ${c.fail?`<span class="bad">${c.fail}✗</span> `:""}${c.pending?`<span class="run">${c.pending}…</span> `:""}${c.skipped?`<span class="mute">${c.skipped} skipped</span>`:""}`;
  const detail = [c.failing.length ? `<span class="bad">failing: ${esc(c.failing.join(", "))}</span>` : "",
                  c.running.length ? `<span class="run">running: ${esc(c.running.join(", "))}</span>` : ""].filter(Boolean).join("<br>");
  return `<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td>
    <td>${esc(p.title)}<div class="small mute">${esc(p.branch)} · ${esc(p.size)} · updated ${ago(p.updated)}</div>
      <div>${chips(p.labels)}${p.auto_merge?'<span class="chip ok">auto-merge</span>':''}${p.review?`<span class="chip">${esc(p.review.toLowerCase())}</span>`:''}</div>
      ${detail?`<div class="small">${detail}</div>`:""}</td>
    <td style="white-space:nowrap">${prState(p)}<div class="small">${checks}</div></td></tr>`;
}

function agentBlock(t) {
  const running = t.procs.length > 0;
  const busy = t.procs.some(p => p.cpu > 2);
  const recent = t.activity && (Date.now() - new Date(t.activity.last_write)) < 120000;
  const dot = !running ? "bg-mute" : (busy || recent) ? "bg-run" : "bg-ok";
  const state = !running ? "no agent" : (busy || recent) ? "working" : "idle";
  const L = t.loop || {};
  const gate = L["gate"] ? `<span class="chip ${/passed/.test(L.gate)?"ok":/failed/.test(L.gate)?"bad":""}">gate: ${esc(L.gate)}</span>` : "";
  const review = L["review"] ? `<span class="chip ${/^passed/.test(L.review)?"ok":""}">review: ${esc(L.review.replace(/ on this tree/,""))}</span>` : "";
  const paused = L["paused"] === "yes" ? `<span class="chip bad">paused</span>` : "";
  const dirty = L["uncommitted code"] === "yes" ? `<span class="chip warn">uncommitted code</span>` : "";
  const pr = L["PR"] ? prLink(L["PR"]) : "";
  const procs = t.procs.map(p => `${p.kind}, up ${esc(p.elapsed)}, ${p.cpu}% cpu`).join("; ");
  return `<div class="agent"><div class="top"><span class="dot ${dot}"></span><b>${esc(t.bead || t.name)}</b>
    <span class="mute small">${esc(t.branch)}</span> <span class="small">${state}</span>
    ${/bead-loop/.test(t.branch||"") ? '<span class="chip run">orchestrator</span>' : ""} ${pr}</div>
    <div class="small mute">${esc(t.name)}${procs?` · ${procs}`:""}${t.activity?` · last activity ${ago(t.activity.last_write)}`:""}</div>
    <div>${gate}${review}${dirty}${paused}</div>
    ${t.activity && t.activity.last && running ? `<div class="last">${esc(t.activity.last)}</div>` : ""}</div>`;
}

function render(s) {
  const out = [];
  for (const [k, v] of Object.entries(s.errors || {})) out.push(`<div class="err"><b>${esc(k)}</b>: ${esc(v)}</div>`);
  const B = s.beads, A = s.agents, P = s.prs;

  if (B) {
    const c = B.counts;
    const k = (n, l, cls="") => `<div class="kpi"><b class="${cls}">${n ?? 0}</b><span>${l}</span></div>`;
    out.push(sec("Beads", null, `<div class="kpis">${k(c.in_progress,"in progress","run")}${k(c.ready,"ready","ok")}
      ${k(c.blocked_by_deps,"waiting on deps")}${k(c.open,"open")}${k(c.closed,"closed")}
      ${k(B.human.length,"need you",B.human.length?"bad":"")}</div>
      <div class="small mute" style="margin-top:8px">Open by priority: ${[0,1,2,3,4].map(p=>`${prio(p)} ${B.by_priority[p]}`).join(" · ")}</div>`, "wide"));
  }

  const needs = [];
  if (B) B.human.forEach(b => needs.push(beadRow(b, "label human")));
  if (P) P.open.filter(p => !p.auto_merge && !p.draft).forEach(p => needs.push(`<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td></td><td>${esc(p.title)}</td><td class="mute small">PR waits for your review</td></tr>`));
  if (A) A.filter(t => t.loop && t.loop.paused === "yes").forEach(t => needs.push(`<tr><td class="id">${esc(t.bead)}</td><td></td><td>Session paused in ${esc(t.name)}</td><td class="mute small">paused</td></tr>`));
  if (needs.length) out.push(sec(`<span class="bad">Needs you</span>`, needs.length, `<table>${needs.join("")}</table>`, "wide"));

  if (A) {
    const live = A.filter(t => t.procs.length);
    const idle = A.filter(t => !t.procs.length && t.bead);
    const rest = A.filter(t => !t.procs.length && !t.bead);
    out.push(sec("Agents", live.length,
      (live.length ? live.map(agentBlock).join("") : `<div class="empty">No Claude session is running in this repository.</div>`) +
      (idle.length ? `<details><summary>${idle.length} bead worktrees without an agent</summary>${idle.map(agentBlock).join("")}</details>` : "") +
      (rest.length ? `<details><summary>${rest.length} other worktrees</summary>${rest.map(agentBlock).join("")}</details>` : "")));
  }

  if (P) out.push(sec("Pull requests", P.open.length,
    (P.open.length ? `<table>${P.open.map(prRow).join("")}</table>` : `<div class="empty">No open pull requests.</div>`) +
    (P.merged.length ? `<details><summary>Recently merged</summary><table>${P.merged.map(p =>
      `<tr><td class="id"><a href="${esc(p.url)}" target="_blank">#${p.number}</a></td><td>${esc(p.title)}</td><td class="mute small" style="white-space:nowrap">${ago(p.merged)}</td></tr>`).join("")}</table></details>` : "")));

  if (B) {
    out.push(sec("In progress", B.in_progress.length, B.in_progress.length
      ? `<table>${B.in_progress.map(b => beadRow(b, `${esc(b.assignee||"")}<br>${prLink(b.external_ref)} ${ago(b.updated_at)}`)).join("")}</table>`
      : `<div class="empty">Nothing in progress.</div>`));
    const top = B.ready.slice(0, 15), more = B.ready.slice(15);
    out.push(sec("Ready next", B.ready.length, `<table>${top.map(b => beadRow(b, b.estimated_minutes ? Math.round(b.estimated_minutes/60*10)/10 + " h" : "")).join("")}</table>` +
      (more.length ? `<details><summary>${more.length} more</summary><table>${more.map(b => beadRow(b)).join("")}</table></details>` : "")));
    const active = B.epics.filter(e => e.closed > 0 || e.status === "in_progress");
    const other = B.epics.filter(e => !active.includes(e));
    const epicRow = e => `<tr><td class="id">${esc(e.id)}</td><td>${prio(e.priority)}</td><td>${esc(e.title)}${e.eligible?' <span class="chip ok">ready to close</span>':''}</td>
      <td style="width:130px"><div class="bar"><i style="width:${e.total?100*e.closed/e.total:0}%"></i></div><div class="small mute">${e.closed}/${e.total}</div></td></tr>`;
    out.push(sec("Epics", B.epics.length,
      (active.length ? `<table>${active.map(epicRow).join("")}</table>` : `<div class="empty">No epic has a closed child yet.</div>`) +
      (other.length ? `<details><summary>${other.length} epics not started</summary><table>${other.map(epicRow).join("")}</table></details>` : "")));
    out.push(sec("Recently closed", null, `<table>${B.recently_closed.map(b => beadRow(b, `${prLink(b.external_ref)} ${ago(b.closed_at || b.updated_at)}`)).join("")}</table>`));
  }
  $("#main").innerHTML = out.join("");
  $("#meta").textContent = s.generated ? `updated ${ago(s.generated)} (took ${s.took}s) · ${s.root}` : "collecting…";
}

let last = null;
async function load(force) {
  try {
    const r = await fetch("/api/state" + (force ? "?refresh=1" : ""));
    const s = await r.json();
    if (s.generated !== last || !s.generated) { last = s.generated; render(s); }
    else $("#meta").textContent = `updated ${ago(s.generated)} (took ${s.took}s) · ${s.root}`;
  } catch (e) { $("#meta").textContent = "dashboard server not reachable"; }
}
load(); setInterval(() => load(false), 5000);
</script></body></html>
"""


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--port", type=int, default=8765)
    ap.add_argument("--interval", type=int, default=20, help="seconds between data refreshes")
    ap.add_argument("--no-open", action="store_true", help="do not open a browser")
    args = ap.parse_args()

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
