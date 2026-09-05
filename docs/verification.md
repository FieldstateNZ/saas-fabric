# Verification

What was measured, what it showed, and where the numbers came from. Every
command below is reproducible from the repository root; nothing here is
asserted without one.

Last run: 2026-08-29, against `claude/split-issuer-from-endpoints`,
covering both planes, on **Rust 1.98.0** — pinned in
[`rust-toolchain.toml`](../rust-toolchain.toml).

The version is recorded because it mattered. CI used to install `stable`
unpinned, and 1.98's `unused_async_trait_impl` failed this increment's pull
request on `fabric-identity`'s extractor — a file it had not touched. Both
extractors now return an already-complete future rather than being `async fn`,
which is a better statement of what they do, but the failure itself was
toolchain drift. `docs/architecture/toolchain-policy.md` records the pin and
the obligation that comes with it.

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
| Tests | `cargo test --workspace` | 1745 passing, 0 failing, 1 ignored |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 0 warnings |
| Dependencies | `cargo deny check` | advisories, bans, licences, sources — all ok |
| File sizes | `python3 scripts/check_file_sizes.py` | 0 over the 150-line limit (2 exempted, both explained) |
| Architecture | `python3 scripts/check_architecture.py` | 11 invariants hold across 21 crates |
| Console lint | `npm run lint` | 0 findings |
| Console types | `npm run typecheck` | 0 errors |
| Console tests | `npm test` | 81 passing, 0 failing |
| Console build | `npm run build` | 228 kB, 70 kB gzipped |

All eleven run in CI on every push and pull request
(`.github/workflows/ci.yml`); the four Rust gates and the architecture check as
parallel jobs, the four console checks as steps of one job because `npm ci`
dominates each of them.

Nothing is ignored.

The numbers in this table are from a run in the M1 worktree at `9e7c83b`
("Fail closed when a held manifest outlives its payload"), the commit that
adds `fabric-runtime-publication` and the two architecture invariants it
brings (see "Runtime publication (M1)" below) — newer than the
`claude/split-issuer-from-endpoints` run the rest of this document otherwise
describes. That is also why every count grew: 11 invariants across 21 crates,
not 8 across 14; 1745 Rust tests, not 1256; 81 console tests, not 35. None of
the growth outside `fabric-runtime-publication` is this slice's work — it is
the accumulated increments between the two runs — and none of it is
re-narrated here; only the table above and the new section below reflect the
later commit.

## Runtime publication (M1)

`fabric-runtime-publication` is new at `9e7c83b` (ADR 0018): the wire
contract for `tenants.json`, `data-sources.json` and `catalog.json`, the
`RuntimePublication` port, and the filesystem adapter that writes all three
atomically. It has no production caller yet — that is a control-plane crate
this decision names but does not build — so what is verified here is the
producer and its guards in isolation, plus one seam that proves the producer
and the existing runtime plane agree on the wire.

**The composed test.** `tests/published_state_serves_two_tenants.rs` is the
test `docs/delivery.md`'s rule exists for: it publishes a fixture — one
shared DataSource, two tenants isolated by different values in the same
discriminator column, a one-resource `articles` catalogue — through the real
`FilesystemRuntimePublication`, then builds the real
`fabric_tenant_runtime::build_runtime` over the real `JsonFileSource` and the
real `fabric_data_api::build_data_api` over the real `ResourceCatalog`
(deserialised straight from the published `catalog.json`), and drives the
assembled router with bearer tokens for both tenants. What it proves is the
vertical slice, not a layer: isolation is enforced by the predicate the
platform builds, not by which rows a fake happens to return, because the
recording connector in `tests/support/connector.rs` applies that predicate
to a small shared corpus instead of dispatching on tenant identity.

**The mutation experiments.** Ten mutations were run against this crate in
this worktree, following the standard above: mutate, run the named test
binary, record what failed, `git checkout --` the file, confirm
`git status --short` is clean, move on. No code change from this exercise
survives in this commit.

