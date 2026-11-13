#!/usr/bin/env bash
# Resolve the local workspace guest image once for the API's organization
# image-policy creation and startup reconciliation.
g5_dev_workspace_image_env() {
  if [[ -n "${G5_NEW_ORG_DEFAULT_IMAGE_TRACK:-}" || -n "${G5_PROMOTED_BASE_IMAGES:-}" ]]; then
    if [[ -z "${G5_NEW_ORG_DEFAULT_IMAGE_TRACK:-}" || -z "${G5_PROMOTED_BASE_IMAGES:-}" ]]; then
      echo "✗ Set both G5_NEW_ORG_DEFAULT_IMAGE_TRACK and G5_PROMOTED_BASE_IMAGES, or neither" >&2
      return 1
    fi
    export G5_NEW_ORG_DEFAULT_IMAGE_TRACK G5_PROMOTED_BASE_IMAGES
    return 0
  fi

  local image_ref="${G5_DEV_WORKSPACE_BASE_IMAGE:-${G5D_HOST_BASE_IMAGE:-mirror.gcr.io/library/buildpack-deps:resolute}}"
  if [[ "$image_ref" != *@sha256:* ]]; then
    if ! docker image inspect "$image_ref" >/dev/null 2>&1; then
      docker pull "$image_ref" >/dev/null
    fi
    image_ref="$(docker image inspect --format '{{range .RepoDigests}}{{println .}}{{end}}' "$image_ref" | sed -n '1p')"
  fi
  if [[ ! "$image_ref" =~ ^[a-z0-9._-]+(:[0-9]+)?(/[a-z0-9._-]+)+@sha256:[0-9a-f]{64}$ ]]; then
    echo "✗ Could not resolve an immutable local workspace image; set G5_DEV_WORKSPACE_BASE_IMAGE to a digest-pinned OCI ref" >&2
    return 1
  fi
  export G5_NEW_ORG_DEFAULT_IMAGE_TRACK=local-dev
  export G5_PROMOTED_BASE_IMAGES="local-dev=${image_ref}"
}

g5_dev_workspace_image_env
