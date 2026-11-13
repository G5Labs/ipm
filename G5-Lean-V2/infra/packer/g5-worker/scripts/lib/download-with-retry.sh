#!/usr/bin/env bash
# =============================================================================
# download-with-retry.sh — shared bounded-retry download helper for the worker
# image provisioner chain.
#
# Sourced, never executed: the packer build uploads this file once (a `file`
# provisioner ahead of Phase 1) and every provisioner script that fetches an
# upstream artifact sources it from $G5_DOWNLOAD_LIB. It is deliberately NOT
# named with the NN- prefix the chain runs positionally.
#
# Why this exists: the worker image pulls eleven artifacts from six external
# hosts (github.com, nodejs.org, static.rust-lang.org, dl.k8s.io,
# storage.googleapis.com, archive.ubuntu.com), and every one of them was a
# single-shot curl. The 2026-08-12 GitHub release-serving outage failed the
# image build at 55-install-gh.sh with curl exit 56 (G5Labs/G5#1811). Each
# caller verifies a pinned sha256 after the fetch, so retrying is
# supply-chain-safe.
#
# Retry is bounded and fails closed. It covers transient upstream failure; a
# sustained outage still fails the build, loudly, rather than baking a
# half-provisioned image.
# =============================================================================

download_http_status_retryable() {
    case "$1" in
        408|429|5??) return 0 ;;
        *) return 1 ;;
    esac
}

download_curl_status_retryable() {
    local status="$1"
    local http_status="${2:-000}"
    case "$status" in
        5|6|7|18|28|35|52|55|56|92) return 0 ;;
        22) download_http_status_retryable "$http_status" ;;
        *) return 1 ;;
    esac
}

download_require_positive_integer() {
    local name="$1"
    local value="$2"
    case "$value" in
        ""|*[!0-9]*|0)
            echo "[g5-worker] FATAL: ${name} must be a positive integer (got: ${value:-<empty>})" >&2
            return 2
            ;;
    esac
}

download_with_retry() {
    local label="$1"
    local url="$2"
    local output="$3"
    local attempts="${G5_DOWNLOAD_RETRY_ATTEMPTS:-8}"
    local delay="${G5_DOWNLOAD_RETRY_DELAY_SECONDS:-2}"
    local max_delay="${G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS:-30}"
    local connect_timeout="${G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS:-10}"
    local max_time="${G5_DOWNLOAD_MAX_TIME_SECONDS:-300}"
    local attempt=1
    local status=0
    local http_status=""
    local tmp="${output}.tmp"

    download_require_positive_integer G5_DOWNLOAD_RETRY_ATTEMPTS "$attempts" || return 2
    download_require_positive_integer G5_DOWNLOAD_RETRY_DELAY_SECONDS "$delay" || return 2
    download_require_positive_integer G5_DOWNLOAD_RETRY_MAX_DELAY_SECONDS "$max_delay" || return 2
    download_require_positive_integer G5_DOWNLOAD_CONNECT_TIMEOUT_SECONDS "$connect_timeout" || return 2
    download_require_positive_integer G5_DOWNLOAD_MAX_TIME_SECONDS "$max_time" || return 2

    rm -f "$tmp"
    while [ "$attempt" -le "$attempts" ]; do
        echo "[g5-worker] downloading ${label} attempt ${attempt}/${attempts}: ${url}"
        if http_status="$(curl -fsSL --connect-timeout "$connect_timeout" --max-time "$max_time" -w "%{http_code}" "$url" -o "$tmp")"; then
            mv "$tmp" "$output"
            return 0
        else
            status=$?
        fi
        rm -f "$tmp"
        if ! download_curl_status_retryable "$status" "${http_status:-000}" || [ "$attempt" -eq "$attempts" ]; then
            echo "[g5-worker] FATAL: failed to download ${label} from ${url} (curl_exit=${status}, http=${http_status:-000}, attempt=${attempt}/${attempts})" >&2
            return "$status"
        fi
        echo "[g5-worker] ${label} transient download failure (curl_exit=${status}, http=${http_status:-000}); retrying in ${delay}s" >&2
        sleep "$delay"
        if [ "$delay" -lt "$max_delay" ]; then
            delay=$((delay * 2))
            [ "$delay" -le "$max_delay" ] || delay="$max_delay"
        fi
        attempt=$((attempt + 1))
    done
}
