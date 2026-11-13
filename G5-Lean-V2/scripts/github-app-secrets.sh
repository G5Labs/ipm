#!/usr/bin/env bash
# =============================================================================
# github-app-secrets.sh — configure an environment's G5 GitHub App credentials
# =============================================================================
# The G5 GitHub App (external issues) is configured by seven g5-github-app-*
# Secret Manager containers in the selected project, created by its platform root with a
# blank version. They reach g5-api and g5-daemon through the isolated,
# optional g5-github-app-secrets ExternalSecret. See infra/qa/README.md.
#
# Steps, each a separate command (nothing restarts until `restart`):
#
#   seed PEM_FILE   Prompt for the App values, validate all seven against
#                   g5-api's own rules, show what will be written (lengths
#                   only), and after "yes" add one new version per container.
#                   Writes only Secret Manager versions in the selected project.
#   sync            Refresh the ExternalSecret, wait for a new successful sync,
#                   and check the seven Kubernetes values are all set
#                   (configured) or all blank (unconfigured); a partial set
#                   fails. Writes one annotation on the ExternalSecret without
#                   asking; External Secrets then updates the Kubernetes
#                   Secret, which running pods do not read until they restart.
#   restart         Repeat the check, then after "yes" restart g5-api and
#                   g5-daemon, wait for both rollouts, and confirm
#                   instance/info offers github_app exactly when configured.
#                   Both use the Recreate strategy: the API is down while its
#                   pod restarts.
#   rollback        After "yes", add a blank version to all seven containers
#                   (the App becomes unconfigured). Then run sync and restart.
#
# Add --dry-run to seed or rollback to validate and print without writing.
# No value is printed, logged or passed on a command line: values go to
# `gcloud secrets versions add` on stdin, and checks report lengths only.
# kubectl uses a temporary kubeconfig, so your current context is unchanged.
# =============================================================================
set -euo pipefail
set +x # Credentials must not enter shell tracing, including bash -x invocations.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/secret-target.sh
source "$SCRIPT_DIR/lib/secret-target.sh"

PROJECT="g5-qa-1ccc"
PROJECT_EXPLICIT=false
EXTERNAL_URL=""
EXTERNAL_URL_EXPLICIT=false
CLUSTER=""
LOCATION=""
NAMESPACE="g5"
EXTERNAL_SECRET="g5-github-app-secrets"
SYNC_WAIT_SECONDS="${SYNC_WAIT_SECONDS:-120}"
ROLLOUT_TIMEOUT="${ROLLOUT_TIMEOUT:-600s}"

# Container -> environment setting, in a fixed order.
CONTAINERS=(
  g5-github-app-id
  g5-github-app-slug
  g5-github-app-private-key
  g5-github-app-oauth-client-id
  g5-github-app-oauth-client-secret
  g5-github-app-oauth-callback-url
  g5-github-app-connection-state-secret
)
SETTINGS=(
  G5_GITHUB_APP_ID
  G5_GITHUB_APP_SLUG
  G5_GITHUB_APP_PRIVATE_KEY
  G5_GITHUB_APP_OAUTH_CLIENT_ID
  G5_GITHUB_APP_OAUTH_CLIENT_SECRET
  G5_GITHUB_APP_OAUTH_CALLBACK_URL
  G5_GITHUB_APP_CONNECTION_STATE_SECRET
)

# Holds payload files (mode 700) and the temporary kubeconfig; removed on exit.
WORK_DIR="$(mktemp -d)"
chmod 700 "$WORK_DIR"
trap 'rm -rf "$WORK_DIR"' EXIT

die() { echo "ERROR: $*" >&2; exit 1; }
info() { echo "$*" >&2; }

confirm() {
  local answer
  read -r -p "$1 Type yes to continue: " answer
  [[ "$answer" == "yes" ]] || die "not confirmed; nothing written"
}

# Fail before any write if a container is missing (the platform apply has not
# run) or cannot be read.
require_containers() {
  local container
  for container in "${CONTAINERS[@]}"; do
    gcloud secrets describe "$container" --project="$PROJECT" \
      --format="value(name)" >/dev/null ||
      die "$container does not exist in $PROJECT or cannot be read; nothing written"
  done
}

# Add one version per container. $1 is a directory holding one file per
# container; each file's bytes are the version payload, sent on stdin.
add_versions() {
  local dir="$1" container version added=()
  for container in "${CONTAINERS[@]}"; do
    if ! version="$(gcloud secrets versions add "$container" --project="$PROJECT" \
      --data-file=- --format="value(name)" < "$dir/$container" 2>/dev/null)"; then
      info "Added so far: ${added[*]:-none}. Rerun the same command to add all seven again."
      die "adding a version to $container failed"
    fi
    added+=("$container")
    info "  $container: version ${version##*/}"
  done
}

