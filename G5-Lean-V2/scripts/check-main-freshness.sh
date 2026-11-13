#!/usr/bin/env bash
# check-main-freshness — extracted from the infra/ci/cloudbuild-deploy.yaml inline step
# (2026-07-17): the inline script grew past Cloud Build's 10,000-character
# per-arg limit (build 267ee0ea: "build step 24 arg 1 too long"), which
# rejected every downstream `gcloud builds triggers run` promotion. The step
# now exports the Cloud Build substitutions as env and invokes this file.
# Behavior matches the former inline script (yaml $$ unescaped) except the
# intentional [D24] fail-closed approval lookup added on PR #1291 review.
# Requires env: COMMIT_SHA BUILD_ID PROJECT_ID _PROMOTION_MODE _TARGET_PROJECT
#               _GKE_CLUSTER _GKE_ZONE _HELM_NAMESPACE _AR_REGION
# Ontology: software_architecture/infra/cloudbuild_pipelines D5/D24.
set -euo pipefail
# WORKSPACE_DIR exists solely for the hermetic shell regression test
# (test-check-main-freshness.sh); the Cloud Build wrapper never sets it,
# so production always uses /workspace ([I1] cross-step state contract).
WORKSPACE_DIR="${WORKSPACE_DIR:-/workspace}"
source "${WORKSPACE_DIR}/delivery.env"
source "${WORKSPACE_DIR}/image-refs.env"
source "${WORKSPACE_DIR}/deploy-queue.env"

# Production promotions use immutable source provenance and Git ancestry rather
# than QA build timestamps. A delayed approval and same-SHA retry are valid.
# QA retains its existing newest-main queue behavior.
if [[ "${_ENVIRONMENT:-qa}" == "staging" || "${_ENVIRONMENT:-qa}" == "prod" ]]; then
  exec python3 ./scripts/app_release.py freshness
fi

artifact_commit="${COMMIT_SHA}"
current_build="${BUILD_ID}"
current_create="${DEPLOY_QUEUE_BUILD_CREATE_TIME}"

# Ontology [D24]: in promotion (non-build) mode, resolve the target
# environment's currently deployed commit (g5-api Deployment image
# digest -> Artifact Registry commit tag) so the verdict becomes
# "does the artifact ADVANCE the deployment" instead of "is the
# artifact the newest main deploy". Approval latency makes the
# newest-main predicate unsatisfiable under continuous merging (the
# 2026-07 prod freeze) while the advance predicate preserves the
# anti-rollback / migration-ordering guarantee. Resolution retries as
# one unit before giving up: the newest-main fallback is stricter, so
# on an APPROVED build a merely transient GKE/AR read failure would
# convert any normal week (a newer main merge exists) into a red
# wasted approval — under the [D25] weekly release train that burns
# the scheduled approval. A genuinely unresolvable environment (fresh
# cluster, digest without a commit tag) still falls back after the
# attempts are exhausted. g5-api is the digest anchor because it is
# the migration-bearing workload the original stale-promotion
# incident corrupted.
deployed_commit=""
deployed_digest=""
if [[ "${_PROMOTION_MODE}" != "build" && -n "${_GKE_CLUSTER}" ]]; then
  resolve_max_attempts=4
  resolve_retry_sleep=15
  resolve_attempt=1
  while :; do
    set +e
    gcloud container clusters get-credentials "${_GKE_CLUSTER}" \
      --location="${_GKE_ZONE}" \
      --project="${_TARGET_PROJECT}" >/dev/null 2>&1
    deployed_image="$(kubectl get deployment g5-api -n "${_HELM_NAMESPACE}" \
      -o jsonpath='{.spec.template.spec.containers[0].image}' 2>/dev/null)"
    set -e
    deployed_digest="${deployed_image##*@}"
    if [[ "${deployed_digest}" == sha256:* ]]; then
      set +e
      deployed_commit="$(gcloud artifacts docker tags list "${AR}/api" \
        --format='value(tag,version)' 2>/dev/null \
        | awk -v digest="${deployed_digest}" '$2 == digest && $1 ~ /^[0-9a-f]+$/ && length($1) == 40 {print $1; exit}')"
      set -e
    fi
    if [[ -n "${deployed_commit}" ]]; then
      break
    fi
    if [[ "${resolve_attempt}" -ge "${resolve_max_attempts}" ]]; then
      break
    fi
    echo "check-main-freshness: deployed-commit resolution for ${_TARGET_PROJECT} came up empty (attempt ${resolve_attempt}/${resolve_max_attempts}); retrying in ${resolve_retry_sleep}s..." >&2
    resolve_attempt=$((resolve_attempt + 1))
    sleep "${resolve_retry_sleep}"
  done
  if [[ -n "${deployed_commit}" ]]; then
    echo "check-main-freshness: ${_TARGET_PROJECT} currently runs commit ${deployed_commit} (g5-api digest ${deployed_digest})"
  else
    echo "check-main-freshness: could not resolve the deployed commit for ${_TARGET_PROJECT} after ${resolve_max_attempts} attempts; falling back to the newest-main predicate" >&2
  fi