| # | Mutation | File | Test(s) that failed | First failure line |
| --- | --- | --- | --- | --- |
| 1a | Globex's discriminator value zeroed to `""` via the `GLOBEX_DISCRIMINATOR_VALUE` constant | `tests/support/fixtures.rs` | 7 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row` (at `9e7c83b` this left 13/13 green — see below) | `assertion \`left == right\` failed` on the captured predicate |
| 1b | Globex's discriminator value changed to `""` at the binding call site only, constant left alone | `tests/support/fixtures.rs` | 9 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row`, `each_call_reaches_the_connector_carrying_only_its_own_tenant_predicate` | `assertion \`left == right\` failed`<br>`left: Some(Compare { … value: String("") })`<br>`right: Some(Compare { … value: String("tenant-globex-915") })` |
| 2 | Both tenants given the same discriminator value | `tests/support/fixtures.rs` | 6 of 13 composed tests, incl. both named above | `assertion \`left == right\` failed`<br>`left: Some(Compare { … value: String("tenant-acme-482") })`<br>`right: Some(Compare { … value: String("tenant-globex-915") })` |
| 3 | Recording connector ignores the predicate, always returns the whole corpus | `tests/support/connector.rs` | 4 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row` (`each_call_reaches_…_tenant_predicate` still passed — it inspects the captured predicate, not the connector's answer) | `assertion \`left == right\` failed: {"data":[{"id":"1","title":"Acme Handbook"},{"id":"1","title":"Globex Playbook"}],…}`<br>`left: 2 right: 1` |
| 4 | Stale-revision compare inverted (`<` → `>`) | `src/verdict.rs` | `a_stale_revision_publication_is_refused_and_the_last_good_files_remain` in **both** integration binaries, plus `a_refused_publication_writes_nothing_at_all`, `an_emptying_publication_is_refused_unless_it_is_intended`, and 4 `verdict_tests` unit tests | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Unchanged, data_sources: Unchanged, catalog: Unchanged }` |
| 5 | Divergent-payload compare bypassed (`held_payload == incoming.payload` → `true`) | `src/verdict.rs` | `a_same_revision_publication_with_a_different_payload_is_refused` in **both** integration binaries, plus `verdict_tests::the_same_revision_with_different_bytes_is_refused_as_divergent` | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Unchanged, data_sources: Unchanged, catalog: Unchanged }` |
| 6 | Emptying guard disabled | `src/validate.rs` | `an_emptying_publication_is_refused_unless_it_is_intended` (composed) and `validate_tests::taking_tenants_from_non_empty_to_empty_without_intent_is_refused` (unit) — **the adapter's own integration suite (`tests/filesystem_runtime_publication.rs`) stayed fully green** | `called \`Result::unwrap_err()\` on an \`Ok\` value: ()` |
| 7 | Referential-integrity (dangling DataSource) guard disabled | `src/validate.rs` | `a_publication_naming_a_data_source_it_does_not_publish_is_refused_before_any_write` in **both** integration binaries, plus a `validate_tests` unit test | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Written, data_sources: Written, catalog: Written }` |
| 8 | Write order reversed: tenants before data sources | `src/filesystem/adapter.rs` | `a_data_source_is_written_before_the_tenant_that_references_it`, `a_publication_that_failed_between_documents_is_completed_by_the_next_one`, `the_temp_file_never_survives_a_publish_call_success_or_failure` | `assertion failed: !dir.path().join("tenants.json").exists()` |
| 9 | A held tenants manifest with a lost payload parses as empty again | `src/filesystem/parse.rs` | both `..._when_the_held_tenants_payload_is_lost` tests, plus a `filesystem::parse::tests` unit test | `called \`Result::unwrap_err()\` on an \`Ok\` value: []` |
| 10 | `rename` replaced with a direct write to the target | `src/filesystem/atomic_write.rs` | `the_temp_file_never_survives_a_publish_call_success_or_failure`, `a_publication_that_failed_between_documents_is_completed_by_the_next_one`, `the_manifest_is_written_after_the_payload_it_describes` | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Written, data_sources: Written, catalog: Written }` |

Two rows were findings at `9e7c83b`, and both are closed by follow-up commits
on the same branch:

- **Row 1a left everything green at `9e7c83b`**, the same class of mistake
  `docs/delivery.md` names: `GLOBEX_DISCRIMINATOR_VALUE` fed both the
  published binding (`tests/support/fixtures.rs`) and the recording
  connector's corpus (`tests/support/connector.rs`), so zeroing the constant
  moved both sides together and isolation held trivially. The corpus is now
  written as literals — the database's own truth, which must not follow the
  fixture — and the same mutation fails 7 of 13 (the row above records the
  rerun). Row 1b is the earlier form of the same mutation with the coupling
  broken by hand.
- **Row 6 showed the adapter-level suite had no direct coverage of
  `EmptyingNotIntended`**: its emptying-named tests removed the held payload
  first, so `HeldPayloadLost` fired before the guard was reached. The suite
  now has `a_populated_tenants_payload_with_no_manifest_still_guards_against_emptying`,
  which seeds a populated payload with no manifest, offers an empty tenants
  document without intent, and asserts the refusal and six unchanged files —
  the state the shipped `examples/` are in, and the one a first regression of
  the held-content reading had reopened.

Row 10 is the one decision 22 (ADR 0018, part 5) predicted might leave
nothing failing — atomicity was argued to remove "a spurious alarm and a
stale window, not a data-loss risk". It did not: three tests still catch it,
because the fixture obstructs a *sibling temp-file path* to force a write
failure, and removing the temp-file stage removes the obstruction's effect
along with it — the write that used to fail now succeeds outright, which
`the_manifest_is_written_after_the_payload_it_describes` and the other two
are built to notice. The prediction would only be borne out by a mutation
that leaves the temp-file staging in place and only removes the final
`rename`, which was not tried here.

## Acting on Keycloak as the operator

The count went **down**, from 1262 to 1252, and that is the change reporting
itself honestly. What went is a service account's credential and everything
that existed to manage it: the token cache, the `client_credentials` exchange,
and the invalidate-and-retry that a real Keycloak once forced. Tests for a
mechanism that no longer exists are not coverage.

Four tests were rewritten rather than deleted, because the property they
protected still matters under the new mechanism:

- every admin request carries **the operator's own bearer**, unchanged — an
  adapter substituting anything of its own would be the standing authority this
  removed;
- **no credential is ever exchanged for a token**, asserted by counting calls
  to the token endpoint and expecting zero;
- a refusal is **reported rather than retried**, in exactly one attempt,
  because there is no second authority to try;
- a request establishing no operator is refused, which is the router's property
  rather than any posture's.

**Not proven here, and it is the thing most likely to bite.** No test exercises
an operator whose Keycloak authority is `create-realm` alone. That case fails
on the *second* call against a real Keycloak — create the realm, then be
refused inside it by a token minted before the grant existed — and no fake
reproduces it, because the fake has no notion of grants landing in later
tokens. ADR 0012 records the requirement (master-realm `admin`); confirming it
needs LucentRoot.

## Connecting the integration: what these tests do and do not prove

Thirteen tests drive the flow end to end against a Git host that does what a
test tells it to. What they pin is the **ordering**, because that is what
turns a half-finished connection into one an operator can retry rather than one
only somebody with store access can repair.

- An installation that cannot mint a token is **not recorded**, and the
  application stays recorded so the operator can retry the install leg alone.
- A key that cannot be stored leaves **no record at all** — the key arrives
  exactly once, so a record written without it would describe an application
  this platform can never authenticate as, and no retry could fix it.
- A callback carrying a token this platform never issued establishes nothing.
- A creation callback **cannot be replayed**: the second presentation of the
  same token is refused.
- An installation reaching several repositories is recorded as undecided rather
  than guessed at, and the platform reports itself unconfigured until somebody
  says which.
- Choosing a repository the installation cannot reach is refused, so an
  operator working from a stale list cannot point the platform at something it
  cannot read.
- A stored integration is restored after a restart.
- An organisation name that could steer a URL — `../evil`, `a/b`, a name with a
  space — never reaches the URL a browser is handed.

Nine unit tests cover the correlation token itself, including that spending it
at the *wrong* leg still spends it: refusing without removing would leave a
token an attacker could probe against both callbacks until one accepted it.

Eight tests drive `fabric-openbao` over a real socket, which is where the
protocol mistakes live: reading a version 2 entry's double nesting, treating a
`404` as absence rather than failure, deleting through `metadata` so previous
versions of a private key do not survive, and logging in again when a token is
refused mid-lease.

**Not proven here.** No test has created a real GitHub App, redeemed a real
manifest code, or read a real OpenBao. The fakes answer the protocol these
adapters speak, which is what the Keycloak and Git adapters' own socket tests
established as the standard here — but the end-to-end run against LucentRoot is
still outstanding, and it needs the master realm configured first (ADR 0010).

## Starting with nothing connected

Seven integration tests drive the case that used to be impossible: a control
plane with no desired-state repository at all. They run against the real router
`build_control_plane` returns, so the operator extractor and the error mapping
are the deployed ones.

What they pin is that the platform *stays useful* in that state. Listing
clients answers `503 integration_not_configured` rather than a 500 or — worse —
an empty list, because a platform with no clients and a platform nobody has
connected look identical to an operator and only one of them needs somebody to
act. That response carries **no `Retry-After`**, because retrying will not
connect it; the two other failures that share 503 do carry one, which is why
that header is now decided by the error rather than by the status.

Two are worth singling out. `connecting_desired_state_takes_effect_without_a_restart`
binds a repository into a running control plane and asserts the next request is
served — the whole point of late binding, and something no restart-based test
would have caught. `the_integration_status_is_not_public` asserts an
unauthenticated caller gets `401` from the status endpoint: whether this
platform is connected, and to what, is reconnaissance.

Nine unit tests cover the status derivation, including the two cases that make
a status display trustworthy: a platform bound but not yet swept reports
`connected` rather than showing a fault for the first seconds after every
restart, and a *failing* integration still reports when it last worked.

## Operator sign-in: what these tests do and do not prove

Twenty new Rust tests cover the OIDC operator posture and seven cover the
console's half of the round trip. What they pin is worth being precise about,
because the gap matters.

**Proven here.** A token from another issuer, one issued to another client in
the same realm, one whose holder lacks the required role, an expired one, one
signed by a key the provider does not publish, and one naming a key id the
provider did not publish are each refused — and refused as *not an operator*
rather than as *no identity*, which is the distinction that decides whether the
console offers a sign-in or an error. A request with no token, or a malformed
`Authorization` header, is refused as missing. Before the first key set
arrives, everything is refused rather than accepted.

On the console side: a callback whose `state` this tab did not issue is refused
*before* anything is redeemed, a tab that started no sign-in refuses a callback
outright, the verifier is spent once so a second attempt fails, and the code is
cleared from the address bar so a reload cannot replay it.

**Not proven here.** These tests sign with HS256 against a fixture secret, so
that no private key exists in this repository. Production pins RS256 and the
algorithm is the one thing that differs; every decision under test is made by
the same code either way. What has *not* been exercised is a real realm: no
test has read a live JWKS document, redeemed a real authorization code, or
verified a token Keycloak actually issued.

That is the same distinction [the Keycloak adapter's](architecture/control-plane.md)
own tests draw, and the reason its 20 tests were worth running over a real
socket. The equivalent for this posture is a run against LucentRoot's master
realm once that realm has the console client and the operator role — which
nothing creates yet, deliberately (ADR 0010).

The workspace total rose from 977 to 1189 over the control-plane increment
(**212 new Rust tests**), to 1201 with operator sign-in, to 1218 with
late-bound desired state, and to 1262 with the connection flow — plus the
console's 35, which run separately.

Three of the 181 arrived after the merge, and are worth singling out because of
what they are: `an_edit_preserves_every_other_key_and_value`,
`an_edit_preserves_the_order_keys_were_written_in`, and
`an_edit_does_not_preserve_formatting`. The last pins a **limitation** rather
than a feature — comments, blank lines, quoting and flow style are all lost on
a round trip — because the documentation had claimed the opposite ("byte for
byte") in three places and nothing checked it. If a future parser starts
preserving any of them, that test fails and the prose is corrected in the same
change.

| Crate | Tests | What they pin |
| --- | --- | --- |
| `fabric-client-model` | 47 | the document format, what an edit preserves and what it does not, every name's rule, and the redirect-URI authority rule |
| `fabric-reconciliation` | 24 | the diff, idempotence, the status state machine |
| `fabric-control-plane` | 135 | the API contract, concurrency, both operator postures, late-bound desired state, the connection flow, boundaries |
| `fabric-keycloak` | 20 | the admin protocol over a real socket, including the refusal-retry a real Keycloak forced |
| `fabric-client-git` | 52 | optimistic concurrency, the GitHub App token exchange, and how a rejected or expiring token is replaced — all over a real socket |
| `fabric-control-plane-api` | 16 | configuration, secrets, the shipped examples |
| `fabric-openbao` | 8 | the KV protocol over a real socket, including the re-login a refused token forces |
| `control-plane-ui` | 35 | the API client, the badge, the role editor, the sign-in round trip, what an operator is told and offered about the integration |

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

## Real integration: LucentRoot

Everything above is this workspace testing itself. This section is different:
it records what has been run **against the real platform**, and it exists
because "tested against a protocol fake" and "known to work" are different
claims and were previously not distinguished.

Run on 2026-08-28 against LucentRoot — a single-node k3s cluster reached over
the operator tailnet — with **Keycloak 26.7.2**.

### Keycloak: proven

| Claimed | How |
|---|---|
| The adapter speaks the real admin protocol | A machine identity was created in `master` and reconciliation ran against it. No fake in the path. |
| A missing realm is created | Keycloak held only `master`; one sweep produced `acme`, enabled, display name `Acme`. |
| Required roles are created | `Client Realm Administrator` and `Client Realm User`, alongside the three Keycloak creates for itself. |
| A declared application client is created | `web`: public, `standardFlowEnabled`, `openid-connect`, the declared redirect URI, and **no secret field**. |
| Reconciliation is idempotent | `reconciliation.applying` fired exactly once across three sweeps while the observed timestamp kept advancing. |
| Drift is detected | `Client Realm User` was deleted directly through the admin API. The next sweep reported `drifted`, corrected it, and the sweep after that reported `applied`. |
| The narrow permission is sufficient | The identity holds `create-realm` and nothing else. See below. |

### Two things only the real instance could have told us

**`create-realm` is enough.** Keycloak grants a service account that creates a
realm the full administrative role set *for that realm*, on the corresponding
`<realm>-realm` client. So the identity earns authority over exactly what it
created. No master-realm administrator role, and no bootstrap debt.

**And it is granted into later tokens.** The first pass over a new client mints
a token, creates the realm, and is then refused inside it — holding a token
that is valid and was minted a moment too early. This failed a real
reconciliation before it was understood. `admin::requests` now discards the
cached token and retries once on `401` or `403`; `keycloak_adapter.rs` pins
both the retry and the fact that it happens only once.

Neither was findable from inside this workspace. Both are the reason this
section exists.

### Git: the adapter is proven, the deployment is not

The GitHub App path — signing an assertion, exchanging it for an installation
token, presenting the token rather than the key, and minting once rather than
per request — is tested against a real socket in `installation_tokens.rs`.

It has **not** run against GitHub, because the App does not exist yet: creating
one is a human action in an organisation's settings, and nothing in this
repository can perform it. Until then, `FieldstateNZ/saas-fabric-clients`
exists and holds Acme's document, and the control plane has read that document
through its real parser — but by way of the local-directory development
adapter, not the Git one.

So the honest split is:

| Component | Status |
|---|---|
| Keycloak adapter | **Real integration proven** |
| Desired-state document contract | **Real document proven** — the seeded file parses and serves |
| Git adapter | Protocol fake only; blocked on a GitHub App |
| Deployment through `saas-fabric-platform` | Not started |

### What the real document changed

The seeded Acme client declares `http://acme.lucentroot.internal/*`, and the
model refused it: `RedirectUri` permitted plain HTTP only on loopback.

