#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

validate_saved_token() {
  local token=$1
  local response_file status valid=1

  response_file=$(mktemp "${runtime_scratch}/file.XXXXXXXX")
  status=$(curl_json \
    "${response_file}" \
    -H "Accept: application/json" \
    -H "Authorization: token ${token}" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/user")

  if [[ "${status}" == "200" ]] && python3 - "${response_file}" "${GITBUCKET_E2E_USER}" <<'PY'
import json
import sys

try:
    with open(sys.argv[1], encoding="utf-8") as response:
        data = json.load(response)
    valid = isinstance(data, dict) and data.get("login") == sys.argv[2]
except (OSError, ValueError):
    valid = False
sys.exit(0 if valid else 1)
PY
  then
    valid=0
  fi
  rm -f "${response_file}"
  return "${valid}"
}

ensure_validation_user() {
  local create_payload response_file status

  response_file=$(mktemp "${runtime_scratch}/file.XXXXXXXX")
  status=$(curl_json \
    "${response_file}" \
    -u "root:root" \
    -H "Accept: application/json" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/users/${GITBUCKET_E2E_USER}")

  case "${status}" in
    200)
      rm -f "${response_file}"
      return 0
      ;;
    404)
      ;;
    *)
      echo "failed to check validation user ${GITBUCKET_E2E_USER} (HTTP ${status})" >&2
      rm -f "${response_file}"
      return 1
      ;;
  esac

  create_payload=$(python3 -c 'import json,os; u=os.environ["GITBUCKET_E2E_USER"]; print(json.dumps(dict(login=u,password=os.environ["GITBUCKET_E2E_PASSWORD"],email=u+"@example.test",fullName=u,isAdmin=False,description="gitbucket-mcp-server e2e user",url=None)))')

  status=$(curl_json \
    "${response_file}" \
    -u "root:root" \
    -H "Accept: application/json" \
    -H "Content-Type: application/json" \
    -X POST \
    -d "${create_payload}" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/admin/users")

  if [[ "${status}" != "200" && "${status}" != "201" ]]; then
    echo "failed to create validation user ${GITBUCKET_E2E_USER} (HTTP ${status})" >&2
    rm -f "${response_file}"
    return 1
  fi

  rm -f "${response_file}"
}

create_token_via_web() {
  local user=$1
  local password=$2
  local note=$3
  local cookie_file page_file token

  cookie_file=$(mktemp "${runtime_scratch}/file.XXXXXXXX")
  page_file=$(mktemp "${runtime_scratch}/file.XXXXXXXX")

  curl -sS -L \
    -c "${cookie_file}" \
    -b "${cookie_file}" \
    --data-urlencode "userName=${user}" \
    --data-urlencode "password=${password}" \
    --data-urlencode "hash=" \
    -o /dev/null \
    "${GITBUCKET_E2E_BASE_URL}/signin"

  curl -sS -L \
    -c "${cookie_file}" \
    -b "${cookie_file}" \
    --data-urlencode "note=${note}" \
    -o "${page_file}" \
    "${GITBUCKET_E2E_BASE_URL}/${user}/_personalToken"

  token=$(extract_html_clipboard_token "${page_file}")
  rm -f "${cookie_file}" "${page_file}"

  if [[ -z "${token}" ]]; then
    echo "failed to extract token from GitBucket application page for ${user}" >&2
    return 1
  fi

  printf '%s\n' "${token}"
}

ensure_repo_with_token() {
  local token=$1
  local repo_full_name=$2
  local repo_name=${repo_full_name#*/}
  local create_payload response_file status

  response_file=$(mktemp "${runtime_scratch}/file.XXXXXXXX")
  status=$(curl_json \
    "${response_file}" \
    -H "Accept: application/json" \
    -H "Authorization: token ${token}" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/repos/${repo_full_name}")

  case "${status}" in
    200)
      rm -f "${response_file}"
      return 0
      ;;
    404)
      ;;
    *)
      echo "failed to check repo ${repo_full_name} (HTTP ${status})" >&2
      rm -f "${response_file}"
      return 1
      ;;
  esac

  create_payload=$(printf '%s' "${repo_name}" | python3 -c 'import json,sys; print(json.dumps(dict(name=sys.stdin.read(),private=False,auto_init=True)))')

  status=$(curl_json \
    "${response_file}" \
    -H "Accept: application/json" \
    -H "Authorization: token ${token}" \
    -H "Content-Type: application/json" \
    -X POST \
    -d "${create_payload}" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/user/repos")

  if [[ "${status}" != "200" && "${status}" != "201" ]]; then
    echo "failed to create repo ${repo_full_name} (HTTP ${status})" >&2
    rm -f "${response_file}"
    return 1
  fi

  rm -f "${response_file}"
}

write_runtime_env() {
  printf '%s' "$1" | python3 "${SCRIPT_DIR}/runtime.py" write
}

main() (
  local token=""
  local runtime_scratch

  ensure_runtime_dir
  runtime_scratch=$(mktemp -d "${GITBUCKET_E2E_ROOT}/bootstrap.XXXXXXXX")
  trap 'rm -rf -- "${runtime_scratch}"' EXIT
  compose up -d
  wait_for_gitbucket
  ensure_validation_user

  if [[ -f "${GITBUCKET_E2E_ENV_FILE}.json" ]]; then
    token=$(python3 "${SCRIPT_DIR}/runtime.py" token)
    if ! validate_saved_token "${token}"; then token=""; fi
  fi

  if [[ -z "${token}" ]]; then
    token=$(create_token_via_web "${GITBUCKET_E2E_USER}" "${GITBUCKET_E2E_PASSWORD}" "gitbucket-mcp-server-e2e")
  fi

  ensure_repo_with_token "${token}" "${GITBUCKET_E2E_REPO}"
  write_runtime_env "${token}"

  echo "Docker GitBucket E2E environment is ready."
  echo "Env file: ${GITBUCKET_E2E_ENV_FILE}"
  printf 'Run: source %q && cargo test --test e2e_test -- --ignored --nocapture\n' "${GITBUCKET_E2E_ENV_FILE}"
)

main "$@"