cmd_seed() {
  local dry_run="$1" pem="${2:-}"
  [[ -n "$pem" ]] || die "usage: $0 seed [--dry-run] PEM_FILE"
  [[ -r "$pem" ]] || die "cannot read $pem"
  openssl pkey -in "$pem" -noout 2>/dev/null ||
    die "$pem is not a readable private key"
  require_containers

  local app_id slug client_id client_secret
  read -r -p "App ID (numeric): " app_id
  read -r -p "App slug: " slug
  read -r -p "Client ID: " client_id
  read -r -s -p "Client secret (hidden): " client_secret
  echo >&2

  # The same rules g5-api applies (GithubAppConfig::resolve); a value it would
  # reject is refused here, before anything is written.
  # g5-api parses the ID as a non-zero u64; compare digits without leading
  # zeros against u64::MAX as strings, since bash arithmetic is signed.
  local id_digits="${app_id#"${app_id%%[!0]*}"}"
  [[ "$app_id" =~ ^[0-9]+$ && -n "$id_digits" ]] &&
    { ((${#id_digits} < 20)) ||
      [[ ${#id_digits} -eq 20 && ! "$id_digits" > "18446744073709551615" ]]; } ||
    die "App ID must be a positive number no larger than 18446744073709551615"
  [[ "$slug" =~ ^[A-Za-z0-9-]+$ ]] || die "App slug may contain only letters, digits and '-'"
  [[ -n "${client_id//[[:space:]]/}" ]] || die "Client ID is empty"
  [[ -n "${client_secret//[[:space:]]/}" ]] || die "Client secret is empty"

  local dir="$WORK_DIR/values"
  mkdir "$dir"
  printf '%s' "$app_id" > "$dir/g5-github-app-id"
  printf '%s' "$slug" > "$dir/g5-github-app-slug"
  cp "$pem" "$dir/g5-github-app-private-key"
  printf '%s' "$client_id" > "$dir/g5-github-app-oauth-client-id"
  printf '%s' "$client_secret" > "$dir/g5-github-app-oauth-client-secret"
  printf '%s' "$CALLBACK_URL" > "$dir/g5-github-app-oauth-callback-url"
  openssl rand -hex 32 | tr -d '\n' > "$dir/g5-github-app-connection-state-secret"
  unset client_secret

  info ""
  info "Project $PROJECT: add one new version to each container (lengths in bytes):"
  local container
  for container in "${CONTAINERS[@]}"; do
    info "  $container: $(wc -c < "$dir/$container" | tr -d ' ')"
  done
  info "The callback URL is $CALLBACK_URL and must match the App's settings."
  if [[ "$dry_run" == true ]]; then
    info "Dry run: nothing written."
    return
  fi
  confirm "Write these seven versions?"
  add_versions "$dir"
  info "Done. Next: $NEXT_COMMAND sync"
}

cmd_rollback() {
  local dry_run="$1"
  require_containers
  info "Project $PROJECT: add a blank version to all seven g5-github-app-* containers."
  info "After sync and restart, the G5 GitHub App is unconfigured (external issues off)."
  if [[ "$dry_run" == true ]]; then
    info "Dry run: nothing written."
    return
  fi
  confirm "Blank all seven?"
  local dir="$WORK_DIR/values" container
  mkdir "$dir"
  for container in "${CONTAINERS[@]}"; do
    printf ' ' > "$dir/$container"
  done
  add_versions "$dir"
  info "Done. Next: $NEXT_COMMAND sync, then $NEXT_COMMAND restart"
}

# kubectl against the validated target only, through a temporary kubeconfig.
kube_setup() {
  export KUBECONFIG="$WORK_DIR/kubeconfig"
  gcloud container clusters get-credentials "$CLUSTER" \
    --location="$LOCATION" --project="$PROJECT" >/dev/null 2>&1 ||
    die "could not get credentials for $CLUSTER"
}

es_field() {
  kubectl -n "$NAMESPACE" get externalsecret "$EXTERNAL_SECRET" -o jsonpath="$1"
}

# Print each setting's trimmed length (a missing key is 0) and set APP_STATE:
# "configured" when all seven are set, "unconfigured" when all are blank (the
# bootstrap or rollback state). A partial set fails: g5-api would leave the
# App off and report the missing settings.
check_values() {
  local names report
  names="$(printf '"%s",' "${SETTINGS[@]}")"
  report="$(kubectl -n "$NAMESPACE" get secret "$EXTERNAL_SECRET" -o json | jq -r "
    .data as \$d | [${names%,}]
    | map(. as \$k | {\$k, n: (\$d[\$k] // \"\" | @base64d | gsub(\"^\\\\s+|\\\\s+\$\"; \"\") | length)})
    | (.[] | \"  \\(.k) \\(.n)\"),
      (if all(.n > 0) then \"configured\" elif all(.n == 0) then \"unconfigured\" else \"partial\" end)")" ||
    die "could not read Secret $NAMESPACE/$EXTERNAL_SECRET"
  echo "${report%$'\n'*}"
  APP_STATE="${report##*$'\n'}"
  case "$APP_STATE" in
    configured) info "All seven set: the App is configured." ;;
    unconfigured) info "All seven blank: the App is unconfigured." ;;
    *) die "some settings are blank (length 0) and others are not; seed all seven and run sync again" ;;
  esac
}

cmd_sync() {
  kube_setup
  es_field '{.metadata.name}' >/dev/null ||
    die "ExternalSecret $NAMESPACE/$EXTERNAL_SECRET not found; has the addons apply run?"
  local before deadline
  before="$(es_field '{.status.refreshTime}')"
  kubectl -n "$NAMESPACE" annotate externalsecret "$EXTERNAL_SECRET" \
    force-sync="$(date +%s)" --overwrite >/dev/null
  deadline=$((SECONDS + SYNC_WAIT_SECONDS))
  until [[ "$(es_field '{.status.refreshTime}')" != "$before" &&
    "$(es_field '{.status.conditions[?(@.type=="Ready")].status}')" == "True" ]]; do
    ((SECONDS < deadline)) ||
      die "no successful refresh within ${SYNC_WAIT_SECONDS}s: $(es_field '{.status.conditions[?(@.type=="Ready")].message}')"
    sleep 2
  done
  info "Synced at $(es_field '{.status.refreshTime}'). Setting lengths:"
  check_values
  info "Next: $NEXT_COMMAND restart"
}

cmd_restart() {
  kube_setup
  info "Setting lengths in $NAMESPACE/$EXTERNAL_SECRET:"
  check_values
  confirm "Restart g5-api and g5-daemon in $CLUSTER with the App $APP_STATE (the API is briefly down)?"
  kubectl -n "$NAMESPACE" rollout restart deployment/g5-api deployment/g5-daemon
  kubectl -n "$NAMESPACE" rollout status deployment/g5-api --timeout="$ROLLOUT_TIMEOUT"
  kubectl -n "$NAMESPACE" rollout status deployment/g5-daemon --timeout="$ROLLOUT_TIMEOUT"
  local features
  features="$(curl -fsS "$INSTANCE_INFO_URL" | jq -c '.data.features')"
  info "instance/info features: $features"
  jq -e 'type == "array"' <<<"$features" >/dev/null ||
    die "instance/info returned no feature list; cannot confirm the App state"
  if [[ "$APP_STATE" == configured ]]; then
    jq -e 'index("github_app")' <<<"$features" >/dev/null ||
      die "github_app is not offered; check: kubectl -n $NAMESPACE logs deployment/g5-api | grep 'GitHub App configuration'"
    info "The G5 GitHub App is configured."
  else
    jq -e 'index("github_app") | not' <<<"$features" >/dev/null ||
      die "github_app is still offered after restarting with blank settings"
    info "The G5 GitHub App is off."
  fi
}

main() {
  local command="" dry_run=false positional=()
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --project|--external-url|--cluster|--region)
        [[ $# -ge 2 && -n "$2" && "$2" != --* ]] || die "$1 requires a value"
        case "$1" in
          --project) PROJECT="$2"; PROJECT_EXPLICIT=true ;;
          --external-url) EXTERNAL_URL="${2%/}"; EXTERNAL_URL_EXPLICIT=true ;;
          --cluster) CLUSTER="$2" ;;
          --region) LOCATION="$2" ;;
        esac
        shift 2 ;;
      --dry-run) dry_run=true; shift ;;
      --help|-h)
        echo "Usage: $0 [--project ID --external-url https://HOST] [--cluster NAME --region us-central1] seed [--dry-run] PEM_FILE | sync | restart | rollback [--dry-run]"
        echo "Defaults to QA. Staging/prod require explicit project and URL. Only seed/rollback support dry-run."
        exit 0 ;;
      --*) die "unknown option: $1" ;;
      *) positional+=("$1"); shift ;;
    esac
  done
  command="${positional[0]:-}"
  case "$command" in
    seed) [[ ${#positional[@]} -eq 2 ]] || die "seed requires exactly one PEM_FILE" ;;
    sync|restart|rollback) [[ ${#positional[@]} -eq 1 ]] || die "unexpected command arguments" ;;
    *) die "expected seed, sync, restart or rollback (see --help)" ;;
  esac
  [[ "$dry_run" != true || "$command" == seed || "$command" == rollback ]] || die "--dry-run is supported only for seed/rollback"
  validate_secret_target github-app
  CALLBACK_URL="$EXTERNAL_URL/api/v1/github-app/oauth/callback"
  INSTANCE_INFO_URL="$EXTERNAL_URL/api/v1/instance/info"
  printf -v NEXT_COMMAND '%q ' "$0" --project "$PROJECT" --external-url "$EXTERNAL_URL" --cluster "$CLUSTER" --region "$LOCATION"
  case "$command" in
    seed) cmd_seed "$dry_run" "${positional[1]}" ;;
    sync) cmd_sync ;;
    restart) cmd_restart ;;
    rollback) cmd_rollback "$dry_run" ;;
    *) die "usage: $0 seed [--dry-run] PEM_FILE | sync | restart | rollback [--dry-run]" ;;
  esac
}

main "$@"
