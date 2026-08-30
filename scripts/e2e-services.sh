#!/usr/bin/env bash
# Starts the services the end-to-end suite needs, and prints how to enable it.
#
# The issuer is deliberately unreachable: Keycloak mints tokens for
# `https://identity.example/realms/<realm>` while this process reads keys from
# `http://127.0.0.1:18300`. That split is the property the suite exists to
# prove, so the fixture has to create it rather than describe it.
set -euo pipefail

KEYCLOAK_PORT=18300
OPENFGA_PORT=18301
ADMIN_PASSWORD=e2e-fixture-password

case "${1:-up}" in
  up)
    docker rm -f fabric-e2e-keycloak fabric-e2e-openfga >/dev/null 2>&1 || true

    docker run -d --name fabric-e2e-openfga -p "${OPENFGA_PORT}:8080" \
      openfga/openfga:latest run >/dev/null

    docker run -d --name fabric-e2e-keycloak -p "${KEYCLOAK_PORT}:8080" \
      -e KC_BOOTSTRAP_ADMIN_USERNAME=admin \
      -e KC_BOOTSTRAP_ADMIN_PASSWORD="${ADMIN_PASSWORD}" \
      -e KC_HOSTNAME=https://identity.example \
      -e KC_HOSTNAME_STRICT=false \
      quay.io/keycloak/keycloak:26.0 start-dev >/dev/null

    printf 'waiting for Keycloak'
    for _ in $(seq 1 60); do
      if curl -sf -m 2 "http://127.0.0.1:${KEYCLOAK_PORT}/realms/master/.well-known/openid-configuration" >/dev/null 2>&1; then
        echo " ready"
        break
      fi
      printf '.'
      sleep 5
    done

    echo "creating realms"
    ADMIN=$(curl -s "http://127.0.0.1:${KEYCLOAK_PORT}/realms/master/protocol/openid-connect/token" \
      -d grant_type=password -d client_id=admin-cli -d username=admin \
      -d "password=${ADMIN_PASSWORD}" | sed -n 's/.*"access_token":"\([^"]*\)".*/\1/p')

    # Realms live here rather than in the test, because a Rust file naming
    # `publicClient` or a protocol mapper would put Keycloak's own vocabulary
    # in a crate that must not know it (ADR 0008, enforced by
    # scripts/check_architecture.py). The suite reads a token and nothing else.
    for REALM in tworealmsacme tworealmsfoo unregacme unregrogue audienceacme \
                 outageacme cachedacme injectacme injectvictim; do
      API="http://127.0.0.1:${KEYCLOAK_PORT}/admin/realms"

      curl -s -o /dev/null -X POST "${API}" -H "Authorization: Bearer ${ADMIN}" \
        -H 'Content-Type: application/json' \
        -d "{\"realm\":\"${REALM}\",\"enabled\":true}"

      curl -s -o /dev/null -X POST "${API}/${REALM}/clients" -H "Authorization: Bearer ${ADMIN}" \
        -H 'Content-Type: application/json' \
        -d '{"clientId":"app","publicClient":true,"directAccessGrantsEnabled":true,
             "redirectUris":["*"],
             "protocolMappers":[{"name":"openfga-audience","protocol":"openid-connect",
               "protocolMapper":"oidc-audience-mapper",
               "config":{"included.client.audience":"openfga","access.token.claim":"true"}}]}'

      curl -s -o /dev/null -X POST "${API}/${REALM}/users" -H "Authorization: Bearer ${ADMIN}" \
        -H 'Content-Type: application/json' \
        -d '{"username":"tenantuser","enabled":true,"email":"u@example.invalid",
             "firstName":"T","lastName":"U",
             "credentials":[{"type":"password","value":"e2e-fixture-password","temporary":false}]}'
    done
    echo "realms ready"

    cat <<EOF

Run the suite with:

  FABRIC_E2E=1 \\
  FABRIC_E2E_KEYCLOAK=http://127.0.0.1:${KEYCLOAK_PORT} \\
  FABRIC_E2E_ISSUER_BASE=https://identity.example \\
  FABRIC_E2E_OPENFGA_PORT=${OPENFGA_PORT} \\
  cargo test -p fabric-fga-auth --test whole_path
EOF
    ;;

  down)
    docker rm -f fabric-e2e-keycloak fabric-e2e-openfga >/dev/null 2>&1 || true
    echo "stopped"
    ;;

  *)
    echo "usage: $0 [up|down]" >&2
    exit 2
    ;;
esac
