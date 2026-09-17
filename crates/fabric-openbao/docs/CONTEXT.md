# fabric-openbao — LLM context

The control plane's secret partition and integration record, kept in
OpenBao. Control-plane crate (see `docs/architecture/crate-dependencies.md`).
The only crate in the workspace that knows OpenBao exists. Depends on
`fabric-core`, `fabric-control-plane` (for the ports and their types), plus
`async-trait`, `reqwest`, `serde`, `serde_json`, `tokio` (`fs`, `sync`
features), `tracing` (declared but, as of this writing, not called anywhere
in `src/`). Dev-dependency: `tokio` (`macros`, `rt-multi-thread`).

## Public surface (all re-exported from `lib.rs`)

- `OpenBao` — the low-level client. `new(config: &OpenBaoConfig, clock:
  Arc<dyn Clock>) -> Result<Self, String>`. `describe() -> String` (address,
  mount, partition — never a credential). No public read/write methods;
  `kv.rs`, `client_secrets.rs`, `secret_store.rs` and `integration_store.rs`
  call its `pub(crate)` `send`/`send_in`/`data_url`/`metadata_url`/`address`/`mount`.
- `OpenBaoSecretStore` — implements `fabric_control_plane::SecretStore`
  (`get`/`put`/`delete` by `&SecretName`, `describe`). `new(client: Arc<OpenBao>) -> Self`.
  A single field, `VALUE = "value"` (private), is the one field name every
  entry is written/read under.
- `OpenBaoIntegrationStore` — implements
  `fabric_control_plane::IntegrationStore` (`load`/`save`/`clear` by
  `IntegrationKind`). `new(client: Arc<OpenBao>) -> Self`. Serialises
  `GitIntegration` to a JSON string under the field `DOCUMENT = "record"`
  (private).
- `OpenBaoClientSecrets` — implements `fabric_control_plane::ClientSecrets`
  (`list`/`metadata`/`reveal`/`write`/`delete`, all keyed by
  `&SecretNamespace` + `&SecretPath`). `new(store: Arc<OpenBao>) -> Self` (`const`).
- `OpenBaoConfig` (`serde::Deserialize`, `Clone`, `Debug`,
  `deny_unknown_fields`, all fields `pub`) — `address: String` (required),
  `mount: String` (default `"secret"`), `prefix: String` (default
  `"platform/saas-fabric/instances/master"`), `auth_mount: String` (default
  `"kubernetes"`), `role: String` (required), `service_account_token_path: String`
  (default `/var/run/secrets/kubernetes.io/serviceaccount/token`),
  `http_timeout_seconds: u64` (default `10`). `Debug` is the plain derive —
  nothing here is a secret.

## Internal modules

