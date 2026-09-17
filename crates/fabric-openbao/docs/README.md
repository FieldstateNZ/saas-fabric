# fabric-openbao

The control plane's secret partition and integration record, kept in
OpenBao. This is the only crate in the workspace that knows OpenBao exists —
`fabric-control-plane` defines the ports, and this crate is one
implementation of them.

Control-plane crate (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on `fabric-control-plane` and `fabric-core`.

## Why this crate exists

Until [ADR 0011](../../../docs/decisions/0011-the-platform-creates-its-own-git-application.md),
the control plane never had to write a secret anywhere: every credential was
created by a human and delivered into the pod's environment by External
Secrets. That is a one-way path. The moment the platform started
*generating* credential material itself — a GitHub App's private key, which
the host hands back exactly once — it needed somewhere to put it, and
projecting secrets inward provides no such thing. So the control plane
becomes a client of a secret store, and this crate is that client.

It implements three ports `fabric-control-plane` owns:

- `SecretStore` — this Fabric instance's own secrets (the platform's GitHub
  App private key, chiefly).
- `IntegrationStore` — the Git integration *record* (application id, slug,
  installation id, repository) for either integration kind — see
  [ADR 0017](../../../docs/decisions/0017-fabric-decides-which-client-secret-boundary-an-operation-reaches.md)
  and "Two integration kinds, one store" below.
- `ClientSecrets` — a *client's* secrets, inside that client's own OpenBao
  namespace.

Nothing above this crate names a mount, a path, an authentication method, or
a lease. The domain asks for a secret by name within a partition; where that
partition physically lives is decided entirely here.

## Key concepts

- **Kubernetes auth, not a static token.** The pod exchanges its own
  service-account JWT for a store token (`auth.rs`, `auth/login.rs`) against
  OpenBao's Kubernetes auth mount (`POST {address}/v1/auth/{auth_mount}/login`
  with `{"role", "jwt"}`). There is no credential for a human to create,
  rotate, or transport — the pod already holds an identity its orchestrator
  issued.
- **`OpenBao`** — the low-level client: builds request URLs under one
  instance's `mount`/`prefix`, holds the login token cache, and retries once
  on a `403` after invalidating the cached token (`client/requests.rs`) — a
  store token can be revoked before its lease expires, and this is how that
  shows up rather than as a wedged failure.
