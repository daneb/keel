#!/usr/bin/env bash
#
# Release keel.
#
#   ./release.sh <version>            # run it
#   ./release.sh <version> --dry-run  # say what it would do, change nothing
#
# The script is a gate, not a wizard. It refuses on the first failed
# precondition and tells you which one, because a release that needed a human
# to notice something is a release that will eventually go out unnoticed.
#
# Release notes come from the matching CHANGELOG.md section. There is no
# interactive notes prompt: keel's own runs are frequently piped, and a `cat`
# waiting on stdin silently eats whatever the caller piped in.

set -euo pipefail

readonly REPO="daneb/keel"
readonly EXPECTED_BRANCH="master"

# ---------------------------------------------------------------- presentation

if [ -t 1 ]; then
    readonly C_RED=$'\033[0;31m' C_GRN=$'\033[0;32m' C_YEL=$'\033[0;33m'
    readonly C_DIM=$'\033[2m' C_BLD=$'\033[1m' C_OFF=$'\033[0m'
else
    readonly C_RED='' C_GRN='' C_YEL='' C_DIM='' C_BLD='' C_OFF=''
fi

step()  { printf '\n%s▶ %s%s\n' "$C_BLD" "$1" "$C_OFF"; }
ok()    { printf '  %s✓%s %s\n' "$C_GRN" "$C_OFF" "$1"; }
note()  { printf '  %s·%s %s\n' "$C_DIM" "$C_OFF" "$1"; }
warn()  { printf '  %s!%s %s\n' "$C_YEL" "$C_OFF" "$1"; }
die()   { printf '\n%s✗ %s%s\n\n' "$C_RED" "$1" "$C_OFF" >&2; exit 1; }

# ------------------------------------------------------------------- arguments

VERSION=""
DRY_RUN=false

while [ $# -gt 0 ]; do
    case "$1" in
        --dry-run) DRY_RUN=true ;;
        -h|--help) sed -n '3,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*)        die "unknown flag: $1" ;;
        *)         [ -n "$VERSION" ] && die "version given twice: $VERSION and $1"
                   VERSION="$1" ;;
    esac
    shift
done

[ -n "$VERSION" ] || die "usage: ./release.sh <version> [--dry-run]"

