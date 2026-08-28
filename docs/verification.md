# Verification

What was measured, what it showed, and where the numbers came from. Every
command below is reproducible from the repository root; nothing here is
asserted without one.

Last run: 2026-08-14, against commit `HEAD` of
`claude/tenant-runtime-data-api-5ea0ca`, after six rounds of adversarial
review. The last two ran narrow independent lenses rather than one generalist
pass, and between them found nineteen blocking defects — more than the four
generalist rounds before them combined. Round six's lenses were NDC wire
conformance (checked against the published specification for the first time),
the write path end to end, and configuration and deployment.

## Gates

| Gate | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | clean |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | 0 findings |
| Tests | `cargo test --workspace` | 977 passing, 0 failing |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 0 warnings |
| Dependencies | `cargo deny check` | advisories, bans, licences, sources — all ok |
| File sizes | `python3 scripts/check_file_sizes.py` | 0 over the 150-line limit |
| Architecture | `python3 scripts/check_architecture.py` | 5 invariants hold across 7 crates |

All seven run in CI on every push and pull request
(`.github/workflows/ci.yml`), as parallel jobs.

## File sizes

240 Rust source files under `crates/*/src`. The policy
(`docs/architecture/file-size-policy.md`) treats 150 production lines as a
hard limit and 120 as advisory; test lines never count, whether they live in
a sibling `*_tests.rs` or in a trailing `#[cfg(test)]` module.

- **Over 150 lines: none.** The exemption list is empty.
- **Over 120 lines: 20 files**, largest 146.

The largest ten:

| Lines | File |
| --- | --- |
| 146 | `crates/fabric-connector/src/execution/execution_target.rs` |
| 145 | `crates/fabric-connector-ndc/src/registration.rs` |
| 143 | `crates/fabric-connector-ndc/src/wire/query.rs` |
| 139 | `crates/fabric-connector/src/filter/filter_expression.rs` |
| 138 | `crates/fabric-connector-ndc/src/config/connector_config.rs` |
| 136 | `crates/fabric-api/src/startup/connectors/pending_connector.rs` |
| 136 | `crates/fabric-connector-ndc/src/translate/response.rs` |
| 136 | `crates/fabric-data-api/src/models/list_query.rs` |
| 135 | `crates/fabric-connector/src/capabilities.rs` |
| 135 | `crates/fabric-connector-ndc/src/connector.rs` |

These twenty are in the "needs a clear reason" band rather than the failing
one, and the reason is the same in most cases: they carry a lot of rustdoc.
`execution_target.rs` is 146 lines for a seven-field struct, and most of that
is the enumeration of what the type deliberately does *not* carry. Splitting
prose away from the thing it explains would satisfy the counter and make the
code worse, so it has not been done.

## Dependency licences

200 packages in the resolved graph, every one carrying an OSI-approved
permissive licence. `deny.toml`'s `exceptions` list is empty, and its `allow`
list is the set of licences actually present — not a set approved in
principle, so an unmatched entry never sits there as noise.

| Count | Licence |
| --- | --- |
| 106 | MIT OR Apache-2.0 |
| 34 | MIT |
| 18 | Unicode-3.0 |
| 10 | Apache-2.0 OR MIT |
| 8 | Apache-2.0 |
| 3 | Apache-2.0 OR ISC OR MIT |
| 3 | ISC |
| 3 | MIT/Apache-2.0 |
| 2 | Apache-2.0/MIT |
| 2 | MIT OR Apache-2.0 OR Zlib |
| 2 | Unlicense OR MIT |
| 2 | Zlib OR Apache-2.0 OR MIT |
| 1 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| 1 | Apache-2.0 AND ISC |
| 1 | Apache-2.0 OR BSL-1.0 |
| 1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| 1 | BSD-3-Clause |
| 1 | MIT AND BSD-3-Clause |
| 1 | MIT OR Apache-2.0 OR LGPL-2.1-or-later |

Three entries deserve a note, because each looks worse at a glance than it is:

- **`Apache-2.0 OR BSL-1.0`** (`ryu`) — BSL-1.0 is the **Boost** Software
  Licence, OSI-approved and permissive. It is not the Business Source
  Licence, which is BUSL-1.1 and is banned. The disjunction takes Apache-2.0
  regardless.
- **`MIT OR Apache-2.0 OR LGPL-2.1-or-later`** (`r-efi`) — a disjunction. MIT
  is taken; the LGPL branch is never exercised.
- **`Apache-2.0 AND ISC`** (`ring`) — a conjunction, both allowed. `ring`
  declares this in its own manifest, so no `cargo-deny` clarification is
  needed. (An earlier draft of `deny.toml` carried one asserting
  `MIT AND ISC AND OpenSSL`; that is not what version 0.17.14 carries, and
  removing it changed nothing, which is how the mistake surfaced.)