LucentRoot's gateway has one listener, on port 80, with no TLS — because
ICANN resolved in July 2024 to withhold `.internal` from delegation
permanently, reserving it for private-use applications — so it cannot resolve
publicly or receive a trusted certificate. So the rule was wrong, not the
environment. Plain HTTP is now permitted on loopback *and* under `.internal`,
and `authority_tests.rs` pins the hostile cases a substring check would have
let through.

That is a defect the first real document found on its first read.

## What is not verified

Named here rather than left for a reader to discover.

**Control plane:**

- **The installation token's expiry is honoured but never observed.** The
  adapter reads GitHub's `expires_at` and caches to it, with a margin, a floor
  and a ceiling — all tested against a socket. What no test can show is that
  GitHub's stated expiry matches when the token actually stops working. The
  `401` retry is what covers the difference.
- **No test against a real Git host.** The concurrency mechanism is tested
  against a stateful fake that moves blob hashes and refuses stale ones, which
  is what the contents API does — but "the host answers `409` for a stale
  `sha`" is still read from documentation rather than observed. Blocked on the
  GitHub App; see "Real integration" above.
- **A realm update has not been observed.** Reconciliation created `acme` and
  has never had to change its display name, so the claim that Keycloak's realm
  update applies only the fields it is given — the reason `RealmUpdate` carries
  two — remains read rather than measured.
- **`409` on a duplicate role has not been observed.** The adapter treats it as
  success because the port requires idempotence, and the diff means it is
  rarely reached. Against real Keycloak the diff has always been right first.
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
