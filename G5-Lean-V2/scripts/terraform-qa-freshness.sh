#!/usr/bin/env bash
# Source with: BUILD_ID PROJECT_ID LOCATION TRIGGER_NAME CONTROL_FILE.
# Reuse the same fail-closed supersession check at root/apply/publication boundaries.
# A skip cancels this stale build's remaining work. It does not prove the newer
# build applied anything: failed/cancelled successors still require forward recovery.
tf_freshness_build_id="$1"
tf_freshness_project="$2"
tf_freshness_location="$3"
tf_freshness_trigger="$4"
tf_freshness_control="$5"
tf_freshness_created=""

# Stop stale builds before further Terraform work or release publication.
skip_if_superseded() {
  if [[ -z "$tf_freshness_created" ]]; then
    tf_freshness_created="$(gcloud builds describe "$tf_freshness_build_id" --project "$tf_freshness_project" \
      --region "$tf_freshness_location" --format='value(createTime)')"
    [[ -n "$tf_freshness_created" ]] || { echo "cannot read this build's createTime"; exit 1; }
  fi
  local newer
  newer="$(gcloud builds list --project "$tf_freshness_project" --region "$tf_freshness_location" \
    --filter="substitutions.TRIGGER_NAME=\"$tf_freshness_trigger\" AND createTime>\"$tf_freshness_created\" AND id!=\"$tf_freshness_build_id\"" \
    --format='value(id,status,substitutions.COMMIT_SHA)' --limit 5)"
  if [[ -n "$newer" ]]; then
    echo "Skipping remaining Terraform roots and release: newer $tf_freshness_trigger builds exist (this build: $tf_freshness_build_id at $tf_freshness_created):"
    echo "$newer"
    echo "This cancels stale work; it does not confirm a successful takeover. Recover failures from current main."
    printf 'export SKIP_TERRAFORM=true\n' > "$tf_freshness_control"
    exit 0
  fi
}
