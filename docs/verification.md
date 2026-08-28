# Verification

What was measured, what it showed, and where the numbers came from. Every
command below is reproducible from the repository root; nothing here is
asserted without one.

Last run: 2026-08-28, against `claude/saas-fabric-control-plane-keycloak-7ac835`,
covering both planes.

The runtime plane's numbers come from the same tree as the previous run
(2026-08-14, after six rounds of adversarial review; the last two ran narrow
independent lenses rather than one generalist pass and between them found
nineteen blocking defects — more than the four generalist rounds before them
combined). Nothing in the runtime plane changed for this increment, which is
itself checked: the architecture script now fails if a control-plane crate
appears anywhere in the runtime graph.

The control plane's numbers are new. What was verified beyond the gates below
is at the end, under "The control plane, end to end".

## Gates

| Gate | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | clean |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | 0 findings |
| Tests | `cargo test --workspace` | 1155 passing, 0 failing, 1 ignored |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 0 warnings |
| Dependencies | `cargo deny check` | advisories, bans, licences, sources — all ok |
| File sizes | `python3 scripts/check_file_sizes.py` | 0 over the 150-line limit |
| Architecture | `python3 scripts/check_architecture.py` | 8 invariants hold across 13 crates |
| Console lint | `npm run lint` | 0 findings |
| Console types | `npm run typecheck` | 0 errors |
| Console tests | `npm test` | 17 passing, 0 failing |
| Console build | `npm run build` | 202 kB, 63 kB gzipped |

All eleven run in CI on every push and pull request
(`.github/workflows/ci.yml`); the four Rust gates and the architecture check as
parallel jobs, the four console checks as steps of one job because `npm ci`
dominates each of them.

The single ignored test is a `#[doc(ignore)]` example in `fabric-identity`'s
extractor, and predates this work.

The workspace total rose from 977 to 1155: **178 new Rust tests**, plus the
console's 17, which run separately.

| Crate | Tests | What they pin |
| --- | --- | --- |
| `fabric-client-model` | 34 | the document format, preservation, every name's rule |
| `fabric-reconciliation` | 24 | the diff, idempotence, the status state machine |
| `fabric-control-plane` | 63 | the API contract, concurrency, the operator seam, boundaries |
| `fabric-keycloak` | 18 | the admin protocol, over a real socket |
| `fabric-client-git` | 24 | optimistic concurrency, over a real socket |
| `fabric-control-plane-api` | 15 | configuration, secrets, the shipped examples |
| `control-plane-ui` | 17 | the API client, the badge, the role editor |

## File sizes

474 Rust source files under `crates/*/src`, of which 156 are the control
plane's. The policy (`docs/architecture/file-size-policy.md`) treats 150
production lines as a hard limit and 120 as advisory; test lines never count,
whether they live in a sibling `*_tests.rs` or in a trailing `#[cfg(test)]`
module.

- **Over 150 lines: none.** The exemption list is still empty.
- **Over 120 lines: 52 files**, largest 150. Eight of the 52 are in the new
  crates, largest 131.

Two control-plane files reached the limit while being written and were split
rather than exempted, and both splits are worth recording because they are the
same split:

| Was | Became |
| --- | --- |
| `fabric-client-git/src/github/http.rs` (236) | `http.rs` (how a request is made) + `operations.rs` (what the operations are) + `decoding.rs` |
| `fabric-keycloak/src/admin/http.rs` (179) | `http.rs` (client, token, bearer) + `requests.rs` (the four operations and how each status is read) |

Neither fragments a type across files: in both cases the struct kept its own
file and one impl block moved, which is the convention `config::loading` and
`config::validation` already follow in the runtime host.

The advisory band grew from 20 files to 52, and the reason is the same as it
was: rustdoc. Every file in the band is a type or a function set whose prose
outweighs its code — `redirect_uri.rs` is 130 lines for a newtype over a
`String`, and most of that is the argument for why a wildcard in the host is
refused. Splitting prose away from the thing it explains would satisfy the
counter and make the code worse.

The console's 17 source files are held to the same 150-line limit by ESLint's
`max-lines`; none is close.

## Dependency licences

208 packages in the resolved graph — 195 third-party and 13 of this
workspace's own — every one carrying an OSI-approved permissive licence. `deny.toml`'s `exceptions` list is empty, and its `allow`
list is the set of licences actually present — not a set approved in
principle, so an unmatched entry never sits there as noise.

| Count | Licence |
| --- | --- |
| 107 | MIT OR Apache-2.0 |
| 35 | MIT |
| 18 | Unicode-3.0 |
| 14 | Apache-2.0 |
| 10 | Apache-2.0 OR MIT |
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

The control plane added exactly two third-party crates, both verified per crate
and per version: `serde_norway` 0.9.42 (MIT OR Apache-2.0) and
`unsafe-libyaml-norway` 0.2.15 (MIT). See
`docs/architecture/dependency-policy.md`. Notably it added **no** Git library
and **no** Kubernetes client — the desired-state adapter speaks its host's
contents API over the `reqwest` that was already in the graph.

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
fabric-core            (no internal dependencies; the only crate both planes share)

  runtime plane
