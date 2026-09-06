# fabric-ndc-acceptance — LLM context

Test-only crate, in neither plane, holding the composed NDC connector
acceptance test for issue #62. No `[dependencies]`; `[dev-dependencies]`
only. `publish = false`.

## Public surface

None. `src/lib.rs` is a doc comment and nothing else -- no types, no
functions, no re-exports. The composed acceptance test and its harness live
entirely under `tests/`.

## Tests

- `tests/published_state_reaches_a_real_connector.rs` (slice 4) -- the
  composed test. Publishes a fixture (one shared, discriminator-isolated
  DataSource; two tenants; a one-resource `articles` catalogue) through the
  real `FilesystemRuntimePublication`, negotiates the real
  `fabric_connector_ndc::build_ndc_connector` against a running
  `ghcr.io/hasura/ndc-postgres:v3.1.0`, builds the real
  `fabric_tenant_runtime::build_runtime` and
  `fabric_data_api::build_data_api` over it, and drives the router with
  `tower::ServiceExt::oneshot`. Eleven tests: two-tenant isolation on both
  the list and keyed routes, a direct `psql` proof both physical rows exist,
  the `x-tenant-id` header refusal, no response naming the connector id or
  the discriminator, a connector refused for declaring no routing argument,
  an HTTP impostor (nginx) refused as malformed rather than believed, a
  stopped connector answering `503`, the version-floor handshake, and a real
  insert reporting the connector's own `affected` count. A twelfth,
  `a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives`,
  is not implemented -- see "F3" below.
- `tests/the_stack_comes_up.rs` (slice 3) -- the container harness's own
  proof it comes up and answers, one layer below the composed test.

Every test calls `support::gate::docker_available_or_skip` first.

## Dev-dependencies (all `workspace = true`)

- `fabric-runtime-publication` -- publishes the fixture through the real
  filesystem adapter.
- `fabric-connector-ndc` -- the real adapter under test, executing against a
  running `ghcr.io/hasura/ndc-postgres` process.
- `fabric-tenant-runtime` -- the real `build_runtime` reading the published
  fixture.
- `fabric-data-api` -- the real `build_data_api` router served over it.
- `fabric-identity` -- `build_identity`, `IdentityConfig`, `TokenReader`,
  `TokenClaims`, `encode_unsigned_token`. **Not** `TrustedIngressReader`: it
  needs an `Arc<dyn fabric_core::Clock>`, and this crate has no
  `fabric-core` dependency (see `tests/support/unsigned_reader.rs`).
- `fabric-connector` -- the neutral `DataConnector` trait and `ConnectorId`,
  needed to register the adapter and reason about the executed operation
  independent of the wire protocol.
- `axum`, `http`, `tokio`, `tower` (feature `util`, for
  `tower::ServiceExt::oneshot`), `serde_json` -- drive and inspect the
  assembled router the same way `fabric-data-api`'s and
  `fabric-runtime-publication`'s own tests do.
- `base64` -- decodes the payload segment of this crate's own unsigned test
  tokens (`tests/support/unsigned_reader.rs`); already pinned in the
  workspace root and already in `Cargo.lock` (`fabric-identity` uses it for
  the same encoding internally), so this is not a new external dependency to
  verify.

**No `fabric-core` dependency, deliberately.** `scripts/check_architecture.py`'s
`expected` table for this crate does not list one, and adding it is a
`scripts/` change this issue does not make. Two consequences worth knowing
before touching `tests/support/`:

- `fixtures.rs` builds every `fabric-runtime-publication` document
  (`DataSourceDocument`, `TenantBindingDocument`, `CatalogDocument`,
  `ResourceDefinitionDocument`) from a `serde_json::json!` literal via
  `serde_json::from_value`, never from a struct literal -- those types'
  fields are typed in `fabric_core` identifiers (`DataSourceId`, `TenantId`,
  `BindingRevision`, ...) this crate cannot name.
- `unsigned_reader.rs`'s `UnsignedTokenReader` implements
  `fabric_identity::TokenReader` directly, decoding exactly the wire format
  `fabric_identity::encode_unsigned_token` produces, without a signature or
  expiry check. It is not a production posture; it exists only because this
  crate cannot implement `fabric_core::Clock` to construct a
  `TrustedIngressReader`.

## `tests/support/` layout

- `docker/` -- `process.rs` (drives the `docker` binary, knows nothing about
  containers or networks), `containers.rs`, `networks.rs`, `polling.rs`
  (deadline-bounded, never a bare sleep). `docker.rs` is a thin facade
  re-exporting all four, so every other file's `docker::foo(...)` calls are
  unaffected by which file `foo` actually lives in.
- `gate.rs` -- `docker_available_or_skip` and `REQUIRE_ENV`
  (`FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE`). Also gates the digest-fallback in
  `docker/containers.rs`: set to `1`, a pinned image absent locally is a
  failure naming the digest, never a silent bare-tag substitution.
- `images.rs` -- every image, pinned by digest, in one place.
- `names.rs` -- `RunId` (per-run container/network naming) and
  `sweep_stale` (removes a prior hard-killed run's leftovers by the
  `fabric-ndc-acc-` prefix).
- `postgres.rs` -- starts postgres, seeds `SEED_SQL` (the shared `articles`
  table, as SQL literals).