# Semver, because the tag is a public API and "v1.2" sorts badly forever.
[[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
    || die "not a semver version: $VERSION"

readonly VERSION DRY_RUN
readonly TAG="v${VERSION}"

run() {
    if $DRY_RUN; then
        printf '  %swould run:%s %s\n' "$C_DIM" "$C_OFF" "$*"
    else
        "$@"
    fi
}

printf '%skeel release %s%s\n' "$C_BLD" "$TAG" "$C_OFF"
$DRY_RUN && printf '%sdry run — nothing will be changed%s\n' "$C_YEL" "$C_OFF"

# ---------------------------------------------------------------- preconditions

step "Preconditions"

[ -d .git ] && [ -f Cargo.toml ] || die "run this from the root of the keel repository"

branch=$(git rev-parse --abbrev-ref HEAD)
[ "$branch" = "$EXPECTED_BRANCH" ] \
    || die "on branch '$branch', expected '$EXPECTED_BRANCH'"
ok "on $EXPECTED_BRANCH"

# filter-branch and friends leave the index stat cache stale, which makes
# diff-index report a clean tree as dirty. Refresh before believing it.
git update-index --refresh >/dev/null 2>&1 || true
git diff-index --quiet HEAD -- || die "working tree has uncommitted changes"
ok "working tree clean"

if git rev-parse "$TAG" >/dev/null 2>&1; then
    die "tag $TAG already exists locally"
fi
if git ls-remote --tags origin 2>/dev/null | grep -q "refs/tags/${TAG}$"; then
    die "tag $TAG already exists on origin"
fi
ok "$TAG is free"

# The author of a release commit is part of the public record. keel's history
# has been rewritten once already to correct this; warn rather than repeat it.
cfg_email=$(git config user.email || echo "")
last_email=$(git log -1 --format='%ae')
if [ "$cfg_email" != "$last_email" ]; then
    warn "git config user.email is '$cfg_email' but HEAD was authored by '$last_email'"
    warn "the release commit will use '$cfg_email' — fix it now if that is wrong"
fi
ok "authoring as ${cfg_email:-<unset>}"

command -v gh >/dev/null 2>&1 || warn "gh not installed — the GitHub release step will be skipped"

# ----------------------------------------------------------------- the changelog

step "Release notes"

[ -f CHANGELOG.md ] || die "CHANGELOG.md not found"

NOTES=$(mktemp)
trap 'rm -f "$NOTES"' EXIT

# Pull the section for this version out of a Keep a Changelog file: everything
# between this version's heading and the next one.
awk -v ver="$VERSION" '
    $0 ~ "^## \\[" ver "\\]" { grab = 1; next }
    grab && /^## \[/         { exit }
    grab                     { print }
' CHANGELOG.md | sed -e '/./,$!d' > "$NOTES"

[ -s "$NOTES" ] || die "CHANGELOG.md has no '## [$VERSION]' section — write it first"
ok "$(wc -l < "$NOTES" | tr -d ' ') lines from CHANGELOG.md"

# --------------------------------------------------------------------- the gates

step "Gates"

if $DRY_RUN; then
    note "would run: cargo clippy -D warnings, cargo test (fmt reported, not enforced)"
else
    # Deliberately not a blocking check. keel's own gate config (.keel/keel.toml)
    # declares build, lint and test — not fmt. This codebase is hand-formatted in
    # a compact style rustfmt does not reproduce, and reformatting 82 of 84 files
    # to satisfy a standard the project never adopted is exactly the wide blast
    # radius the house rules warn about. Report it; do not enforce it.
    if ! cargo fmt --all -- --check >/dev/null 2>&1; then
        n=$(cargo fmt --all -- --check 2>/dev/null | grep -c '^Diff in' || true)
        note "rustfmt would change $n hunks — not enforced (see .keel/keel.toml)"
    else
        ok "formatted"
    fi

    cargo clippy --all-targets --quiet -- -D warnings 2>&1 | tail -5
    ok "no clippy warnings"

    test_out=$(cargo test --quiet 2>&1) || { echo "$test_out" | tail -30; die "tests failed"; }
    passed=$(echo "$test_out" | awk '/^test result: ok/ { n += $4 } END { print n+0 }')
    [ "$passed" -gt 0 ] || die "no tests ran — that is a failure, not a pass"
    ok "$passed tests passed"
fi

# ---------------------------------------------------------------- version bump

step "Version"

current=$(awk -F'"' '/^version = /{ print $2; exit }' Cargo.toml)
if [ "$current" = "$VERSION" ]; then
    note "Cargo.toml already at $VERSION"
else
    note "$current → $VERSION"
    if ! $DRY_RUN; then
        # BSD and GNU sed disagree about -i. Do it in awk instead, once.
        awk -v v="$VERSION" '
            !done && /^version = / { print "version = \"" v "\""; done = 1; next }
            { print }
        ' Cargo.toml > Cargo.toml.tmp && mv Cargo.toml.tmp Cargo.toml

        got=$(awk -F'"' '/^version = /{ print $2; exit }' Cargo.toml)
        [ "$got" = "$VERSION" ] || die "failed to write version into Cargo.toml"

        cargo update --workspace --quiet 2>/dev/null || cargo check --quiet >/dev/null 2>&1 || true
        ok "Cargo.toml and Cargo.lock at $VERSION"
    fi
fi

if ! $DRY_RUN && ! git diff --quiet -- Cargo.toml Cargo.lock; then
    run git add Cargo.toml Cargo.lock
    run git commit -q -m "Release ${TAG}"
    ok "committed"
fi

# ----------------------------------------------------------------------- the tag

step "Tag"
run git tag -a "$TAG" -F "$NOTES"
ok "$TAG created"

# ---------------------------------------------------------------------- push

step "Push"
run git push origin "$EXPECTED_BRANCH"
run git push origin "$TAG"
ok "pushed to origin"

# ------------------------------------------------------------- GitHub release

step "GitHub release"
if ! command -v gh >/dev/null 2>&1; then
    warn "gh not installed — create it at https://github.com/${REPO}/releases/new?tag=${TAG}"
elif ! $DRY_RUN && gh release view "$TAG" --repo "$REPO" >/dev/null 2>&1; then
    note "release $TAG already exists"
else
    # --repo is not optional: keel's remote may be an ssh alias that gh cannot
    # resolve back to a GitHub host.
    run gh release create "$TAG" --repo "$REPO" --title "$TAG" --notes-file "$NOTES"
    ok "https://github.com/${REPO}/releases/tag/${TAG}"
fi

# ------------------------------------------------------------------ crates.io

step "crates.io"
note "not published by this script — 'cargo publish' needs a token this script"
note "will not handle. Run 'cargo login' yourself, then: cargo publish"

printf '\n%s%s released.%s\n\n' "$C_GRN" "$TAG" "$C_OFF"
