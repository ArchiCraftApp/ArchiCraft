#!/usr/bin/env bash
# Installs Ubuntu packages on a GitHub runner, also while the runner's Ubuntu mirrors are down
# (archicraft-qv0uc). The runner's sources go through /etc/apt/apt-mirrors.txt:
# azure.archive.ubuntu.com, then archive.ubuntu.com and security.ubuntu.com. On 2026-11-11
# Canonical's DNS was down, none of them resolved, and apt-get failed after 16 minutes
# (actions/runner-images #15296); the Azure mirror has also served archives at 60 kB/s for
# minutes at a time (#14823). So the package lists and then the archives come from the runner's
# sources within FIRST_SECONDS each and, when either step fails or runs out of time, from
# https://mirrors.edge.kernel.org/ubuntu alone, an Ubuntu mirror whose name does not resolve
# through Canonical's DNS. apt checks its Release files against the Ubuntu archive keyring as it
# checks the runner's, so the packages are Ubuntu's own either way. Unpacking and configuring them
# has no time limit, so dpkg is never stopped half way. After a fallback the package lists are the
# mirror's until the next apt-get update.
# Usage: scripts/ci-apt-install.sh <package>...
#   FIRST_SECONDS     limit of each step on the runner's sources, in seconds (default 120)
#   FALLBACK_SECONDS  limit of each step on the fallback mirror, in seconds (default 300)
#   CODENAME          the Ubuntu release (default: VERSION_CODENAME of /etc/os-release)
set -euo pipefail
if [ "$#" -eq 0 ]; then
    echo "usage: scripts/ci-apt-install.sh <package>..." >&2
    exit 2
fi
first_seconds="${FIRST_SECONDS:-120}"
fallback_seconds="${FALLBACK_SECONDS:-300}"
mirror=https://mirrors.edge.kernel.org/ubuntu
keyring=/usr/share/keyrings/ubuntu-archive-keyring.gpg
packages=("$@")

# apt_get <seconds, or 0 for no limit> <argument...>: apt-get as root and without questions.
# timeout runs as root too, so at the limit it stops apt-get's download methods as well.
apt_get() {
    local seconds="$1"
    shift
    if [ "$seconds" -gt 0 ]; then
        sudo timeout --kill-after=10 "$seconds" env DEBIAN_FRONTEND=noninteractive apt-get "$@"
    else
        sudo env DEBIAN_FRONTEND=noninteractive apt-get "$@"
    fi
}

# fetch <seconds> [option...]: the package lists, failing on any index that does not come (plain
# apt-get update only warns and leaves the install to the old lists), then the archives of the
# packages and their dependencies, each step within <seconds>.
fetch() {
    local seconds="$1"
    shift
    apt_get "$seconds" update --error-on=any "$@" &&
        apt_get "$seconds" install --yes --no-install-recommends --download-only "$@" "${packages[@]}"
}

if fetch "$first_seconds"; then
    apt_get 0 install --yes --no-install-recommends "${packages[@]}"
    exit 0
fi

echo "::warning::The runner's Ubuntu mirrors failed or took over ${first_seconds} s; installing ${packages[*]} from $mirror instead."
codename="${CODENAME:-$(. /etc/os-release && echo "${VERSION_CODENAME:-}")}"
if [ -z "$codename" ]; then
    echo "ci-apt-install.sh: /etc/os-release names no VERSION_CODENAME" >&2
    exit 1
fi
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir "$dir/parts"
for suite in "$codename" "$codename-updates" "$codename-security"; do
    echo "deb [signed-by=$keyring] $mirror $suite main restricted universe multiverse"
done >"$dir/mirror.list"
# The mirror's list alone: an empty directory instead of /etc/apt/sources.list.d keeps out the
# runner's sources (Canonical's hosts and the third-party repositories) and their timeouts.
set -- -o "Dir::Etc::SourceList=$dir/mirror.list" -o "Dir::Etc::SourceParts=$dir/parts" \
    -o Acquire::Languages=none
fetch "$fallback_seconds" "$@"
apt_get 0 install --yes --no-install-recommends "$@" "${packages[@]}"
