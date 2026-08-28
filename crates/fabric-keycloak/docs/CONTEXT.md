# fabric-keycloak — LLM context

The Keycloak identity-provider adapter. Depends on `fabric-core`,
`fabric-client-model`, `fabric-reconciliation`, `async-trait`, `reqwest`,
`serde`, `serde_json`, `tokio`, `tracing`. Event domain `12`.

**Only `fabric-control-plane-api` may depend on this crate**, and
`scripts/check_architecture.py` enforces it.

## Public surface

Three items, deliberately:

- `KeycloakIdentityProvider::new(&KeycloakConfig, AdminCredential, Arc<dyn Clock>)
  -> Result<Self, String>`. Implements `fabric_reconciliation::IdentityProvider`.
- `KeycloakConfig { base_url, admin_realm, client_id, client_secret_ref, http_timeout_seconds }`
  + `validate()`. All non-secret; belongs in a `ConfigMap`.
- `AdminCredential::new(impl Into<String>)`. No `Display`; `Debug` prints
  `AdminCredential(redacted)`. `expose()` is `pub(crate)` and named to be
  conspicuous.

Everything else — `wire::*`, `admin::*` — is `pub(crate)`.

## Internal shape

- `admin::Paths` — every URL this crate builds, in one file. `RealmName` and
  `OidcClientId` are DNS-label/identifier validated, which is why interpolating
  them is safe without escaping here.
- `admin::TokenCache` — client-credentials grant against
  `{base}/realms/{admin_realm}/protocol/openid-connect/token`, cached for
  `expires_in` less 30s, `tokio::sync::Mutex` held across the refresh.
- `admin::KeycloakAdmin` — `get`, `get_optional` (404 → `None`),
  `create` (409 → `Ok`), `update`. Bearer auth applied in `send`.
- `admin::errors` — `transport_failure`, `status_failure`. Never formats
  `reqwest::Error` (it can carry the full URL) and never reads a body.
- `wire` — `RealmRepresentation`, `NewRealmRepresentation`, `RealmUpdate`,
  `RoleRepresentation`, `NewRoleRepresentation`, `ClientRepresentation`,
  `NewClientRepresentation`, `TokenResponse` (no `Debug`, holds a token).
- `provider::observe` / `provider::mutate` — one function per port operation.

## Hard invariants — do not break

1. **No Keycloak type may become `pub`.** The architecture check greps for
   `*Representation`, `RealmUpdate`, `TokenResponse`, `publicClient`,
   `standardFlowEnabled`, `openid-connect` outside this crate.
2. **No response body in any error, ever.**
3. **`RealmUpdate` stays two fields.** A fuller body resets settings SaaS Fabric
   does not manage.
4. **`NewClientRepresentation` must never gain a secret field.**
5. **`create` treats `409` as success.** The port requires idempotence.
6. **`get_optional` treats `404` as absence and nothing else as absence.**
   Creating a realm over one that is merely unreachable replaces a live realm
   with an empty one.
7. **`AdminCredential` keeps its redacting `Debug` and gains no `Display`.**
8. **Nothing here deletes.**

## Design notes

- `ROLE_PAGE = 2000`, and a realm returning exactly that many is reported as a
  failure rather than reconciled against a truncated list. A silent truncation
  would leave a client permanently reporting changes it had already made, with
  no log line saying why.
- `update_oidc_client` looks the client up first, because Keycloak addresses an
  update by its internal `id` and not by `clientId`. A client that has vanished
  between observation and update is created rather than reported as a mystery.
- `AdminCredential` is deliberately *not* `fabric_connector::ResolvedSecret`:
  sharing it would put a runtime-plane crate in the control plane's graph for
  forty lines.
- `tests/support/fake_keycloak.rs` is a real socket, because everything worth
  testing here is protocol — path, body, bearer, and how `404`/`409` are read.
