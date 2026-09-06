# fabric-ndc-acceptance — LLM context

Test-only crate, in neither plane, holding the composed NDC connector
acceptance test for issue #62. No `[dependencies]`; `[dev-dependencies]`
only. `publish = false`.

## Public surface

None. `src/lib.rs` is a doc comment and nothing else -- no types, no
functions, no re-exports. This slice adds the crate boundary and the
architecture-gate amendment that admits it; the acceptance test under
`tests/` is a later slice of issue #62.

## Dev-dependencies (all `workspace = true`)

- `fabric-runtime-publication` -- publishes the fixture through the real
  filesystem adapter.
- `fabric-connector-ndc` -- the real adapter under test, executing against a
  running `ghcr.io/hasura/ndc-postgres` process.
- `fabric-tenant-runtime` -- the real `build_runtime` reading the published
  fixture.
- `fabric-data-api` -- the real `build_data_api` router served over it.
- `fabric-identity` -- mints bearer tokens for the assembled router, as
  `fabric-runtime-publication`'s own composed test already does.
- `fabric-connector` -- the neutral `DataConnector` trait the NDC adapter
  implements; needed to reason about the executed operation independent of
  the wire protocol.
- `axum`, `http`, `tokio`, `tower` (feature `util`, matching
  `fabric-data-api`'s own dev-dependency, for `tower::ServiceExt::oneshot`),
  `serde_json` -- drive and inspect the assembled router the same way
  `fabric-data-api`'s and `fabric-runtime-publication`'s own tests do.

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

## Invariants to preserve

- No `[dependencies]` section, ever. If this crate needs code beyond
  documentation, that is a sign the composed acceptance test has grown
  production-shaped logic that belongs in one of the crates it tests
  instead.
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
