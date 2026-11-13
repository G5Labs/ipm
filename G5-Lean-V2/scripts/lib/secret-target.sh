#!/usr/bin/env bash
# Shared target validation for operator-run secret setup. No cloud calls.

validate_secret_url() {
  # An origin only: HTTPS DNS hostname, no credentials, port, path or query.
  [[ "$EXTERNAL_URL" =~ ^https://([a-z0-9]([a-z0-9-]*[a-z0-9])?\.)+[a-z]([a-z0-9-]*[a-z0-9])?$ ]] || {
    echo "ERROR: --external-url must be an HTTPS hostname without a path, port, query or credentials" >&2
    return 1
  }
}

validate_secret_target() {
  local mode="$1" expected_cluster="" default_url="" environment=""
  [[ "$PROJECT" =~ ^[a-z][a-z0-9-]{4,28}[a-z0-9]$ ]] || {
    echo "ERROR: invalid project ID" >&2; return 1
  }
  case "$PROJECT" in
    g5lean-admin-1ccc)
      echo "ERROR: the artifact-only admin project is not an application secret target" >&2; return 1 ;;
    g5-qa-1ccc) environment=qa; expected_cluster=g5-lean-qa; default_url=https://qa.g5labs.ai ;;
    g5lean-staging-1ccc) environment=staging; expected_cluster=g5-lean-staging ;;
    g5lean-prod-1ccc) environment=prod; expected_cluster=g5-lean-prod ;;
    g5-dev-1ccc) default_url=https://dev.g5labs.ai ;;
  esac
  if [[ "$environment" == staging || "$environment" == prod ]]; then
    [[ "$PROJECT_EXPLICIT" == true && "$EXTERNAL_URL_EXPLICIT" == true ]] || {
      echo "ERROR: staging/prod require explicit --project and --external-url" >&2; return 1
    }
  fi
  if [[ "$mode" == github-app && -z "$expected_cluster" ]]; then
    echo "ERROR: GitHub App setup supports only the reviewed QA, staging and prod projects" >&2
    return 1
  fi
  EXTERNAL_URL="${EXTERNAL_URL:-$default_url}"
  validate_secret_url || return 1
  # Replacement hostnames remain configurable; known other-environment origins
  # and the marketing apex cannot accidentally become a target callback.
  case "$EXTERNAL_URL" in
    https://g5labs.ai|https://www.g5labs.ai)
      [[ -z "$environment" ]] || { echo "ERROR: marketing apex is not an application callback origin" >&2; return 1; } ;;
    https://dev.g5labs.ai) [[ -z "$environment" ]] || { echo "ERROR: wrong environment callback origin" >&2; return 1; } ;;
    https://qa.g5labs.ai) [[ -z "$environment" || "$environment" == qa ]] || { echo "ERROR: wrong environment callback origin" >&2; return 1; } ;;
    https://lean.g5labs.ai) [[ "$environment" == staging ]] || { echo "ERROR: wrong environment callback origin" >&2; return 1; } ;;
    https://app.g5labs.ai) [[ "$environment" == prod ]] || { echo "ERROR: wrong environment callback origin" >&2; return 1; } ;;
  esac
  if [[ "$mode" == github-app ]]; then
    CLUSTER="${CLUSTER:-$expected_cluster}"
    LOCATION="${LOCATION:-us-central1}"
    [[ "$CLUSTER" == "$expected_cluster" && "$LOCATION" == us-central1 ]] || {
      echo "ERROR: cluster/region does not match the reviewed project" >&2; return 1
    }
  elif [[ "$environment" == staging || "$environment" == prod ]]; then
    LICENSE_ADMIN_SECRET_ID="${LICENSE_ADMIN_SECRET_ID:-g5-license-admin-token-$environment}"
    [[ "$LICENSE_ADMIN_SECRET_ID" == "g5-license-admin-token-$environment" ]] || {
      echo "ERROR: license admin secret does not match the target environment" >&2; return 1
    }
  fi
}
