#!/usr/bin/env bash
# =============================================================================
# resolve-e2e-url.sh — Resolve the correct E2E API URL
# =============================================================================
# Inside a devcontainer, localhost doesn't reach sibling containers. This
# script connects the devcontainer to the test stack's Docker network so
# compose service hostnames (g5-api, client, oauth-stub) resolve correctly.
#
# Usage: source scripts/resolve-e2e-url.sh
# Sets: G5_E2E_API_URL, E2E_BASE_URL, E2E_API_URL, E2E_OAUTH_STUB_URL,
#       G5_E2E_DATABASE_URL
# =============================================================================

TEST_COMPOSE_PROJECT="${G5_CI_COMPOSE_PROJECT:-${COMPOSE_PROJECT_NAME:-g5-test}}"
TEST_NETWORK="${TEST_COMPOSE_PROJECT}_g5-test"
export E2E_API_CONTAINER="${E2E_API_CONTAINER:-${TEST_COMPOSE_PROJECT}-api}"
API_SERVICE="${TEST_COMPOSE_PROJECT}-api"
CLIENT_SERVICE="${TEST_COMPOSE_PROJECT}-client"
OAUTH_SERVICE="${TEST_COMPOSE_PROJECT}-oauth-stub"

# Detect if we're running inside a Docker container
_in_container() {
  [[ -f /.dockerenv ]] || grep -qE 'docker|containerd' /proc/1/cgroup 2>/dev/null
}

if _in_container; then
  # Connect this container to the test stack network (idempotent)
  SELF_ID=$(cat /proc/self/cgroup 2>/dev/null | grep -oP '[0-9a-f]{64}' | head -1 || hostname)
  if [[ -n "$SELF_ID" ]]; then
    docker network connect "$TEST_NETWORK" "$SELF_ID" 2>/dev/null || true
  fi

  # Now compose service hostnames should resolve. Verify with a quick DNS check.
  if getent hosts "$API_SERVICE" &>/dev/null; then
    export G5_E2E_API_URL="http://${API_SERVICE}:7500"
    export E2E_API_URL="http://${API_SERVICE}:7500"
    # The test edge serves HTTPS on its network alias so previews run on
    # isolated hosts (infra/docker/docker-compose.test.yml `client`).
    export E2E_BASE_URL="https://${G5_CI_E2E_APEX:-app.g5-e2e.test}"
    export E2E_OAUTH_STUB_URL="${E2E_OAUTH_STUB_URL:-http://${OAUTH_SERVICE}:9876}"
    export G5_E2E_DATABASE_URL="${G5_E2E_DATABASE_URL:-postgres://g5:g5_test@${TEST_COMPOSE_PROJECT}-postgres:5432/g5_test}"
  else
    # Fallback: resolve via docker inspect
    API_IP=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "${TEST_COMPOSE_PROJECT}-api" 2>/dev/null || echo "")
    CLIENT_IP=$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "${TEST_COMPOSE_PROJECT}-client" 2>/dev/null || echo "")
    if [[ -n "$API_IP" ]]; then
      export G5_E2E_API_URL="http://${API_IP}:7500"
      export E2E_API_URL="http://${API_IP}:7500"
    fi
    if [[ -n "$CLIENT_IP" ]]; then
      # No alias resolution without the network: the HTTPS edge still
      # answers by IP, but the TLS name will not match (ignoreHTTPSErrors).
      export E2E_BASE_URL="https://${CLIENT_IP}"
    fi
    export G5_E2E_DATABASE_URL="${G5_E2E_DATABASE_URL:-postgres://g5:g5_test@${TEST_COMPOSE_PROJECT}-postgres:5432/g5_test}"
  fi
else
  # Running on the host against docker-compose.test.yml — use the published
  # test-stack ports, not the container-internal service ports. Prefer the
  # current Docker port mapping when the stack is already running so ad-hoc
  # host commands keep working even when ci-docker had to choose non-default
  # host ports to avoid collisions.
  _published_port() {
    local env_name="$1"
    local container_name="$2"
    local container_port="$3"
    local default_port="$4"
    local value="${!env_name:-}"
    if [[ -n "$value" ]]; then
      echo "$value"
      return
    fi
    value="$(docker port "$container_name" "${container_port}/tcp" 2>/dev/null | awk -F: 'NR==1 {print $NF}')"
    if [[ -n "$value" ]]; then
      echo "$value"
    else
      echo "$default_port"
    fi
  }

  API_HOST_PORT="$(_published_port G5_TEST_PORT_API "${TEST_COMPOSE_PROJECT}-api" 7500 7501)"
  EDGE_HOST_PORT="$(_published_port G5_TEST_PORT_EDGE "${TEST_COMPOSE_PROJECT}-client" 80 3001)"
  POSTGRES_HOST_PORT="$(_published_port G5_TEST_PORT_POSTGRES "${TEST_COMPOSE_PROJECT}-postgres" 5432 5433)"

  export G5_E2E_API_URL="${G5_E2E_API_URL:-http://localhost:${API_HOST_PORT}}"
  export E2E_API_URL="${E2E_API_URL:-http://localhost:${API_HOST_PORT}}"
  export E2E_BASE_URL="${E2E_BASE_URL:-http://localhost:${EDGE_HOST_PORT}}"
  export E2E_OAUTH_STUB_URL="${E2E_OAUTH_STUB_URL:-http://localhost:9876}"
  export G5_E2E_DATABASE_URL="${G5_E2E_DATABASE_URL:-postgres://g5:g5_test@localhost:${POSTGRES_HOST_PORT}/g5_test}"
fi


echo "E2E URLs: API=${G5_E2E_API_URL:-unset} Client=${E2E_BASE_URL:-unset} OAuth=${E2E_OAUTH_STUB_URL:-unset} DB=${G5_E2E_DATABASE_URL:-unset}"
