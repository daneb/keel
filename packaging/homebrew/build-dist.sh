#!/usr/bin/env bash
#
# Package one release binary for the Homebrew cask:
#
#   packaging/homebrew/build-dist.sh <rust-target> <out-dir>
#
# Writes <out-dir>/keel-<version>-<arch>-apple-darwin.tar.gz and its .sha256.

set -euo pipefail

target="${1:?usage: build-dist.sh <rust-target> <out-dir>}"
out="${2:?usage: build-dist.sh <rust-target> <out-dir>}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
version=$(awk -F'"' '/^version = /{ print $2; exit }' "$root/Cargo.toml")
arch="${target%%-*}"
name="keel-${version}-${arch}-apple-darwin"

(cd "$root" && cargo build --release --locked --target "$target")

mkdir -p "$out"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT

mkdir -p "$stage/$name"
cp "$root/target/$target/release/keel" "$stage/$name/keel"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$stage/$name/"

tar -C "$stage" -czf "$out/$name.tar.gz" "$name"
(cd "$out" && shasum -a 256 "$name.tar.gz" > "$name.tar.gz.sha256")
echo "$out/$name.tar.gz"
