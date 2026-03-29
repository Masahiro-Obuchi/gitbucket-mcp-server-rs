#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMPOSE_FILE="${REPO_ROOT}/docker/e2e/compose.yaml"
COMPOSE_PROJECT_NAME="${GITBUCKET_E2E_COMPOSE_PROJECT:-gitbucket-mcp-e2e}"
GITBUCKET_E2E_ROOT="${GITBUCKET_E2E_ROOT:-${REPO_ROOT}/.tmp/e2e}"
GITBUCKET_E2E_HTTP_PORT="${GITBUCKET_E2E_HTTP_PORT:-18080}"
GITBUCKET_E2E_BASE_URL="${GITBUCKET_E2E_BASE_URL:-http://127.0.0.1:${GITBUCKET_E2E_HTTP_PORT}}"
GITBUCKET_E2E_ENV_FILE="${GITBUCKET_E2E_ENV_FILE:-${GITBUCKET_E2E_ROOT}/runtime.env}"
GITBUCKET_E2E_USER="${GITBUCKET_E2E_USER:-gb-mcp-e2e-user}"
GITBUCKET_E2E_PASSWORD="${GITBUCKET_E2E_PASSWORD:-gb-mcp-e2e-pass}"
GITBUCKET_E2E_REPO_OWNER="${GITBUCKET_E2E_REPO_OWNER:-${GITBUCKET_E2E_USER}}"
GITBUCKET_E2E_REPO_NAME="${GITBUCKET_E2E_REPO_NAME:-gitbucket-mcp-e2e}"
GITBUCKET_E2E_REPO="${GITBUCKET_E2E_REPO_OWNER}/${GITBUCKET_E2E_REPO_NAME}"

export REPO_ROOT
export COMPOSE_FILE
export COMPOSE_PROJECT_NAME
export GITBUCKET_E2E_ROOT
export GITBUCKET_E2E_HTTP_PORT
export GITBUCKET_E2E_BASE_URL
export GITBUCKET_E2E_ENV_FILE
export GITBUCKET_E2E_USER
export GITBUCKET_E2E_PASSWORD
export GITBUCKET_E2E_REPO_OWNER
export GITBUCKET_E2E_REPO_NAME
export GITBUCKET_E2E_REPO

compose() {
  docker compose -p "${COMPOSE_PROJECT_NAME}" -f "${COMPOSE_FILE}" "$@"
}

ensure_runtime_dir() {
  mkdir -p "${GITBUCKET_E2E_ROOT}"
}

wait_for_gitbucket() {
  local attempt

  for attempt in $(seq 1 90); do
    if curl -fsS "${GITBUCKET_E2E_BASE_URL}/signin" >/dev/null 2>&1 || \
      curl -fsS "${GITBUCKET_E2E_BASE_URL}/" >/dev/null 2>&1; then
      return 0
    fi
    sleep 2
  done

  echo "GitBucket did not become ready at ${GITBUCKET_E2E_BASE_URL}" >&2
  return 1
}

curl_json() {
  local output_file=$1
  shift
  curl -sS -o "${output_file}" -w "%{http_code}" "$@"
}

extract_html_clipboard_token() {
  local file=$1
  python3 -c 'import pathlib,re,sys; body=pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"); match=re.search(r"data-clipboard-text=\"([0-9a-f]{40})\"", body); print(match.group(1) if match else "")' "${file}"
}
