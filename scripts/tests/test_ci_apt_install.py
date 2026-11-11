#!/usr/bin/env python3
"""Tests of scripts/ci-apt-install.sh, which installs Ubuntu packages on a GitHub runner and falls
back to an Ubuntu mirror outside Canonical's DNS when the runner's own mirrors fail or stall
(archicraft-qv0uc: the Flatpak leg of the preview release).

Each test runs the script with stub sudo, timeout and apt-get first on PATH. The apt-get stub
records every call (the step, whether it was given the fallback's source list, the limit that
timeout set) and fails the steps a test names, with exit status 100 as apt-get does or 124 as
timeout does at its limit. Python 3 standard library only:
python3 scripts/tests/test_ci_apt_install.py
"""

import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent.parent / "ci-apt-install.sh"
PACKAGES = ["flatpak", "flatpak-builder", "elfutils", "python3-pil"]
KEYRING = "/usr/share/keyrings/ubuntu-archive-keyring.gpg"
MIRROR = "https://mirrors.edge.kernel.org/ubuntu"

SUDO = """#!/bin/sh
exec "$@"
"""

# timeout [-option...] <seconds> <command...>: runs the command and tells it its limit.
TIMEOUT = """#!/bin/sh
while [ "$#" -gt 0 ]; do
    case "$1" in
        -*) shift ;;
        *) break ;;
    esac
done
STUB_LIMIT="$1"
export STUB_LIMIT
shift
exec "$@"
"""

# A call is <source>-<step>: source "fallback" when it names a source list of its own, else
# "runner"; step "update", "download" (--download-only) or "install".
APT_GET = """#!/usr/bin/env python3
import json, os, sys

args = sys.argv[1:]
options = dict(
    args[i + 1].split("=", 1)
    for i in range(len(args) - 1)
    if args[i] == "-o" and "=" in args[i + 1]
)
source = "fallback" if "Dir::Etc::SourceList" in options else "runner"
step = "update" if "update" in args else "download" if "--download-only" in args else "install"
call = {"call": source + "-" + step, "limit": os.environ.get("STUB_LIMIT"), "args": args}
if source == "fallback":
    with open(options["Dir::Etc::SourceList"]) as listed:
        call["list"] = listed.read().splitlines()
    call["parts"] = sorted(os.listdir(options["Dir::Etc::SourceParts"]))
with open(os.environ["STUB_LOG"], "a") as log:
    log.write(json.dumps(call) + "\\n")
sys.exit(json.loads(os.environ["STUB_FAIL"]).get(call["call"], 0))
"""

FALLBACK_LIST = [
    f"deb [signed-by={KEYRING}] {MIRROR} {suite} main restricted universe multiverse"
    for suite in ("noble", "noble-updates", "noble-security")
]


def run(fail=None, env=None, packages=PACKAGES):
    """Runs the script with the stubs; returns its exit status, its output and the apt-get calls."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        stubs = tmp / "bin"
        stubs.mkdir()
        for name, text in (("sudo", SUDO), ("timeout", TIMEOUT), ("apt-get", APT_GET)):
            (stubs / name).write_text(text)
            (stubs / name).chmod(0o755)
        log = tmp / "calls.jsonl"
        environment = dict(
            os.environ,
            PATH=f"{stubs}{os.pathsep}{os.environ['PATH']}",
            STUB_LOG=str(log),
            STUB_FAIL=json.dumps(fail or {}),
            CODENAME="noble",
        )
        environment.update(env or {})
        result = subprocess.run(
            [str(SCRIPT), *packages], env=environment, capture_output=True, text=True, timeout=60
        )
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
    return result.returncode, result.stdout + result.stderr, calls


def names(calls):
    return [call["call"] for call in calls]


class RunnerMirrors(unittest.TestCase):
    def test_working_runner_mirrors_serve_the_whole_install(self):
        status, output, calls = run()
        self.assertEqual(status, 0, output)
        self.assertEqual(names(calls), ["runner-update", "runner-download", "runner-install"])
        self.assertNotIn("::warning::", output)

    def test_the_update_fails_on_any_index_that_does_not_come(self):
        # Without it apt-get update only warns and the install runs on old lists: on 2026-11-11
        # that took 16 minutes to fail on mirrors that did not resolve.
        _, _, calls = run(fail={"runner-update": 100})
        updates = [call for call in calls if call["call"].endswith("-update")]
        self.assertEqual(len(updates), 2)
        for call in updates:
            self.assertIn("--error-on=any", call["args"])

    def test_every_fetch_has_a_time_limit_and_the_unpacking_none(self):
        _, _, calls = run(fail={"runner-download": 124})
        self.assertEqual(
            [(call["call"], call["limit"]) for call in calls],
            [
                ("runner-update", "120"),
                ("runner-download", "120"),
                ("fallback-update", "300"),
                ("fallback-download", "300"),
                ("fallback-install", None),
            ],
        )

    def test_the_limits_come_from_the_environment(self):
        _, _, calls = run(fail={"runner-update": 124}, env={"FIRST_SECONDS": "7", "FALLBACK_SECONDS": "9"})
        self.assertEqual([call["limit"] for call in calls], ["7", "9", "9", None])


class Fallback(unittest.TestCase):
    def test_unresolvable_runner_mirrors_fall_back_to_the_mirror_outside_canonical(self):
        status, output, calls = run(fail={"runner-update": 100})
        self.assertEqual(status, 0, output)
        self.assertEqual(
            names(calls), ["runner-update", "fallback-update", "fallback-download", "fallback-install"]
        )
        self.assertIn("::warning::", output)
        for call in calls[1:]:
            # Only the mirror, signed by the Ubuntu archive key; none of the runner's sources.
            self.assertEqual(call["list"], FALLBACK_LIST)
            self.assertEqual(call["parts"], [])

    def test_a_download_that_runs_out_of_time_falls_back(self):
        status, output, calls = run(fail={"runner-download": 124})
        self.assertEqual(status, 0, output)
        self.assertEqual(
            names(calls),
            ["runner-update", "runner-download", "fallback-update", "fallback-download", "fallback-install"],
        )

    def test_the_install_fails_when_the_fallback_fails_too(self):
        for fail in ({"runner-update": 100, "fallback-update": 100}, {"runner-update": 124, "fallback-download": 124}):
            with self.subTest(fail=fail):
                status, _, calls = run(fail=fail)
                self.assertNotEqual(status, 0)
                self.assertFalse([name for name in names(calls) if name.endswith("-install")])

    def test_every_package_is_fetched_and_installed_without_recommends(self):
        _, _, calls = run(fail={"runner-update": 100})
        for call in calls:
            if not call["call"].endswith("-update"):
                self.assertIn("--no-install-recommends", call["args"])
                self.assertEqual(call["args"][-len(PACKAGES):], PACKAGES)


class Usage(unittest.TestCase):
    def test_no_package_is_a_usage_error(self):
        status, output, calls = run(packages=[])
        self.assertEqual(status, 2, output)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
