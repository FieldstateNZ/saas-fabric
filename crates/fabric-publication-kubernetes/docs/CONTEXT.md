# fabric-publication-kubernetes — LLM context

The Kubernetes adapter for `fabric-runtime-publication`'s `RuntimePublication`
port (ADR 0018 "The Kubernetes adapter", ADR 0023 part 4). Control plane.
Dependencies: `fabric-core`, `fabric-runtime-publication`, `async-trait`,
`reqwest`, `serde`, `serde_json`, `thiserror`, `tokio` (`fs`). No `kube`
crate, by workspace rule.

## Public surface (`lib.rs`)

- `PublicationTarget { namespace: String }` (`config.rs`) — `deny_unknown_fields`;
  `validate() -> Result<(), String>` refuses anything but a DNS label.
- `KubernetesRuntimePublication` (`publish.rs`) — `in_cluster(PublicationTarget) -> Result<Self, String>`
  (validates the target; reads `/var/run/secrets/kubernetes.io/serviceaccount/ca.crt`;
  token file `…/token` re-read per request; base `https://kubernetes.default.svc`;
  no redirects; 4 s timeout). Implements `RuntimePublication`:
  - `current()` — three `GET`s → `HeldDocuments::revisions()`.
  - `publish(&RuntimeSnapshot)` — three `GET`s remembering `resourceVersion`
    → `plan_publication` → every object built and size-checked (1 MiB cap,
    `Unwritable`) before any write → `PUT`/`POST` per `Written` document in
    the order data sources, catalogue, tenants.
  - `describe()` — the namespace, nothing else.
  - `#[cfg(test)] with_client(Client, &str)`.

## Modules

- `client.rs` — `Client { http, base, token_file }`, `Found::{Object, Absent}`,
  `get(path, document)`, `write(path, &ConfigMap, document)` (`PUT` when the
  object carries a `resource_version`, else `POST`); status → error words:
  401/403 not permitted, 404 absent (reads only), 409 another writer, 422
  refused shape; 2 MiB response cap. Every error is `Unreadable`/`Unwritable`
  naming the document; never a body, URL or token.
- `held.rs` — `Reads { tenants, data_sources, catalog: Read { held: HeldDocument, resource_version } }`,
  `Reads::fetch`, `Reads::held() -> HeldDocuments`; a manifest key that will
  not parse or names another document is `Unreadable`; a wrong object name
  in the answer is `Unreadable`.
- `object.rs` — `Keys::of(kind)` (the two `data` keys from the wire's file
  constants), `name_of(kind)` (`fabric-runtime-tenants` / `-data-sources` /
  `-catalog`), `collection_of(ns)`, `path_of(ns, kind)`, `object_for(ns, kind, &DocumentPlan, resource_version)`
  (both keys, `MANAGED_BY_LABEL`, the cap `OBJECT_CAP`).
- `wire.rs` — `ConfigMap { metadata: Metadata, data }`, `Metadata { name, namespace, labels, resource_version }`
  (camelCase on the wire), `MANAGED_BY_LABEL`.
- `errors.rs` — `Transport(String)`; `unreadable(kind, detail)`, `unwritable(kind, detail)`.
- `testing.rs` (test-only) — `fake(responses)` local-socket API server that
  records request lines and bodies; `snapshot(revision)`; `object(name, version, data)`.

## Hard invariants

- Decides nothing: every refusal is `plan_publication`'s. Do not add a rule
  here; add it to `fabric-runtime-publication` where both adapters get it.
- Object names are constants; only the namespace is configuration, and it is
  validated before it is a path segment. No document value reaches a path.
- Build every object before writing any; never split a document across
  objects; never `delete`.
- Write order is the plan's field order: data sources, catalogue, tenants.
- A replace carries the `resourceVersion` it was read at; a `409` is
  `Unwritable`, never retried inside one publication.
- Errors carry no response body, URL or credential.
- `scripts/check_architecture.py`: only this crate may name the ConfigMap
  path or object names; `resource_version` is shared with
  `fabric-deployment-kubernetes` alone.
