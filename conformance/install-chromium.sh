#!/usr/bin/env sh
# Installs Playwright's Chromium for run.sh on a Linux CI runner, and its system libraries only
# when the runner image lacks one (archicraft-1er5). `npx playwright install-deps chromium` ran
# apt on every run: the ubuntu-24.04 image has every library Chromium links, so it only upgraded
# a few and added fonts, 26 MB from an Ubuntu mirror that served a package a minute on 2026-10-22
# and so cancelled the job. A launch shows whether anything is missing: without one of its
# libraries Chromium does not start (the loader names it; Playwright 1.63's own host check looks
# for chrome-linux/, which the headless shell no longer has). Only when the launch fails does
# this install Playwright's list of packages, with apt retries and stall timeouts, in at most two
# attempts of INSTALL_SECONDS each, and launch again; a launch that still fails fails the script.
# Usage: conformance/install-chromium.sh
set -eu
cd "$(dirname "$0")"
INSTALL_SECONDS=90

# A no-op when the cache restored Chromium; otherwise a download from Playwright's CDN.
npx playwright install chromium

# At most 30 s, so two launches, two install attempts of at most INSTALL_SECONDS + 10 and the
# download fit in the CI step's 5 minutes.
launches() {
    timeout 30 node --input-type=module -e '
import { chromium } from "@playwright/test";
const browser = await chromium.launch({ timeout: 20000 });
await browser.close();
'
}

if launches; then
    echo "Chromium starts: the runner image has every system library it needs."
    exit 0
fi

echo "::warning::Chromium does not start on this runner image; installing its system libraries."
# Read by apt-get under Playwright's install-deps, before /etc/apt/apt.conf.d: retry a failed
# download, give up on a stalled one after 20 s instead of 120, wait for a held dpkg lock.
apt_config="$(mktemp)"
cat >"$apt_config" <<'EOF'
Acquire::Retries "3";
Acquire::http::Timeout "20";
Acquire::https::Timeout "20";
DPkg::Lock::Timeout "60";
EOF
for attempt in 1 2; do
    # timeout runs as root, so at the limit it stops apt-get and dpkg too, not just Playwright.
    if sudo env "PATH=$PATH" "APT_CONFIG=$apt_config" timeout --kill-after=10 "$INSTALL_SECONDS" \
        node_modules/.bin/playwright install-deps chromium; then
        break
    fi
    echo "Attempt $attempt to install Chromium's system libraries failed or took over ${INSTALL_SECONDS} s."
    # Finish what a stopped dpkg left half configured, or the next apt-get refuses to run.
    sudo dpkg --configure -a || true
done
launches
