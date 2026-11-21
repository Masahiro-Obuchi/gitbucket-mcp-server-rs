#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 || ! "${3:-}" =~ ^[1-9][0-9]*$ ]]; then
  echo "Usage: bash scripts/release/publish_draft.sh <tag ref> <package version> <release ID>" >&2
  exit 2
fi

tag_ref=$1
version=$2
tag=${tag_ref#refs/tags/}
endpoint="repos/${GITHUB_REPOSITORY:?}/releases/$3"
draft=$(gh api "$endpoint" --jq '[.tag_name, .draft] | @tsv')
if [[ "${draft}" != "${tag}"$'\ttrue' ]]; then
  echo "The staged release is not a draft for the expected tag." >&2
  exit 1
fi

commit=$(git rev-parse --verify HEAD)
bash "$(dirname -- "$0")/verify_tag.sh" "$tag_ref" "$version"
gh api --method PATCH "$endpoint" -F draft=false \
  -f "tag_name=$tag" -f "target_commitish=$commit" --jq '.html_url'
