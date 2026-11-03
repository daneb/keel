#!/usr/bin/env bash
#
# Point the cask at a published release:
#
#   packaging/homebrew/update-cask.sh <version> [cask-file] [name]
#
# Reads the two .sha256 files the release workflow uploaded, then rewrites
# the cask's version and checksums in place. The default cask-file is the
# template in this repo; pass a tap checkout's Casks/moor.rb to update that.
# <name> is the project (default moor): the release lives at daneb/<name> and
# its tarballs are <name>-<version>-<arch>-apple-darwin.tar.gz, so the same
# script serves every project in the tap.

set -euo pipefail

version="${1:?usage: update-cask.sh <version> [cask-file] [name]}"
name="${3:-moor}"
cask="${2:-$(dirname "${BASH_SOURCE[0]}")/Casks/$name.rb}"
repo="daneb/$name"

sha_for() {
    curl -fsSL "https://github.com/$repo/releases/download/v$version/$name-$version-$1-apple-darwin.tar.gz.sha256" \
        | awk '{ print $1 }'
}

arm=$(sha_for aarch64)
intel=$(sha_for x86_64)
[[ "$arm" =~ ^[0-9a-f]{64}$ && "$intel" =~ ^[0-9a-f]{64}$ ]] || { echo "bad checksum from release v$version" >&2; exit 1; }

# awk, once per file: BSD and GNU sed disagree about -i.
awk -v v="$version" -v arm="$arm" -v intel="$intel" '
    /^  version "/        { print "  version \"" v "\""; next }
    /^  sha256 arm:/      { print "  sha256 arm:   \"" arm "\","; next }
    /^         intel:/    { print "         intel: \"" intel "\""; next }
    { print }
' "$cask" > "$cask.tmp" && mv "$cask.tmp" "$cask"
echo "$cask -> $version"