- `connector.rs` -- starts `ndc-postgres` in `ConnectorMode::Static` or
  `::Named`, mounting the checked-in `tests/fixtures/ndc-postgres-v3.1.0/`
  configuration.
- `stack.rs` -- `Stack::up` assembles network + postgres + connector;
  `Drop` tears down in reverse; `stop_connector` simulates a mid-run outage.
- `impostor.rs` -- `Impostor::start`, a self-contained nginx (its own
  network, cleaned up on drop) reconfigured to answer `200` on every path,
  standing in for "a real HTTP process that is not an NDC connector."
- `tempdir.rs` -- the hand-rolled `TempDir`, deliberately not shared with
  `fabric-runtime-publication`'s own (this crate must not depend on that
  crate's `tests/`-local modules).
- `fixtures.rs` -- the publication fixture (see "No `fabric-core`
  dependency" above for why it is built from JSON).
- `requests.rs` -- token/request/response helpers, deliberately duplicated
  from `fabric-runtime-publication`'s own `tests/support/requests.rs` rather
  than shared.
- `unsigned_reader.rs` -- `UnsignedTokenReader` (see "No `fabric-core`
  dependency" above).
- `compose.rs` -- `compose()`: publishes a fixture and assembles the real
  runtime and Data API over an already-negotiated connector. Takes the
  connector as a parameter rather than negotiating it internally, because
  several tests need negotiation itself to fail.

## Why this crate is in neither plane

Three checks in `scripts/check_architecture.py` block every other
placement; the full argument, with exact line numbers, is in `src/lib.rs`'s
module doc comment. Summary:

1. `check_ndc_containment`'s source scan (`:323`, loop at `:328-349`)
   forbids any crate but `fabric-connector-ndc` (and, narrowly,
   `fabric-api` via `NDC_NAMES_THE_HOST_MAY_USE`, `:109-114`) from naming an
   NDC type anywhere under the crate, `tests/` included.
2. The same function's dependency-edge loop (`:352-364`) forbids any crate
   but those two from declaring a dependency -- dev included -- on
   `fabric-connector-ndc`.
3. `check_runtime_plane_cannot_reach_the_publisher` (`:765-809`) forbids
   every `RUNTIME_PLANE` crate (`fabric-connector-ndc`, `fabric-data-api`,
   `fabric-api` among them) from reaching `fabric-runtime-publication` in
   any table, dev included.

No existing crate can dev-depend on both `fabric-runtime-publication` and
`fabric-connector-ndc` without tripping one of these. `fabric-ndc-acceptance`
is admitted by name in both loops of `check_ndc_containment`
(`NDC_ACCEPTANCE_CRATE`) instead of widening either gate, and is
deliberately **not** added to `DOMAIN_CRATES`, `RUNTIME_PLANE`, or
`CONTROL_PLANE` -- nothing in either plane depends on it, so ADR 0018's
publisher fence (`check_runtime_plane_cannot_reach_the_publisher`,
`check_plane_reachability_is_transitive`) holds exactly as before.

## F3: what this crate does not, and cannot yet, prove

`a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives` is
named in the issue #62 plan and not implemented. The real
`delete_articles_by_id_and_tenant_key` and
`update_articles_by_id_and_tenant_key` procedures require `key_id` and
`key_tenant_key` arguments alongside their predicate, and
`fabric_connector_ndc::CollectionProcedures` has nowhere to carry a required
key argument -- a neutral `MutationSpec::Delete { filter }` or
`MutationSpec::Update` cannot be expressed against this connector's
generated procedures as they stand. This is out of scope for this crate: it
needs a production change in `fabric-connector-ndc`, tracked as its own
follow-up issue that supersedes ADR 0004 rather than amending it further.
`docs/verification.md`'s "Connector acceptance (issue #62)" section records
the same deferral as "F3."

## Invariants to preserve

- No `[dependencies]` section, ever. If this crate needs code beyond
  documentation, that is a sign the composed acceptance test has grown
  production-shaped logic that belongs in one of the crates it tests
  instead.
- No `fabric-core` dev-dependency either, unless `scripts/check_architecture.py`'s
  `expected` table is updated in the same change to admit it. See "No
  `fabric-core` dependency" above for the two places that constraint is
  already worked around.
- Do not add this crate to `DOMAIN_CRATES`, `RUNTIME_PLANE`, or
  `CONTROL_PLANE` in `scripts/check_architecture.py`. It exists specifically
  because it belongs to none of them.
- Do not let anything outside this crate declare a dependency on it. A
  crate in either plane depending on `fabric-ndc-acceptance` would put a
  test-only crate in a production dependency graph, which is backwards --
  and, for a `RUNTIME_PLANE` crate specifically, would reintroduce exactly
  the publisher-reachability path `check_runtime_plane_cannot_reach_the_publisher`
  exists to refuse, since this crate itself reaches `fabric-runtime-publication`.
- Keep the `expected` entry in `scripts/check_architecture.py` to exactly
  this crate's real dev edges, the same discipline
  `docs/architecture/crate-dependencies.md` already asks of
  `fabric-runtime-publication`'s own table.
- Every container and network this harness starts is named under the
  `fabric-ndc-acc-` prefix (`names.rs`). Anything added to `support/` that
  starts a container must go through `docker::run` with a `RunSpec` built
  from a `RunId`, so `sweep_stale` can find it if a run is hard-killed.