fi

# `gcloud builds list` is eventually consistent: the g5-deploy-qa origin
# build that just finished and triggered THIS promotion can be transiently
# missing from the list response. That previously made check-main-freshness
# fail closed with "Could not find a g5-deploy-qa origin build" on a purely
# transient lag. Retry the list + origin detection a bounded number of times
# before trusting a "no origin" verdict. In build (dev) mode the detector
# never raises, so this loop breaks on the first attempt — behavior there is
# unchanged. The per-fetch verdict itself lives in the checked-in,
# unit-tested scripts/check_deploy_freshness.py ([D5]/[D24]).
freshness_max_attempts=8
freshness_retry_sleep=15
# In legacy-fallback promotion mode (no deployed commit resolved),
# require the "artifact is newest" verdict to repeat across this many
# consecutive list fetches before mutating staging/prod, so a newer
# g5-deploy-qa build still hidden by gcloud builds list eventual
# consistency is not missed on the proceed path (Codex P1 on PR #416).
# The [D24] advance predicate does not need this window: its inputs
# (the deployed commit read from the cluster and origin builds that
# are at least one dev->promotion cycle old) are not subject to the
# just-finished-build lag, so its proceed verdict is accepted
# immediately. A "skip" verdict is always accepted immediately
# because a visible newer build is always real.
freshness_confirm_rounds=3
verdict=""
remote_head=""
verdict_reason=""
deploy_trigger_id=""
freshness_attempt=1
freshness_confirmations=0
while :; do
# A transient `gcloud builds list` failure must be retried, not fatal:
# under the step's `set -e` a bare list call would abort the whole step
# before the retry logic below runs. Disable errexit around the list and
# the detector, capture each status, and only run the detector when
# the list itself succeeded — a list failure is treated as a retryable
# miss just like an unresolved origin.
set +e
# Filter to the only trigger the detector consumes: unfiltered, 200
# builds cover mere HOURS of project volume, so any promotion approved
# late (the release train approves Tuesday's artifact on Thursday by
# design) finds its origin aged out and fails closed. Filter by
# trigger_id, which the Cloud Build API applies server-side; a
# substitutions.TRIGGER_NAME filter makes gcloud page through the
# project's whole build history and filter client-side (114 pages and
# 1.5 minutes by 2026-10-05, growing with every PR push).
if [[ -z "${deploy_trigger_id}" ]]; then
  deploy_trigger_id="$(gcloud builds triggers describe g5-deploy-qa \
    --region="${_AR_REGION}" \
    --project="$PROJECT_ID" \
    --format='value(id)')"
fi
builds_list_status=1
if [[ -n "${deploy_trigger_id}" ]]; then
  gcloud builds list \
    --limit=500 \
    --filter="trigger_id=\"${deploy_trigger_id}\"" \
    --region="${_AR_REGION}" \
    --project="$PROJECT_ID" \
    --format='json(id,createTime,status,substitutions)' \
    > "${WORKSPACE_DIR}/recent-deploy-builds.json"
  builds_list_status=$?
fi
if [[ "${builds_list_status}" -ne 0 ]]; then
  verdict=""
  remote_head=""
  freshness_detect_status=${builds_list_status}
  echo "listing g5-deploy-qa builds failed (trigger id '${deploy_trigger_id}', status=${builds_list_status})" > "${WORKSPACE_DIR}/freshness-detect.err"
else
  detector_args=(
    --builds-json "${WORKSPACE_DIR}/recent-deploy-builds.json"
    --current-build-id "${current_build}"
    --current-create-time "${current_create}"
    --artifact-commit "${artifact_commit}"
    --promotion-mode "${_PROMOTION_MODE}"
  )
  if [[ -n "${deployed_commit}" ]]; then
    detector_args+=(--deployed-commit "${deployed_commit}")
  fi
  verdict_line="$(python3 ./scripts/check_deploy_freshness.py \
    "${detector_args[@]}" 2>"${WORKSPACE_DIR}/freshness-detect.err")"
  freshness_detect_status=$?
  IFS=$'\t' read -r verdict remote_head verdict_reason <<< "${verdict_line}"
fi
set -e

