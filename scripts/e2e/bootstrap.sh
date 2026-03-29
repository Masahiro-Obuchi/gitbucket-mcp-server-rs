#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

validate_saved_token() {
  local token=$1
  local response_file status

  response_file=$(mktemp)
  status=$(curl_json \
    "${response_file}" \
    -H "Accept: application/json" \
    -H "Authorization: token ${token}" \
    "${GITBUCKET_E2E_BASE_URL}/api/v3/user")

  rm -f "${response_file}"
  [[ "${status}" == "200" ]]
}

ensure_validation_user() {
  local create_payload response_file status

  response_file=$(mktemp)
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
      cat "${response_file}" >&2
      rm -f "${response_file}"
      return 1
      ;;
  esac

  create_payload=$(cat <<EOF
{"login":"${GITBUCKET_E2E_USER}","password":"${GITBUCKET_E2E_PASSWORD}","email":"${GITBUCKET_E2E_USER}@example.test","fullName":"${GITBUCKET_E2E_USER}","isAdmin":false,"description":"gitbucket-mcp-server e2e user","url":null}
EOF
)
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
    cat "${response_file}" >&2
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

  cookie_file=$(mktemp)
  page_file=$(mktemp)

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

  response_file=$(mktemp)
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
      cat "${response_file}" >&2
      rm -f "${response_file}"
      return 1
      ;;
  esac

  create_payload=$(cat <<EOF
{"name":"${repo_name}","private":false}
EOF
)
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
    cat "${response_file}" >&2
    rm -f "${response_file}"
    return 1
  fi

  rm -f "${response_file}"
}

write_runtime_env() {
  local token=$1

  cat > "${GITBUCKET_E2E_ENV_FILE}" <<EOF
export GITBUCKET_E2E_URL=${GITBUCKET_E2E_BASE_URL}
export GITBUCKET_E2E_TOKEN=${token}
export GITBUCKET_E2E_OWNER=${GITBUCKET_E2E_REPO_OWNER}
export GITBUCKET_E2E_REPO=${GITBUCKET_E2E_REPO_NAME}
EOF
}

main() {
  local token=""

  ensure_runtime_dir
  compose up -d
  wait_for_gitbucket
  ensure_validation_user

  if [[ -f "${GITBUCKET_E2E_ENV_FILE}" ]]; then
    # shellcheck disable=SC1090
    source "${GITBUCKET_E2E_ENV_FILE}"
    if [[ -n "${GITBUCKET_E2E_TOKEN:-}" ]] && validate_saved_token "${GITBUCKET_E2E_TOKEN}"; then
      token="${GITBUCKET_E2E_TOKEN}"
    fi
  fi

  if [[ -z "${token}" ]]; then
    token=$(create_token_via_web "${GITBUCKET_E2E_USER}" "${GITBUCKET_E2E_PASSWORD}" "gitbucket-mcp-server-e2e")
  fi

  ensure_repo_with_token "${token}" "${GITBUCKET_E2E_REPO}"
  write_runtime_env "${token}"

  echo "Docker GitBucket E2E environment is ready."
  echo "Env file: ${GITBUCKET_E2E_ENV_FILE}"
  echo "Run: source ${GITBUCKET_E2E_ENV_FILE} && cargo test --test e2e_test -- --ignored --nocapture"
}

main "$@"
