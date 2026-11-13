#!/usr/bin/env bash
# cancel-stale-ci-builds.sh — shared stale-CI-build cancellation helper.
#
# Invoked as the first step of both PR CI pipelines (infra/ci/cloudbuild-ci-rust.yaml
# and infra/ci/cloudbuild-ci-e2e.yaml) so the two copies cannot drift. Behavioral
# contract: software_architecture/integration-stack/cicd D18 (issue #1328).
#
# Best-effort and fire-and-forget: every failure path warns and exits 0. This
# script must never fail the quality gate, and it does not wait for cancelled
# builds to reach a terminal state.
#
# Build-stream identity is dual-mode: `_PR_NUMBER` when the trigger event
# carried one, else head repo URL + head branch. Live GitHub PR events have
# been observed to deliver an empty `_PR_NUMBER` while `COMMIT_SHA`,
# `_HEAD_BRANCH`, and `_HEAD_REPO_URL` are populated (#1328), so a non-empty
# PR number must never be a precondition for cancellation.
#
# Inputs (environment; `gcloud` is resolved from PATH so guard tests can stub it):
#   G5_CI_EXPECTED_TRIGGER  the invoking pipeline file's own trigger name
#   G5_CI_TRIGGER_NAME      $TRIGGER_NAME of the current build
#   G5_CI_BUILD_ID          $BUILD_ID of the current build
#   G5_CI_COMMIT_SHA        $COMMIT_SHA of the current build
#   G5_CI_PR_NUMBER         ${_PR_NUMBER} (may legitimately be empty)
#   G5_CI_HEAD_BRANCH       ${_HEAD_BRANCH}
#   G5_CI_HEAD_REPO_URL     ${_HEAD_REPO_URL}
#   G5_CI_REGION            Cloud Build region
#   G5_CI_PROJECT           GCP project id

set -euo pipefail

main() {
  local expected_trigger="${G5_CI_EXPECTED_TRIGGER:-}"
  local current_trigger="${G5_CI_TRIGGER_NAME:-}"
  local current_build="${G5_CI_BUILD_ID:-}"
  local current_sha="${G5_CI_COMMIT_SHA:-}"
  local pr_number="${G5_CI_PR_NUMBER:-}"
  local head_branch="${G5_CI_HEAD_BRANCH:-}"
  local head_repo_url="${G5_CI_HEAD_REPO_URL:-}"
  local region="${G5_CI_REGION:-}"
  local project="${G5_CI_PROJECT:-}"

  if [[ -z "${expected_trigger}" || -z "${current_build}" || -z "${current_sha}" \
    || -z "${region}" || -z "${project}" \
    || "${current_trigger}" != "${expected_trigger}" ]]; then
    echo "No PR CI context; skipping stale build cancellation."
    return 0
  fi

  local stream_filter stream_desc
  if [[ -n "${pr_number}" ]]; then
    stream_filter="substitutions._PR_NUMBER=\"${pr_number}\""
    stream_desc="PR #${pr_number}"
  elif [[ -n "${head_branch}" && -n "${head_repo_url}" ]]; then
    # Refuse rather than escape filter metacharacters: a `"` or `\` in a git
    # ref or repo URL is pathological, and a malformed filter could match the
    # wrong build stream.
    if [[ "${head_branch}${head_repo_url}" == *'"'* || "${head_branch}${head_repo_url}" == *'\'* ]]; then
      echo "WARN: head branch/repo contains filter metacharacters; skipping stale build cancellation."
      return 0
    fi
    stream_filter="substitutions._HEAD_REPO_URL=\"${head_repo_url}\" AND substitutions._HEAD_BRANCH=\"${head_branch}\""
    stream_desc="branch ${head_branch} (${head_repo_url})"
  else
    echo "No PR number or head branch/repo in build context; skipping stale build cancellation."
    return 0
  fi

  local current_create
  current_create="$(gcloud builds describe "${current_build}" \
    --region="${region}" \
    --project="${project}" \
    --format='value(createTime)' 2>/dev/null || true)"

  if [[ -z "${current_create}" ]]; then
    echo "WARN: could not resolve current build createTime; skipping stale build cancellation."
    return 0
  fi

  # Single-shot listing: `gcloud builds list` is eventually consistent and may
  # transiently omit a candidate — an accepted best-effort miss, not an error.
  local filter="${stream_filter} AND substitutions.TRIGGER_NAME=\"${current_trigger}\""
  local candidates
  candidates="$(gcloud builds list \
    --ongoing \
    --region="${region}" \
    --project="${project}" \
    --filter="${filter}" \
    --format='value(id,substitutions.COMMIT_SHA,createTime)' 2>/dev/null || true)"

  local -a stale_builds=()
  local build_id_candidate sha_candidate create_candidate
  while IFS=$'\t' read -r build_id_candidate sha_candidate create_candidate; do
    [[ -z "${build_id_candidate:-}" ]] && continue
    [[ "${build_id_candidate}" == "${current_build}" ]] && continue
    [[ "${sha_candidate:-}" == "${current_sha}" ]] && continue

    # Older queued builds can start after newer commits. Only the newer build
    # may cancel older builds, never the reverse. RFC 3339 createTime strings
    # from the same API compare correctly as strings.
    if [[ -z "${create_candidate:-}" || ! "${create_candidate}" < "${current_create}" ]]; then
      echo "Preserving non-stale build ${build_id_candidate} (${create_candidate:-unknown})."
      continue
    fi

    stale_builds+=("${build_id_candidate}")
  done <<< "${candidates}"

  if [[ "${#stale_builds[@]}" -eq 0 ]]; then
    echo "No stale in-progress CI builds found for ${stream_desc}."
    return 0
  fi

  echo "Requesting cancellation for stale ${stream_desc} builds: ${stale_builds[*]}"
  if ! gcloud builds cancel "${stale_builds[@]}" \
    --region="${region}" \
    --project="${project}" \
    --quiet; then
    echo "WARN: stale build cancellation request failed; continuing CI."
  fi
}

main "$@"
