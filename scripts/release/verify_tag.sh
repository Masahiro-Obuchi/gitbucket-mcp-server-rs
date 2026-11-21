#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "Usage: bash scripts/release/verify_tag.sh <tag ref> <package version>" >&2
  exit 2
fi

tag_ref=$1
version=$2
if [[ -z "${version}" || "${tag_ref}" != "refs/tags/v${version}" ]] || ! git check-ref-format "${tag_ref}"; then
  echo "Release ref must be the tag matching the package version." >&2
  exit 1
fi

# Checkout can restore the event's ref locally. Fetch the live tag separately so
# a rerun cannot publish artifacts for a tag that was moved or deleted.
if ! git fetch --no-tags origin \
  '+refs/heads/main:refs/remotes/origin/main' \
  "+${tag_ref}:refs/release/tag"; then
  echo "Could not fetch the release tag and main from origin." >&2
  exit 1
fi

tag_commit=$(git rev-parse --verify 'refs/release/tag^{commit}')
checkout_commit=$(git rev-parse --verify HEAD)
if [[ "${tag_commit}" != "${checkout_commit}" ]]; then
  echo "The tag on origin does not point to the checked-out release commit." >&2
  exit 1
fi

if ! git merge-base --is-ancestor "${tag_commit}" refs/remotes/origin/main; then
  echo "The release commit is outside origin/main history." >&2
  exit 1
fi