if [[ "${freshness_detect_status}" -eq 0 && -n "${verdict}" && -n "${remote_head}" ]]; then
  # A skip verdict is real — accept it now.
  if [[ "${verdict}" == "SKIP" ]]; then
    break
  fi
  # Proceed verdict. Build/dev mode (the dev deploy is itself the
  # newest signal) and the [D24] advance predicate are accepted
  # immediately; only the legacy newest-main fallback needs the
  # stabilization rounds described above.
  if [[ "${_PROMOTION_MODE}" == "build" || -n "${deployed_commit}" ]]; then
    break
  fi
  freshness_confirmations=$((freshness_confirmations + 1))
  if [[ "${freshness_confirmations}" -ge "${freshness_confirm_rounds}" ]]; then
    break
  fi
  echo "check-main-freshness: artifact ${artifact_commit} currently looks newest; confirming (${freshness_confirmations}/${freshness_confirm_rounds}) against gcloud builds list lag before promoting..." >&2
  sleep "${freshness_retry_sleep}"
  continue
fi

# Detection failed (origin missing or list error): the verdict is not
# stable, so reset any partial proceed-confirmation before retrying.
freshness_confirmations=0

if [[ "${freshness_attempt}" -ge "${freshness_max_attempts}" ]]; then
  echo "check-main-freshness: g5-deploy-qa origin for ${artifact_commit} still unresolved after ${freshness_attempt} attempts (promotion mode=${_PROMOTION_MODE}); gcloud builds list never surfaced it." >&2
  cat "${WORKSPACE_DIR}/freshness-detect.err" >&2 || true
  exit 1
fi

echo "check-main-freshness: g5-deploy-qa origin for ${artifact_commit} not visible yet (attempt ${freshness_attempt}/${freshness_max_attempts}); gcloud builds list is eventually consistent. Retrying in ${freshness_retry_sleep}s..." >&2
freshness_attempt=$((freshness_attempt + 1))
sleep "${freshness_retry_sleep}"
done
skip_deploy=false

if [[ -z "${verdict}" || -z "${remote_head}" ]]; then
  echo "Could not resolve deploy freshness verdict from Cloud Build history"
  exit 1
fi

if [[ "${verdict}" == "SKIP" ]]; then
  skip_deploy=true
  case "${verdict_reason}" in
    already_deployed)
      echo "Skipping deploy and promotion: artifact commit ${artifact_commit} is already deployed on ${_TARGET_PROJECT} ([D24])."
      ;;
    rollback)
      echo "Skipping deploy and promotion: artifact commit ${artifact_commit} would roll back ${_TARGET_PROJECT}, which currently runs the newer commit ${remote_head} ([D24])."
      ;;
    *)
      echo "Skipping deploy and promotion: artifact commit ${artifact_commit} is stale after waiting ${DEPLOY_QUEUE_WAIT_SECONDS}s for the deploy queue; newer main deploy commit is ${remote_head}"
      ;;
  esac
elif [[ "${verdict_reason}" == "advances" || "${verdict_reason}" == "deployed_origin_unlisted" ]]; then
  echo "Artifact commit ${artifact_commit} advances ${_TARGET_PROJECT} (currently ${deployed_commit}; reason=${verdict_reason}) after deploy queue wait of ${DEPLOY_QUEUE_WAIT_SECONDS}s"
else
  echo "Artifact commit is the newest observed main deploy after deploy queue wait of ${DEPLOY_QUEUE_WAIT_SECONDS}s: ${artifact_commit}"
fi

cat > "${WORKSPACE_DIR}/deploy-control.env" <<EOF
export SKIP_DEPLOY=${skip_deploy}
export ARTIFACT_COMMIT=${artifact_commit}
export DEPLOY_FRESHNESS_HEAD=${remote_head}
EOF

# Ontology [D24]: a skip on a build whose manual approval was already
# spent must be a red build, not a green no-op — a silently green
# skipped promotion is what let prod freeze for nine days undetected.
# Auto-triggered (unapproved) skips keep the clean exit-0 idiom ([I3]).
if [[ "${skip_deploy}" == "true" ]]; then
  # Fail CLOSED when the approval-state lookup itself fails (ontology [D24]:
  # a spent approval must never skip silently). The previous `|| true`
  # converted a transient Build API failure into an empty state, letting an
  # actually-approved stale promotion exit green. Retry, then go red if the
  # state cannot be read.
  approval_state=""
  approval_lookup_ok=false
  for _ in 1 2 3 4; do
    if approval_state="$(gcloud builds describe "${current_build}" \
      --region="${_AR_REGION}" \
      --project="$PROJECT_ID" \
      --format='value(approval.state)' 2>/dev/null)"; then
      approval_lookup_ok=true
      break
    fi
    sleep 10
  done
  if [[ "${approval_lookup_ok}" != "true" ]]; then
    echo "ERROR: could not read this build's approval state after 4 attempts; refusing to green-skip a possibly approved promotion ([D24])." >&2
    exit 1
  fi
  if [[ "${approval_state}" == "APPROVED" ]]; then
    echo "ERROR: this build's manual approval was spent on a skipped deploy (reason=${verdict_reason}). Approve the newest pending promotion for ${_TARGET_PROJECT} instead." >&2
    exit 1
  fi
fi