- **The instance's own partition vs. a client's namespace.** `OpenBaoSecretStore`
  and `OpenBaoIntegrationStore` read and write under this instance's own
  `mount/prefix` (a path, resolved server-side by this crate — see
  `client.rs`'s `data_url`/`metadata_url`). `OpenBaoClientSecrets` instead
  sends every request with an `X-Vault-Namespace: <client>` **header** — a
  boundary OpenBao itself enforces, not a path prefix this code assembles.
  Measured against a real store: the identical path with no namespace
  header, or with a *different* namespace header, both answer `404` (see
  [ADR 0017](../../../docs/decisions/0017-fabric-decides-which-client-secret-boundary-an-operation-reaches.md)).
  Fabric decides *which* namespace a request may target (from trusted
  desired state, never a caller-supplied value); OpenBao decides that a
  request naming one namespace cannot see another's data.
- **Two integration kinds, one store.** `IntegrationKind::ClientConfiguration`
  and `IntegrationKind::PlatformManagement` are recorded at two different,
  fixed paths (`git/integration` and
  `integrations/platform-management/integration`) — the first is where a
  connected instance's record has always lived and is not being moved for
  symmetry; the second is new. Both share one `OpenBaoIntegrationStore`
  because introducing a second backing service for a dozen non-secret fields
  would be a service to run, secure and back up for no benefit — what keeps
  the two honest is the *type* the record round-trips through
  (`GitIntegration`, serialised as JSON under the `record` field), not the
  storage location.
- **Check-and-set on client secrets.** `ClientSecrets::write` always sends
  `cas` (`expected.unwrap_or(0)` — `None` means "must not already exist"),
  never omitting it — omitting it would let the store accept a write against
  a version somebody else already moved past, which is exactly the silent
  overwrite versioning exists to prevent. A CAS mismatch (the store's `400`)
  comes back as `SecretsError::Conflict`, checked before any other status is
  considered, so it is never folded into a generic failure.
- **Delete means delete.** Every removal (`kv::remove`, and the client-secret
  delete path) goes through the *metadata* endpoint, not the *data*
  endpoint. Deleting through `data` marks only the newest version deleted
  and leaves older versions readable — for a credential, that is not
  deletion at all.

## How the pieces fit

```text
fabric-control-plane          defines SecretStore, IntegrationStore, ClientSecrets
        |
fabric-openbao                implements all three, over one client
        |
   OpenBao (client.rs)        one login, one HTTP client, per-instance partition
        |
   OpenBao / Vault server     Kubernetes auth  +  KV v2 mounts, one per namespace
```

`OpenBaoSecretStore` and `OpenBaoIntegrationStore` are typically built over
the *same* `Arc<OpenBao>` client, so one login serves both — see
`integration_store.rs`'s module docs for why the two ports still stay
separate types even though they share a backing service.

## Getting started

```rust,ignore
use std::sync::Arc;
use fabric_core::SystemClock;
use fabric_openbao::{OpenBao, OpenBaoConfig, OpenBaoSecretStore, OpenBaoIntegrationStore};

let config: OpenBaoConfig = figment::Figment::new()/* ... */.extract()?;
let client = Arc::new(OpenBao::new(&config, SystemClock::shared())?);

let secrets = OpenBaoSecretStore::new(Arc::clone(&client));
let integrations = OpenBaoIntegrationStore::new(client);
```

`OpenBaoConfig` needs at minimum `address` and `role`; `mount`, `prefix`,
`auth_mount`, `service_account_token_path` and `http_timeout_seconds` all
default sensibly for an in-cluster deployment (see `config.rs`).

## Common tasks

- **Reading or writing this instance's own secret** — go through
  `SecretStore::get`/`put`/`delete` on `OpenBaoSecretStore`. Names are opaque
  strings scoped beneath the configured `prefix`; there is no way to name a
  path outside it.
- **Recording or clearing a Git integration** — go through
  `IntegrationStore::load`/`save`/`clear` with the right `IntegrationKind`.
  Do not add a third storage path for a new integration kind without
  checking whether it truly needs one — see `integration_store.rs`'s
  `record()` function and its doc comment.
- **Reading or writing a client's secret** — go through `ClientSecrets` on
  `OpenBaoClientSecrets`, always with that client's `SecretNamespace`. Never
  attempt to assemble a namespace-scoped path by hand; the namespace must
  travel as the `X-Vault-Namespace` header for OpenBao's own isolation to
  apply.
- **Running the integration tests against a real store** — see
  `tests/client_secrets.rs`'s module docs; `./scripts/secrets-store.sh up`
  prints the environment variables to set.

## What this crate deliberately does not do

- **No path assembled for a client namespace.** `OpenBaoClientSecrets` never
  builds `.../<client>/...` into a URL; the namespace travels only as the
  `X-Vault-Namespace` header, so isolation is OpenBao's to enforce, not this
  crate's to remember.
- **No secret value ever formatted with `Debug`.** Nothing in this crate
  derives `Debug` on a type holding a token or a secret's fields
  (`auth::Held`, the KV response maps); the control plane's own
  `SecretValue` already hand-writes a redacting `Debug`.
- **No soft delete.** Every removal goes through the metadata endpoint,
  which drops every version, not just the newest.

## Tests

- `tests/client_secrets.rs` — a client's secrets against a real store,
  opt-in via `FABRIC_SECRETS_STORE` (and, once set, a missing
  `FABRIC_SECRETS_TOKEN_FILE` is a hard panic, not a skip). Proves what only
  a real store can: that the namespace header is a boundary OpenBao
  enforces, that a stale check-and-set is refused, and that a delete removes
  every version.
- `tests/secret_partition.rs` — the instance's own secret store and
  integration store, against a fake OpenBao over a real TCP socket
  (`tests/support/mod.rs`'s `FakeOpenBao`), not a mocked client — proof
  against protocol details: login before read, the token in the header,
  version-2 double nesting, `404` read as absence, deletion through
  `metadata`, and re-login after a refusal.

## Gotchas

- The crate is `fabric-openbao` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_openbao` (underscore):
  `RUST_LOG=info,fabric_openbao=debug`. As of this writing, though, nothing
  in `src/` actually calls `tracing::*` — it is a declared dependency with
  no current call site.
- `OpenBaoConfig::prefix` defaults to `"platform/saas-fabric/instances/master"`
  — there is no way for a caller to escape it. Every name this crate is
  asked for is resolved *beneath* it, which is what makes the prefix a
  partition rather than a convention.
- `entries_at` (used by the client-secrets listing walk) treats a `404` from
  the store as an *empty* list, not an error — a client with no secrets yet
  is the ordinary state, not a failure to report the first time an operator
  opens the tab. The listing itself uses HTTP method `LIST` (OpenBao's own
  verb, via `reqwest::Method::from_bytes(b"LIST")`), not `GET`.
- The listing walk (`client_secrets/listing.rs`) is bounded to 8 levels of
  depth (`MAX_DEPTH`). Deeper nesting is not hidden — it is a signal the
  naming convention has drifted, and a truncated tree is a better failure
  than a request that never returns.
- `translate` (in `integration_store.rs`) maps both `SecretStoreError::Unavailable`
  and `SecretStoreError::ReadOnly` to `IntegrationStoreError::Unavailable` —
  `IntegrationStoreError` has no `ReadOnly` variant of its own, because
  nothing about an integration record is read-only the way an
  environment-backed secret store can be.
- `OpenBaoConfig`'s `Debug` derive is fine to print: nothing in it is a
  secret (the role and mount names are configuration, not credentials). The
  actual store token lives only in `auth::Held`, which is never printed and
  never leaves `TokenCache`.