fabric-identity        → core
fabric-connector       → core
fabric-tenant-runtime  → core, connector
fabric-connector-ndc   → core, connector, tenant-runtime
fabric-data-api        → core, identity, tenant-runtime, connector
fabric-api             → all of the above  (composition root)

  control plane
fabric-client-model    → core
fabric-reconciliation  → core, client-model
fabric-control-plane   → core, client-model, reconciliation
fabric-keycloak        → core, client-model, reconciliation      (implements the port)
fabric-client-git      → core, client-model, control-plane       (implements the port)
fabric-control-plane-api → all of the above  (composition root)
```

Also checked structurally, because none of these can be caught by a test:

- **The two planes do not meet.** No crate in either plane depends on a crate
  in the other. This is the increment's central structural claim: the runtime
  plane must keep serving tenants while Git and Keycloak are unreachable, and
  one edge would put control-plane availability behind every tenant request.
- **Keycloak representations stay in `fabric-keycloak`, and Git-hosting
  details in `fabric-client-git`.** Checked as vocabulary, not just as
  dependency edges — `*Representation`, `publicClient`, `openid-connect`,
  `ContentsEntry`, `PutContents`, `contents/` may not appear anywhere else.
  Only the control plane's composition root may depend on either crate.
- **The operator console reaches only the control-plane API.** No file under
  `apps/control-plane-ui/src` may name `client_secret`, `/admin/realms`, the
  Git host's API, or a Keycloak admin endpoint. Checked as a property of the
  console's own source rather than of what a response happened to contain,
  because that is the form the rule takes: a fetch to another origin is the
  violation, whether or not a credential is in the same commit.

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
- **No Kubernetes or Git client anywhere in the graph** — still, and now
  including the control plane, which is where somebody would reach for one.
  `fabric-client-git` speaks its host's contents API over HTTPS, so the
  platform needs no clone, no working copy, and no disk. §6 keeps the control
  plane out of the request path, and the strongest form of that is a client
  that is not linked into any binary this workspace builds.
- **`X-Tenant-Id` appears only where it is rejected**, in `fabric-identity`
  and in tests asserting the rejection.

## The control plane, end to end

Beyond the gates, the vertical slice was exercised against a running process
and a real browser, because the properties that matter most are properties of
a *sequence* and no unit test observes them together.

`cargo run -p fabric-control-plane-api -- examples/control-plane.toml`, the
console at `localhost:5173`, and:

| Checked | Observed |
| --- | --- |
| The console lists clients from the desired-state source | Both example clients, by display name |
| Identity is shown with reconciliation state | Realm, both required roles, the `web` application, `applied` |
| A write goes to desired state, not to the provider | The badge changed to `pending` on save |
| Reconciliation then converges | `applied` on the next read, with a fresh timestamp |
| Optimistic concurrency | `409 revision_conflict` on a second write at the same revision |
| A write with no precondition | `428 revision_required` |
| A realm change | `400 realm_immutable` |
| No operator identity | `401` on every route except `/health` |
| The mutation is attributable | One audit event naming the operator, the client, the operation, and the resulting revision |

The console was also checked at 375 px, where the layout stacks rather than
scrolling sideways.

## What is not verified

Named here rather than left for a reader to discover.

**Control plane:**

- **No test against a real Keycloak.** The adapter is tested against a socket
  that answers like one — which pins the paths, the bodies, the bearer, and how
  `404` and `409` are read — but not against Keycloak itself. The same gap the
  runtime plane has with NDC, and the same lesson applies: the failures that
  survive this kind of testing are the ones where our requests are well-formed
  and our assumptions about the other side are wrong. The specific assumptions
  worth checking first are that a realm update applies only the fields it is
  given, and that `POST /admin/realms/{realm}/roles` answers `409` rather than
  `400` for a role that exists.
- **No test against a real Git host.** Likewise. The concurrency mechanism is
  tested against a stateful fake that moves blob hashes and refuses stale ones,
  which is what the contents API does — but "the host answers `409` for a stale
  `sha`" is read from its documentation, not observed.
- **Reconciliation status is process-local and lost on restart.** Safe, because
  reconciliation is idempotent and re-observes every client within one sweep.
  What is genuinely lost is history: that a client was `drifted` an hour ago.
- **Listing costs one request per client.** Fine at tens of clients; not
  measured beyond that, and the fix when it stops being fine is a different API
  rather than a tweak.
- **Operator authorisation is coarse.** Every authenticated operator may do
  everything the API offers. Not a gap in the implementation — there is no
  per-client permission model to implement yet, and ADR 0009 says so rather
  than implying otherwise.
- **`saas-fabric-clients` does not exist yet.** The Git adapter has never
  addressed the repository it is written for. The document contract it expects
  is `docs/architecture/client-desired-state.md`, and the shipped examples
  conform to it under test.

**Runtime plane** (unchanged from the previous run):

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