**What the audit changed.** `reqwest`'s default `rustls-tls` feature pulls in
`webpki-roots`, which carries CDLA-Permissive-2.0 — permissive, but a Linux
Foundation *data* licence and not OSI-approved. Rather than write an
exception for it, the dependency was removed in favour of
`rustls-tls-native-roots`. ADR 0005 records the reasoning and the cost.

The `ndc-models` finding that predates this work is in ADR 0001: the crate
that publishes the NDC specification types has **no licence at all**, which
is why this workspace hand-writes the wire subset and speaks to connectors
over HTTP rather than linking anything.

## Crate dependency graph

Verified by `scripts/check_architecture.py` against
`docs/architecture/crate-dependencies.md`; a new edge that is not in the
document fails CI.

```
fabric-core            (no internal dependencies)
fabric-identity        → core
fabric-connector       → core
fabric-tenant-runtime  → core, connector
fabric-connector-ndc   → core, connector, tenant-runtime
fabric-data-api        → core, identity, tenant-runtime, connector
fabric-api             → all of the above  (composition root)
```

Also checked structurally, because none of these can be caught by a test:

- **NDC vocabulary stays in `fabric-connector-ndc`.** `fabric-api` may name
  exactly two symbols from it — `NdcConnectorConfig` and
  `build_ndc_connector`, both startup wiring. Nothing else in the workspace
  may name an `Ndc*` type at all. Prose is exempt: several crates explain the
  boundary without being permitted to cross it.
- **No transport in the domain crates.** `fabric-core`, `fabric-connector`
  and `fabric-tenant-runtime` declare no HTTP client or server.
  `fabric-identity` does depend on Axum, deliberately — see the dependency
  document for why that is the crate's job rather than a leak.
- **No database driver anywhere in the graph.** Checked against the full
  resolved set, not just direct declarations — a driver arriving transitively
  compiles into the binary exactly as much as one declared directly, and no
  manifest here would mention it. The runtime plane opens no
  database connections; every physical connection lives inside a connector
  process.
- **No Kubernetes or Git client anywhere in the graph.** §6 keeps the control
  plane out of the request path, and the strongest form of that is a client
  that is not linked at all.
- **`X-Tenant-Id` appears only where it is rejected**, in `fabric-identity`
  and in tests asserting the rejection.

## What is not verified

Named here rather than left for a reader to discover.

- **No connector integration test.** Nothing in this workspace has spoken to
  a running NDC connector, and round six showed what that costs. Two of its
  findings were invisible to every unit test because our requests were
  well-formed and our logic correct: a connector that declares no
  request-level arguments silently ignores the per-tenant routing we send
  (verified against `ndc-postgres` v3.1.0's source, where `Static => None`
  and `acquire` returns one pool regardless), and `affected_rows` is not an
  NDC concept on `/mutation` at all, so the count we report is a heuristic
  read of a connector-private result shape. Both are now checked at startup
  or refused, but neither was findable from inside this workspace.

  ADR 0004 carries the remaining pre-deployment checklist. The item that
  matters most: the payload argument's expected **value shape** is still
  documentation-derived, because the NDC schema does not describe it.

- **No exactly-once write guarantee.** The platform now distinguishes a write
  that provably did not reach the backend from one whose outcome is unknown
  and one that was applied but whose result was lost, and reports each with a
  different status and machine code. That stops the platform *instructing* a
  retry of a write that may have landed; it does not stop a client or a mesh
  taking one — Envoy's `gateway-error` policy retries 502 and 503 regardless.
  Closing it needs an idempotency key and a durable store the Data API does
  not have. `fabric-data-api/docs/README.md` states the promise and names the
  gap.
- **No load or concurrency testing beyond the registry.** The atomic-swap
  behaviour has a multi-threaded test; the HTTP surface under concurrent load
  does not.
- **`IsolationModel::Schema` is safe but inert.** ADRs 0006 and 0007 closed
  the configurations that made it dangerous; neither made per-tenant schema
  routing work. On a destination no other tenant reaches, the variant behaves
  exactly like `Database`, and `schema()` still has no production caller. Its
  rustdoc says so, rather than implying otherwise.

- **Destination identity is configuration equality only.** ADR 0007's
  co-tenancy rule treats two differently-named connections as two
  destinations even when they reach one database, and two `SecretRef`s as
  two even when they resolve to one credential. Closing that needs a
  connector round trip on the request path, which §6 forbids.

- **Pagination determinism is the caller's responsibility.** The Data API
  cannot verify that a caller's sort is unique for a given collection, so it
  does not pretend to. Documented in `fabric-data-api`'s README.
