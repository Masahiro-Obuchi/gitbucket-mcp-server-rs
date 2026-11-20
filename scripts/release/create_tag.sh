#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/release/create_tag.sh [vX.Y.Z[-suffix]]

Examples:
  scripts/release/create_tag.sh
  scripts/release/create_tag.sh vX.Y.Z

When no tag is provided, the script uses v<version from Cargo package>.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "$(git rev-parse --abbrev-ref HEAD)" != "main" ]]; then
  echo "Please run this script on the main branch." >&2
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Working tree is not clean. Commit or stash changes first." >&2
  exit 1
fi

git pull --ff-only origin main
release_commit="$(git rev-parse --verify FETCH_HEAD)"
if [[ "$(git rev-parse HEAD)" != "${release_commit}" ]]; then
  echo "Local main has unpushed commits. Release tags must point to the fetched origin/main." >&2
  exit 1
fi
git fetch --tags origin

pkgid="$(cargo pkgid)"
version="${pkgid##*@}"
expected_tag="v${version}"
tag="${1:-${expected_tag}}"

if [[ ! "${tag}" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.]+)?$ ]]; then
  echo "Tag '${tag}' is not a valid release tag format (vX.Y.Z or vX.Y.Z-suffix)." >&2
  exit 1
fi

if [[ "${tag}" != "${expected_tag}" ]]; then
  echo "Tag '${tag}' does not match Cargo package version '${expected_tag}'." >&2
  exit 1
fi

if git rev-parse -q --verify "refs/tags/${tag}" >/dev/null; then
  echo "Tag '${tag}' already exists locally." >&2
  exit 1
fi

if git ls-remote --tags origin "refs/tags/${tag}" | grep -q .; then
  echo "Tag '${tag}' already exists on origin." >&2
  exit 1
fi

git tag -a "${tag}" "${release_commit}" -m "Release ${tag}"
git push origin "refs/tags/${tag}:refs/tags/${tag}"

echo "Created and pushed ${tag}"
