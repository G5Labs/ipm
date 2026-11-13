#!/usr/bin/env bash
# Install ephemeral runtime-staging prerequisites with bounded retries for
# transient Ubuntu mirror/network failures. Call only inside disposable
# Ubuntu staging containers.
set -euo pipefail

apt_install_with_retry() {
    if [ "$#" -eq 0 ]; then
        echo "usage: apt-install-with-retry.sh <package>..." >&2
        return 2
    fi

    local retry_attempts="${APT_RETRY_ATTEMPTS:-4}"
    local initial_delay_seconds="${APT_RETRY_INITIAL_DELAY_SECONDS:-5}"
    local attempt_timeout_seconds="${APT_RETRY_ATTEMPT_TIMEOUT_SECONDS:-120}"
    local packages_preinstalled="${APT_PACKAGES_PREINSTALLED:-0}"
    case "$retry_attempts" in
        ''|*[!0-9]*|0) echo "APT_RETRY_ATTEMPTS must be a positive integer" >&2; return 2 ;;
    esac
    case "$initial_delay_seconds" in
        ''|*[!0-9]*) echo "APT_RETRY_INITIAL_DELAY_SECONDS must be a non-negative integer" >&2; return 2 ;;
    esac
    case "$attempt_timeout_seconds" in
        ''|*[!0-9]*|0) echo "APT_RETRY_ATTEMPT_TIMEOUT_SECONDS must be a positive integer" >&2; return 2 ;;
    esac
    case "$packages_preinstalled" in
        0|1) ;;
        *) echo "APT_PACKAGES_PREINSTALLED must be 0 or 1" >&2; return 2 ;;
    esac

    if [ "$packages_preinstalled" = "1" ]; then
        local package
        local status
        local missing_packages=()
        for package in "$@"; do
            if ! status="$(dpkg-query -W -f='${db:Status-Abbrev}' "$package" 2>/dev/null)" \
                || [ "${status#ii}" = "$status" ]; then
                missing_packages+=("$package")
            fi
        done
        if [ "${#missing_packages[@]}" -ne 0 ]; then
            echo "[stage-runtime] preprovisioned staging image is missing package(s): ${missing_packages[*]}" >&2
            return 1
        fi
        echo "[stage-runtime] verified $# preinstalled staging package(s); skipping APT network bootstrap" >&2
        return 0
    fi

    local attempt=1
    local attempt_status
    local delay_seconds
    while [ "$attempt" -le "$retry_attempts" ]; do
        echo "[stage-runtime] apt package bootstrap attempt ${attempt}/${retry_attempts}" >&2
        # Variables expand in the bounded child Bash, not in this shell.
        # shellcheck disable=SC2016
        if timeout --signal=TERM --kill-after=5s "${attempt_timeout_seconds}s" \
            bash -c '
                set -euo pipefail
                export DEBIAN_FRONTEND="${DEBIAN_FRONTEND:-noninteractive}"
                apt-get \
                    -o Acquire::Retries=3 \
                    -o Acquire::http::Timeout=30 \
                    -o Acquire::https::Timeout=30 \
                    update -qq >/dev/null
                apt-get \
                    -o Acquire::Retries=3 \
                    -o Acquire::http::Timeout=30 \
                    -o Acquire::https::Timeout=30 \
                    install -y -qq --no-install-recommends "$@" >/dev/null
            ' apt-install-with-retry "$@"; then
            return 0
        else
            attempt_status=$?
            if [ "$attempt_status" -eq 124 ]; then
                echo "[stage-runtime] apt package bootstrap attempt ${attempt}/${retry_attempts} timed out after ${attempt_timeout_seconds}s" >&2
            else
                echo "[stage-runtime] apt package bootstrap attempt ${attempt}/${retry_attempts} exited with status ${attempt_status}" >&2
            fi
        fi

        if [ "$attempt" -eq "$retry_attempts" ]; then
            echo "[stage-runtime] apt package bootstrap exhausted ${retry_attempts} attempts" >&2
            return 1
        fi

        delay_seconds=$((initial_delay_seconds * (1 << (attempt - 1))))
        echo "[stage-runtime] apt package bootstrap failed; retrying in ${delay_seconds}s" >&2
        sleep "$delay_seconds"
        attempt=$((attempt + 1))
    done
}

apt_install_with_retry "$@"