- `auth.rs` + `auth/login.rs` — `TokenCache` (`pub(crate)`; Kubernetes-auth
  login, `EXPIRY_MARGIN = 60s`, `MINIMUM_LIFETIME = 30s`) and `login`
  (`pub(super)`, the exchange itself: reads the JWT off disk at
  `token_path`, `POST {endpoint}` with `{"role", "jwt": jwt.trim()}`, never
  reads a failed login's body — it can echo the presented assertion). `Held`
  (`pub(super)`) — `value: String` (never logged), `good_until: Instant`.
- `client.rs` + `client/requests.rs` — `OpenBao`'s fields
  (`address`, `mount`, `prefix`, `tokens: TokenCache`, `http:
  reqwest::Client`) and URL-building (`data_url`, `metadata_url`, both
  `pub(crate)`, format `{address}/v1/{mount}/data|metadata/{prefix}/{name}`);
  `send`/`send_in` (`pub(crate)`) retry once on `403` after
  `tokens.invalidate()`; `send_in` additionally sets `X-Vault-Namespace`.
  Both delegate to private `attempt`/`attempt_in`, which set
  `X-Vault-Token` from `tokens.token(&self.http)`.
- `kv.rs` — `Read { Found(serde_json::Map<String, serde_json::Value>),
  Absent }` (`pub(crate)` enum) plus `OpenBao::read/write/remove`
  (`pub(crate)`) over one KV v2 entry beneath the instance's own
  `mount`/`prefix`. `read` unwraps the version-2 double nesting
  (`body.data.data`); `404` on read is `Read::Absent`, not an error. `remove`
  uses the metadata endpoint (deletes every version); a `404` on remove is
  treated as success.
- `secret_store.rs` — `OpenBaoSecretStore`, plus `pub(crate) fn
  classify(error: &str) -> SecretStoreError` — inspects the client's
  already-reduced error *message* (`"refused the login"` / `"403"` →
  `NotPermitted`; `"could not be read"` / `"no entry"` → `Malformed`; else
  `Unavailable`) because `OpenBao`'s low-level methods return `Result<_,
  String>` rather than a structured error. Shared with `integration_store.rs`.
- `integration_store.rs` — `OpenBaoIntegrationStore`; `record(kind:
  IntegrationKind) -> &'static str` (private `const fn`) maps
  `ClientConfiguration` → `"git/integration"`, `PlatformManagement` →
  `"integrations/platform-management/integration"` — the first path predates
  the second and is deliberately not being moved for symmetry. `translate(error:
  &str) -> IntegrationStoreError` (private) calls `classify` and maps
  `SecretStoreError::{Unavailable, ReadOnly}` both onto
  `IntegrationStoreError::Unavailable` (which has no `ReadOnly` of its own).
- `client_secrets.rs` + `client_secrets/{listing,operations,wire}.rs` —
  `OpenBaoClientSecrets`; every request carries `X-Vault-Namespace` via
  `send` (private, wraps `store.send_in`, maps a transport failure to
  `SecretsError::Unavailable`). `entries_at` (`pub(super)`) issues an HTTP
  `LIST` (`reqwest::Method::from_bytes(b"LIST")`, falling back to `GET` if
  that somehow fails to parse) against the metadata endpoint, treats `404`
  as an empty `Vec`, and reads `/data/keys`. `listing::walk` (`pub(super)`)
  does a breadth-first, depth-bounded (`MAX_DEPTH = 8`) traversal, skipping
  any entry that will not parse as a `SecretPath`. `operations.rs`
  implements the `ClientSecrets` trait's five methods. `wire.rs`
  (`pub(super)` functions `body`, `metadata`, `values`, `written`, `removed`)
  maps HTTP status/body into `SecretsError`: `NotFound` (404), `Refused`
  (403/401), `Conflict` (400, **only** checked by `written`, ahead of the
  generic status mapping — a CAS mismatch), else `Unavailable`. `values`
  accepts only string-typed fields, erroring on anything else.
- `config.rs` — `OpenBaoConfig` and its five `default_*` functions.

## Hard invariants — do not break

1. **A client's namespace travels only as the `X-Vault-Namespace` header**,
   never assembled into a path by this crate. OpenBao enforces the
   isolation; this crate only supplies the (trusted) namespace value —
   ADR 0017's split ("OpenBao enforces namespace isolation. Fabric enforces
   which namespace an operation may target.") is the reasoning to preserve
   verbatim in any change here.
2. **`ClientSecrets::write` always sends `cas`.** `expected.unwrap_or(0)` —
   omitting it on a `None` `expected` (meaning "must not exist") would let a
   write silently clobber a version the caller never read.
3. **Delete goes through the metadata endpoint**, in both `kv::remove` and
   `client_secrets` delete — the data endpoint only soft-deletes the latest
   version.
4. **The two `IntegrationKind`s are stored at two independent, fixed paths.**
   `git/integration`'s path is not being changed for symmetry with the newer
   `integrations/platform-management/integration` — see `record()`'s own
   doc comment.
5. **No store token, secret value, or private key is ever logged or
   `Debug`-printed.** This crate never derives `Debug` on anything holding
   one (`auth::Held`, the KV field maps); the redacting `Debug` for a secret
   *value* already lives on `fabric_control_plane::SecretValue`, not here.
6. **A `404` on a client-secrets listing is an empty result, not an error**
   (`entries_at`) — a client with no secrets yet is ordinary. A `404` on
   `OpenBao::read` (the instance's own partition) is likewise `Read::Absent`,
   not an error; a `404` on `OpenBao::remove` or the client-secrets delete
   path is treated as a successful no-op.

## Notes

- `OpenBaoSecretStore` and `OpenBaoIntegrationStore` are normally
  constructed over the *same* `Arc<OpenBao>` so one Kubernetes-auth login
  serves both — they are separate types over shared infrastructure, not
  separate connections.
- `tests/client_secrets.rs` and `tests/secret_partition.rs` are the
  integration tests; the former needs a real OpenBao (opt-in via
  `FABRIC_SECRETS_STORE`, and once set, a missing `FABRIC_SECRETS_TOKEN_FILE`
  is a panic rather than a skip — see `scripts/secrets-store.sh`), the
  latter runs against a fake HTTP server over a real TCP socket
  (`tests/support/mod.rs`'s `FakeOpenBao`).
- `classify(&str) -> SecretStoreError` and `translate(&str) ->
  IntegrationStoreError` both work by inspecting the client's own
  already-reduced error *message* rather than a status code, because
  `OpenBao`'s low-level methods (`read`/`write`/`remove`, `send`/`send_in`)
  return `Result<_, String>` rather than a structured error.
