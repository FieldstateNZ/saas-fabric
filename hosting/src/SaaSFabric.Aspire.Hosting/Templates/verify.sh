#!/bin/sh
set -eu
# Read-back checks keep credentials in process variables and out of logs.
access_token="$(curl -fsS "$KEYCLOAK_URL/realms/master/protocol/openid-connect/token" \
  --data-urlencode client_id=admin-cli --data-urlencode grant_type=password \
  --data-urlencode "username=$KEYCLOAK_USER" --data-urlencode "password=$KEYCLOAK_PASSWORD" | jq -er .access_token)"
if [ -n "${BRAND_THEME:-}" ]; then
  curl -fsS -H "Authorization: Bearer $access_token" "$KEYCLOAK_URL/admin/realms/$TF_VAR_realm" |
    jq -e --arg theme "$BRAND_THEME" '.loginTheme == $theme' >/dev/null
fi
curl -fsS -H "Authorization: Bearer $access_token" "$KEYCLOAK_URL/admin/realms/$TF_VAR_realm/clients" |
  jq -e --argjson apps "$TF_VAR_applications" '
    [.[] | select(.clientId as $id | $apps | has($id))] |
    length == ($apps | length) and all(.[];
      .publicClient and .standardFlowEnabled and (.directAccessGrantsEnabled | not) and
      .attributes["pkce.code.challenge.method"] == "S256")' >/dev/null
curl -fsS -H "X-Vault-Token: $VAULT_TOKEN" -H "X-Vault-Namespace: $TF_VAR_client_id" \
  "$VAULT_ADDR/v1/sys/mounts" | jq -e '.data["secret/"].options.version == "2"' >/dev/null
curl -fsS -H "X-Vault-Token: $VAULT_TOKEN" -H "X-Vault-Namespace: $TF_VAR_client_id" \
  "$VAULT_ADDR/v1/sys/policies/acl/client" | jq -e '.data.policy | contains("secret/data/*")' >/dev/null
