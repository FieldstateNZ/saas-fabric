# Verification

What was measured, what it showed, and where the numbers came from. Every
command below is reproducible from the repository root; nothing here is
asserted without one.

Last run: 2026-09-18, on `main` at `f4a39d1` (PR #73), covering both
planes, on **Rust 1.98.0** — pinned in
[`rust-toolchain.toml`](../rust-toolchain.toml) — on macOS aarch64.

The version is recorded because it mattered. CI used to install `stable`
unpinned, and 1.98's `unused_async_trait_impl` failed this increment's pull
request on `fabric-identity`'s extractor — a file it had not touched. Both
extractors now return an already-complete future rather than being `async fn`,
which is a better statement of what they do, but the failure itself was
toolchain drift. `docs/architecture/toolchain-policy.md` records the pin and
the obligation that comes with it.

The previous run recorded here was 2026-08-29, against
`claude/split-issuer-from-endpoints`. Two increments have landed since and
changed what is verified: keyed update and delete reaching a real
`ndc-postgres` procedure (issue #62's F3, closed by PR #71, ADR 0020) and
observed platform deployments from a live cluster (PR #72, ADR 0022). Both
are closed, both changed the crate graph, and both get their own short
section below, alongside "Connector acceptance (issue #62)". Most of the
rest of this document — the gates table's headline numbers aside — is the
accumulated record of runs before this one; each section below still states
its own date and commit, and only what changed for this pass is restated
here.

## Gates

| Gate | Command | Result |
| --- | --- | --- |
| Formatting | `cargo fmt --all --check` | clean |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | 0 findings |
| Tests | `cargo test --workspace --exclude fabric-ndc-acceptance` | 2098 passing, 0 failing, 1 ignored — 2090 on `f4a39d1`, plus the eight `crates/fabric-fga-auth-api/tests/example_configuration.rs` adds in the same change as this refresh |
| Docs | `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | 0 warnings |
| Dependencies | `cargo deny check` | advisories, bans, licences, sources — all ok |
| File sizes | `python3 scripts/check_file_sizes.py` | 0 over the 150-line limit (2 exempted, both explained) |
| Architecture | `python3 scripts/check_architecture.py` | 11 invariants hold across 23 crates |
| Console lint | `npm run lint` | 0 findings |
| Console types | `npm run typecheck` | 0 errors |
| Console tests | `npm test` | 143 passing, 0 failing, across 28 files |
| Console build | `npm run build` | `dist/assets/index-Bk9NSHzW.js`, 281.86 kB, gzip 83.58 kB |
| Connector acceptance | `cargo test -p fabric-ndc-acceptance` | not re-run locally for this pass (Docker-backed and out of scope for this doc refresh) — the last locally measured run is recorded below, dated, and is now stale on test count: PR #71 added four tests to `published_state_reaches_a_real_connector` (15, not 11) |
| Connector acceptance, required mode | `FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE=1 cargo test -p fabric-ndc-acceptance` | not run locally for this pass; passed in CI on this commit — GitHub Actions run [35196362764](https://github.com/FieldstateNZ/saas-fabric/actions/runs/35196362764) (workflow `CI`, 2026-09-17) |

Twelve of the thirteen rows above run in CI on every push and pull request
(`.github/workflows/ci.yml`): the four Rust gates, the dependency check
(`deny`), the file-size check, the architecture check, and the
connector-acceptance job as parallel jobs, the four console checks as
steps of one job because `npm ci` dominates each of them. The
`cargo test --workspace` job excludes `fabric-ndc-acceptance`
(`--exclude fabric-ndc-acceptance`) so the connector-acceptance job is the
one place that claim is made, with the requirement set — CI can go green on
that job only by actually reaching a real connector (`tests/support/gate.rs`).
That CI job **is** the "required mode" row; the "default mode" row above it
is not a separate CI job, and for this pass it is not a fresh local
measurement either — it stays here as a pointer to the last one, recorded
below with its own date, rather than asserting a number this pass did not
produce.

**Everything from here to "Nothing is ignored" below is the 2026-08-29
record, unchanged, and its test counts are now stale.** PR #71 added four
tests to `published_state_reaches_a_real_connector` — a cross-tenant keyed
delete, a same-key keyed delete, a keyed update, and the check that no write
response names a key argument or a procedure (see "Keyed writes reach
ndc-postgres" below) — so that binary now holds 15 tests, not 11 (30 with the
15 shared harness unit tests counted below, not 26), and the crate's
two-binary "N passing" total is 48, not 44. The mechanism this section
documents — the pull-deadline fallback, the per-binary resolution cache, the
container/network cleanup — has not changed and was not re-exercised for
this pass; it is left as the record of the run that proved it, not restated
as current.

**What "44 passing" is actually two different kinds of test.**
`cargo test -p fabric-ndc-acceptance` runs two integration binaries,
`published_state_reaches_a_real_connector` (26 tests) and
`the_stack_comes_up` (18 tests), and both now compile this crate's entire
`tests/support/` module tree — including its inline `#[cfg(test)]` unit
tests — because both files declare `mod support;`. Of the 26 and 18: 11 and
3 respectively (14 in total) are container-backed — the composed acceptance
test and the container harness's own smoke test — and call
`support::gate::docker_available_or_skip` first. On this run Docker was up,
so all 14 actually ran against real Docker rather than skipping as pass —
but "reached a running connector and postgres" is only true of 13 of them.
The 14th, `a_connector_that_answers_http_but_not_ndc_is_refused_rather_than_believed`,
deliberately starts only `support::impostor::Impostor` (a reconfigured
nginx) and never a connector or postgres at all — see "Connector acceptance
(issue #62)" below for what it proves instead. The remaining 15 in *each*
binary are harness unit tests — pure-function tests of
`tests/support/docker/image_reference.rs` and its sibling
`image_reference_tests.rs`, `tests/support/go_timestamp.rs`, and
`tests/support/names.rs` — which never call that gate and run
unconditionally, with or without Docker; that is the same 15 tests,
compiled and executed once per binary, not 30 distinct ones.

**This run: Docker up, default mode, real containers throughout.** At
commit `a3f3274` ("Bound the reader-join too, so a lingering grandchild
can't turn a clean exit into a hang" -- the code half of this closure pass,
whose bounded reader-join and richer timeout error are exactly what this
row needed re-measured against), with a real
Docker daemon reachable (`docker version` reports server `29.1.3`) and
`FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE` unset. Timed with `date +%s`
immediately before and after the whole invocation, 252 seconds apart:

```
$ cargo test -p fabric-ndc-acceptance -- --nocapture
     Running unittests src/lib.rs (target/debug/deps/fabric_ndc_acceptance-...)
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/published_state_reaches_a_real_connector.rs (target/debug/deps/published_state_reaches_a_real_connector-...)
# [elided: 26 individual `test ... ok` lines, several "has been running for
# over 60 seconds" progress notices, and the fallback stderr line quoted
# below -- all present in the real --nocapture output, reproduced here only
# as the summary line]
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 127.96s

     Running tests/the_stack_comes_up.rs (target/debug/deps/the_stack_comes_up-...)
# [elided the same way: 18 `test ... ok` lines, three `Stack::up` timing
# eprintln!s, and this binary's own copy of the fallback stderr line]
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 123.33s

   Doc-tests fabric_ndc_acceptance
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

The pinned `ghcr.io/hasura/ndc-postgres` digest still cannot be pulled on
this machine — Docker Desktop's registry route here goes through a proxy
that never answers, per `tests/support/images.rs` — but it no longer hangs
the suite the way it did before this round of review closed that finding:
each binary's own pull deadline (`image_reference::PULL_DEADLINE`, 120
seconds) fired exactly once, then fell back to the bare tag already present
locally under a different digest. Its stderr line, verbatim -- now carrying
the tail of the pull's own stderr alongside the deadline, per this pass's
fix to `process/deadline.rs`; empty here only because this particular hang
produced no output at all before the kill:

> fabric-ndc-acceptance: ghcr.io/hasura/ndc-postgres@sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57 could not be pulled within 120s (`docker pull ghcr.io/hasura/ndc-postgres@sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57` failed: did not complete within 120s and was killed; stderr so far: ); falling back to the bare tag ghcr.io/hasura/ndc-postgres:v3.1.0 (see images.rs for why)

That line appeared exactly twice in the whole run's output — once per test
*binary*, each its own process with its own resolution cache — never once
per test. Every container-backed test after the first one in a binary
reused the cached fallback and paid no further wait, which is the whole
point of `image_reference.rs`'s per-reference cache: the composed binary's
11 container-backed tests plus 15 unit tests still finished in 127.96s
total, and the stack binary's 3 container-backed tests plus 15 unit tests
in 123.33s — one deadline per binary, not one per test, and certainly not
one per container-backed test (which would have been 14 deadlines, over 28
minutes, on this machine).

Of the 14 container-backed tests, 13 actually reached a running connector
and postgres this time: no "skipped -- no Docker daemon available" line
appears anywhere in this run's output. The 14th,
`a_connector_that_answers_http_but_not_ndc_is_refused_rather_than_believed`,
reached only the nginx impostor it starts on purpose
(`support::impostor::Impostor`, deliberately not a `support::stack::Stack`
-- it never touches postgres or the real connector image at all) and passed
by refusing it, exactly what it exists to prove. Two separate checks
immediately afterward, not one, because a container check cannot stand in
for a network check: `docker ps -a --filter name=fabric-ndc-acc` was empty,
confirming the thirteen `Stack`s' and the one `Impostor`'s `Drop` each
removed its own container(s); `docker network ls --filter
name=fabric-ndc-acc` was empty too, separately confirming the same `Drop`s
removed their networks (`docker ps` lists containers, never networks, so it
cannot carry that half of the claim on its own). `pgrep -f
docker-credential-desktop` was also empty afterward -- the bounded
reader-join this pass adds to `process/deadline.rs` is what makes that true
despite the credential helper the pull deadline observes lingering past its
own parent's exit (see that file's `kill_process_group` doc). Wall-clock for
the whole `cargo test -p fabric-ndc-acceptance` invocation, start to finish:
4 minutes 12 seconds.

This is still the **default mode** row, not the required mode: the
bare-tag fallback that made this run possible is exactly what
`FABRIC_REQUIRE_CONNECTOR_ACCEPTANCE=1` refuses
(`tests/support/docker/image_reference.rs`'s required mode), so this run
does not stand in for that row, and cannot. That row was filled in from
`.github/workflows/ci.yml`'s `connector-acceptance` job — whose runner can
pull the pin normally, and which pre-pulls all three pinned images as its
own step precisely so a pull problem shows up there rather than inside a
test's own timeout — on its first green run, for PR #66: every digest was
pulled exactly as pinned, no fallback line appeared, and the two binaries
finished in 12.32 s and 3.48 s, the difference from the 4 minutes above being
the 120 s pull deadline this machine pays once per binary and CI never does.

Nothing is ignored.

Most of the numbers in this table are from a run in the M1 worktree at
`9e7c83b` ("Fail closed when a held manifest outlives its payload"), the
commit that adds `fabric-runtime-publication` and the two architecture
invariants it brings (see "Runtime publication (M1)" below) — newer than the
`claude/split-issuer-from-endpoints` run the rest of this document otherwise
describes. That is also why most counts grew: 11 invariants across 21 crates,
not 8 across 14. None of that growth outside `fabric-runtime-publication` is
this slice's work — it is the accumulated increments between the two runs —
and none of it is re-narrated here; only the table above and the new section
below reflect the later commit.

The **Tests**, **Console tests** and **Console build** rows are newer still —
counted on issue #61's identity-edge lane rebased onto `e47d13a` (the tree
after #66 merged), in the `claude/m2-edge-trust` worktree: `cargo test
--workspace --exclude fabric-ndc-acceptance` (1940 passing, 0 failing,
1 ignored — the exclusion is real on this tree: `fabric-ndc-acceptance` is a
workspace member since #66, and its 44 tests are the `connector-acceptance`
row's, run by CI in required mode), `npm test -- --run` (91 passing) and
`npm run build` (229.27 kB, 70.24 kB gzipped). Everything else in the table
is the M1-era run those rows superseded, or the connector-acceptance rows
recorded under #66.

The Architecture and Tests rows have moved once more since, for issue #62:
this slice adds `fabric-ndc-acceptance`, a 22nd crate
(`python3 scripts/check_architecture.py` now reports 11 invariants across 22
crates, not 21), and excluding it from the workspace suite the way CI already
does (`cargo test --workspace --exclude fabric-ndc-acceptance`) now reports
1767 passing, 0 failing, 1 ignored, not 1745. The Gates table above already
carries both updated numbers; the `9e7c83b` figures just above stay as
recorded for the M1 increment they describe.

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
`fabric_tenant_runtime::build_runtime` over the real `JsonFileSource` (behind a
counting decorator that only delegates, so a refresh can be observed) and the
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
| 1a | Globex's discriminator value zeroed to `""` via the `GLOBEX_DISCRIMINATOR_VALUE` constant | `tests/support/fixtures.rs` | 7 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row` (at `9e7c83b` this left 13/13 green — see below; the two predicate tests still pass, since they compare against the same mutated constant) | `assertion \`left == right\` failed: globex`<br>`left: 404 right: 200` |
| 1b | Globex's discriminator value changed to `""` at the binding call site only, constant left alone | `tests/support/fixtures.rs` | 9 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row`, `each_call_reaches_the_connector_carrying_only_its_own_tenant_predicate` | `assertion \`left == right\` failed`<br>`left: Some(Compare { … value: String("") })`<br>`right: Some(Compare { … value: String("tenant-globex-915") })` |
| 2 | Both tenants given the same discriminator value | `tests/support/fixtures.rs` | 6 of 13 composed tests, incl. both named above | `assertion \`left == right\` failed`<br>`left: Some(Compare { … value: String("tenant-acme-482") })`<br>`right: Some(Compare { … value: String("tenant-globex-915") })` |
| 3 | Recording connector ignores the predicate, always returns the whole corpus | `tests/support/connector.rs` | 4 of 13 composed tests, incl. `two_tenants_sharing_one_data_source_each_receive_only_their_own_row` (`each_call_reaches_…_tenant_predicate` still passed — it inspects the captured predicate, not the connector's answer) | `assertion \`left == right\` failed: {"data":[{"id":"1","title":"Acme Handbook"},{"id":"1","title":"Globex Playbook"}],…}`<br>`left: 2 right: 1` |
| 4 | Stale-revision compare inverted (`<` → `>`) | `src/verdict.rs` | `a_stale_revision_publication_is_refused_and_the_last_good_files_remain` in **both** integration binaries, plus `a_refused_publication_writes_nothing_at_all`, `an_emptying_publication_is_refused_unless_it_is_intended`, and 4 `verdict_tests` unit tests | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Unchanged, data_sources: Unchanged, catalog: Unchanged }` |
| 5 | Divergent-payload compare bypassed (`held_payload == incoming.payload` → `true`) | `src/verdict.rs` | `a_same_revision_publication_with_a_different_payload_is_refused` in **both** integration binaries, plus `verdict_tests::the_same_revision_with_different_bytes_is_refused_as_divergent` | `called \`Result::unwrap_err()\` on an \`Ok\` value: PublicationReport { tenants: Unchanged, data_sources: Unchanged, catalog: Unchanged }` |
| 6 | Emptying guard disabled | `src/validate.rs` | `an_emptying_publication_is_refused_unless_it_is_intended` (composed) and `validate_tests::taking_tenants_from_non_empty_to_empty_without_intent_is_refused` (unit) — at `9e7c83b` the adapter's own integration suite (`tests/filesystem_runtime_publication.rs`) stayed fully green; rerun after `6426c93` it fails `a_populated_tenants_payload_with_no_manifest_still_guards_against_emptying` (21 passed, 1 failed) | `called \`Result::unwrap_err()\` on an \`Ok\` value: ()` |
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
  document without intent, and asserts the refusal with every path's identity
  unchanged (absent before and after counts as unchanged) —
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

## Connector acceptance (issue #62)

`fabric-ndc-acceptance` is new: a test-only crate in neither plane
(`docs/architecture/crate-dependencies.md`), holding the one composed test
that is allowed to compose the real `fabric-runtime-publication` publisher,
the real `fabric-tenant-runtime` runtime, the real `fabric-data-api` Data
API, and the real `fabric-connector-ndc` adapter against an actual
`ghcr.io/hasura/ndc-postgres:v3.1.0` process talking to a real
`postgres:16-alpine` — the gap the previous run of this document named as
"no test has spoken to a running connector".

**The composed test.**
`tests/published_state_reaches_a_real_connector.rs` publishes a fixture —
one shared, `Shared`-placement DataSource behind a `ConnectionSelector::Default`,
two tenants isolated by different values (`tenant-acme-482`,
`tenant-globex-915`) in the same `tenant_key` discriminator column, and a
one-resource `articles` catalogue exposing only `id` and `title` — through
the real `FilesystemRuntimePublication`, brings up postgres and the
connector via `std::process::Command` (`tests/support/docker/`, no
`testcontainers` — lead decision 2), negotiates the real
`fabric_connector_ndc::build_ndc_connector` against the running connector's
base URL, builds the real `fabric_tenant_runtime::build_runtime` over the
published files, and serves both through the real
`fabric_data_api::build_data_api`, driven with `tower::ServiceExt::oneshot`
and unsigned bearer tokens. The corpus is seeded as literal SQL
(`tests/support/postgres.rs::SEED_SQL`), never a Rust constant the fixture
could move alongside it — row 1a's lesson from the M1 section above, applied
here with a real database in place of a fake corpus, which is what makes
mutations 3 and 4 below catch what the equivalent mutation against a shared
constant could not.

Eleven tests. Ten prove the acceptance criteria directly: two tenants
sharing one physical table each receive only their own row, on both the list
and the keyed-lookup routes; the same logical key resolves to a different
physical row per tenant; a direct `psql` count proves both rows really exist
in the one table the query narrowed; a caller-supplied `X-Tenant-Id` header
is refused with `400` before the identity extractor returns, which is before
any connector call could run; no response body names the connector id, the
discriminator column, or either tenant's discriminator value; a connector
declaring no request-level arguments is refused at `build_ndc_connector`
before it can serve anyone; a real HTTP process that is not an NDC connector
(`nginx`, reconfigured to answer `200` on every path — see
`tests/support/impostor.rs` for why the stock welcome page will not do) is
refused as a malformed response rather than believed; a connector stopped
mid-run answers `503 connector_unavailable` with `Retry-After: 5` and no row
from any tenant; and an insert the connector accepts (`insert_articles`,
mapped to `objects`/no `filter_argument` — an insert carries no predicate,
only a stamped row) reports the `affected` count the connector gave, with
the written row readable back only by the tenant that created it. The
eleventh, on the version floor, asserts what is actually observable: the
handshake succeeding is the only signal available from outside
`fabric-connector-ndc`, because ADR 0001 keeps the negotiated version number
inside that crate the same way it keeps every other NDC type there; the
floor's own enforcement is pinned by `fabric-connector-ndc`'s fixture-backed
unit tests instead.

`a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives` was
not implemented at this point in the record. The real
`delete_articles_by_id_and_tenant_key` procedure requires `key_id` and
`key_tenant_key` arguments alongside its `pre_check` predicate, and
`fabric_connector_ndc::CollectionProcedures` had nowhere to carry a required
key argument — a neutral `MutationSpec::Delete { filter }` could not be
expressed against this connector's generated procedures as they then stood.
This was F3, below, deferred to its own issue rather than grown here.

**F3 is now closed.** PR #71 (2026-09-17, ADR 0020, superseding ADR 0004)
gave a procedure mapping a way to name which arguments carry the logical key
and how an update payload is shaped, so key values are read from the
tenant-scoped predicate's own equalities rather than the request body. The
test named above is now implemented, and three more joined it, in
`crates/fabric-ndc-acceptance/tests/published_state_reaches_a_real_connector.rs`:
`a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives`,
`a_keyed_delete_removes_only_this_tenants_row_under_the_shared_key`,
`a_keyed_update_changes_only_this_tenants_row`, and
`no_write_response_names_the_key_arguments_or_the_procedure`. See "Keyed
writes reach ndc-postgres (issue #62's F3, PR #71)" below for what each
proves.

**The mutation experiments.** Five mutations were run against this
composed test in this worktree, following the standard above: mutate, run
`cargo test -p fabric-ndc-acceptance --test published_state_reaches_a_real_connector`,
record what failed, restore the file (`git checkout --`, or a manual revert
for the two mutations inside this crate's own not-yet-committed
`tests/support/fixtures.rs`), confirm `git status --short` is clean, move
on. No code change from this exercise survives in this commit. This run is
also recorded in `e3d5518`'s own commit message -- the commit that both ran
these five mutations and added this table.

| # | Mutation | File | Test(s) that failed | First failure line |
| --- | --- | --- | --- | --- |
| 1 | `IsolationModel::tenant_predicate` returns `None` unconditionally | `crates/fabric-connector/src/execution/isolation_model.rs` | 5 of 11: `two_tenants_sharing_one_physical_table_each_receive_only_their_own_row`, `both_tenants_rows_really_are_in_the_one_table_the_query_narrowed`, `the_predicate_the_platform_built_is_the_predicate_the_database_applied`, `the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant`, `a_write_the_connector_accepts_reports_the_count_the_connector_gave` (this one because the insert stamp is gated by the same early return, so the row landed with no `tenant_key` at all and the `NOT NULL` column rejected it) | `assertion \`left == right\` failed: acme should see only its own row: {"data":[{"id":"1","title":"Acme Handbook"},{"id":"1","title":"Globex Playbook"}],…}`<br>`left: 2 right: 1` |
| 2 | `QuerySpec::for_target` returns `self.clone()` unconditionally | `crates/fabric-connector/src/query/query_spec.rs` | Same 5 as row 1 | `assertion \`left == right\` failed`<br>`left: 200 right: 404` (globex's keyed read of the row acme had just inserted, which an unpredicated query no longer hides) |
| 3 | Globex's discriminator value zeroed to `""` at the binding call site only | `crates/fabric-ndc-acceptance/tests/support/fixtures.rs` (`read_only_snapshot`) | 5 of 11: `both_tenants_rows_really_are_in_the_one_table_the_query_narrowed`, `no_response_names_the_table_the_connector_or_the_discriminator`, `the_predicate_the_platform_built_is_the_predicate_the_database_applied`, `the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant`, `two_tenants_sharing_one_physical_table_each_receive_only_their_own_row` | `assertion \`left == right\` failed: globex`<br>`left: 0 right: 1` — globex's predicate now matches no real row at all, rather than the wrong one |
| 4 | Both tenants given the same discriminator value | `crates/fabric-ndc-acceptance/tests/support/fixtures.rs` (`read_only_snapshot`) | 4 of 11: `the_predicate_the_platform_built_is_the_predicate_the_database_applied`, `both_tenants_rows_really_are_in_the_one_table_the_query_narrowed`, `the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant`, `two_tenants_sharing_one_physical_table_each_receive_only_their_own_row` | `assertion \`left == right\` failed: globex`<br>`left: String("Acme Handbook") right: "Globex Playbook"` |
| 5 | `to_expression` drops the tenant conjunct from the `And` (the last clause, where `Filter::and` places it) | `crates/fabric-connector-ndc/src/translate/expression.rs` | 2 of 11: `a_write_the_connector_accepts_reports_the_count_the_connector_gave`, `the_same_logical_article_key_reaches_a_different_physical_row_for_each_tenant` — a bare list query's tenant predicate is never wrapped in `Filter::And` (there is no caller filter to conjoin with), so only the two tests driving a *keyed* read, whose filter is `key.and(tenant)`, are affected | `assertion \`left == right\` failed`<br>`left: 200 right: 404` |

Mutations 1, 2 and 5 confirm §2.5 of the plan directly: the unpredicated
query really does return both tenants' rows once the platform stops adding
the predicate, which is what makes
`the_predicate_the_platform_built_is_the_predicate_the_database_applied`'s
pair of facts (two physical rows; one row per tenant through the router)
meaningful rather than coincidental. Mutation 3 is the form that left
`docs/verification.md` row 1a's Rust-fixture equivalent green at `9e7c83b`
in the runtime-publication crate; here it cannot, because the corpus a real
`psql` reads back cannot follow a mutation to a Rust binding.

**What this closes, and what it does not.** Two defects round six of review
found were invisible to every unit test because the requests were
well-formed and the logic correct — a connector that declares no
request-level arguments silently ignoring per-tenant routing, and
`affected_rows` not being an NDC concept on `/mutation` at all — are now
observed directly against a running connector, not read from source or
documentation, and both are checked at startup or refused. The falsified
assumptions, and the commit that corrected each:

| # | Assumption that was wrong | Corrected in |
| --- | --- | --- |
| F1 | Every write could omit `fields` on a procedure request. A real `ndc-postgres` refuses that outright: `400 — "Procedure requests must ask for 'affected_rows' or use the 'returning' clause."` | `6defacb` |
| F2 | The shipped example's predicate argument name, `filter`, was a real connector's name for it. A real `ndc-postgres` calls it `pre_check` (delete, update) or `post_check` (insert) | `6defacb` |
| F4 | `wire/response.rs`'s rustdoc gave the wrong reason `rows` is ever absent from a query response. `ndc-postgres` never takes that route; only another connector might | `e5e2d73` |
| F3 | Neutral update/delete could be mapped onto this connector's generated procedures the same way insert is. They cannot: `update_articles_by_id_and_tenant_key` and `delete_articles_by_id_and_tenant_key` require `key_id`/`key_tenant_key` arguments `CollectionProcedures` has nowhere to carry | **Closed since this table was written** — `72aac20` (PR #71, 2026-09-17). ADR 0020 supersedes ADR 0004: a procedure mapping may now name which arguments carry the logical key, key values come only from the tenant-scoped predicate's own equalities, never the request body. See "Keyed writes reach ndc-postgres" below |

## Keyed writes reach ndc-postgres (issue #62's F3, PR #71)

ADR 0020 (`72aac20`, 2026-09-17) supersedes ADR 0004 and closes the one gap
"Connector acceptance (issue #62)" above named and deferred: a procedure
mapping may now name which of a procedure's arguments carry the logical key
and how an update's payload is shaped, so `PATCH` and `DELETE` on a keyed
resource can reach `ndc-postgres` v3.1.0's real
`update_articles_by_id_and_tenant_key` and `delete_articles_by_id_and_tenant_key`
procedures instead of stopping at "cannot be expressed." Key values come only
from the tenant-scoped predicate's own direct equalities — never the request
body, never inferred — and the discriminator still goes out twice: once as a
key argument, once inside `pre_check`, both read from the same conjunct.

**Proven against the real connector.** Four tests joined
`crates/fabric-ndc-acceptance/tests/published_state_reaches_a_real_connector.rs`,
all driven through the same real `FilesystemRuntimePublication` →
`fabric_tenant_runtime` → `fabric_data_api` → `fabric_connector_ndc` stack
against `ghcr.io/hasura/ndc-postgres:v3.1.0` and `postgres:16-alpine` that
the rest of that file uses:

- `a_delete_scoped_to_another_tenant_affects_nothing_and_the_row_survives` —
  the test issue #62 named and could not implement. globex asks to delete a
  row acme owns, by its logical key; the response reports `affected: 0`, a
  direct `psql` count shows the row still exists, and acme can still read it.
  This is the cross-tenant proof: the key argument alone is not what scopes
  the delete, the predicate travelling alongside it is.
- `a_keyed_delete_removes_only_this_tenants_row_under_the_shared_key` — acme
  and globex each have a row under the same logical key `1` (a shared table,
  discriminator-isolated). acme's delete removes exactly one physical row,
  `psql` confirms one remains, and it is globex's; globex's own read is
  unaffected.
- `a_keyed_update_changes_only_this_tenants_row` — a `PATCH` from acme
  changes only acme's physical row under the shared key; `psql` shows
  globex's title unchanged, and globex's own read agrees.
- `no_write_response_names_the_key_arguments_or_the_procedure` — runs the
  cross-tenant delete, the update and the keyed delete in sequence and
  inspects every response body: no `key_id`, `key_tenant_key`, procedure
  name, or other NDC vocabulary escapes to the caller, which is the same
  containment `check_ndc_containment` enforces structurally, now checked
  at the response as well.

**Proven at the unit level, one rule at a time.** ADR 0020 states four rules
and each is enforced rather than documented; the tests it points at (all in
`crates/fabric-connector-ndc/src`):

- *A key value must come from an equality the platform can see* —
  `translate/key_equality_tests.rs`:
  `a_bare_top_level_equality_is_found`,
  `a_direct_clause_of_a_top_level_and_is_found`,
  `an_equality_hidden_under_an_or_is_not_found`,
  `an_equality_inside_a_nested_and_is_not_found`,
  `no_equality_at_all_is_refused`, and
  `two_differing_equalities_are_refused_as_contradictory`.
- *A key argument may not collide with the payload or predicate argument, or
  with another key argument* — `translate/key_arguments_tests.rs`:
  `a_key_argument_colliding_with_the_filter_argument_is_refused_rather_than_overwriting_it`,
  `a_key_argument_colliding_with_the_payload_argument_is_refused`,
  `a_key_argument_colliding_with_another_key_argument_is_refused`.
- *A key argument is checked against the connector's own schema at startup*
  — `registration/key_arguments_tests.rs`:
  `a_key_argument_the_procedure_never_declares_is_refused`,
  `a_key_argument_declared_as_a_predicate_is_refused`,
  `a_key_field_absent_from_the_collections_schema_is_refused`.
- *A required argument nothing supplies is a startup failure, not a `400` on
  the first write* — `registration/required_arguments_tests.rs`:
  `a_mapping_missing_a_required_key_is_refused_at_startup`,
  `a_mapping_supplying_no_keys_at_all_is_refused`,
  `a_delete_mapping_that_puts_a_key_value_in_payload_argument_instead_of_key_arguments_is_refused`,
  `the_full_articles_mapping_covers_every_required_argument_on_the_real_schema`.
- *The payload shape is a closed enum* (`values` or `set_operations`, the
  latter only for an update) — `config/payload_shape_tests.rs`
  (`the_default_shape_is_values`, `parses_snake_case_from_configuration`)
  for the enum itself, and `config/connector_validation_tests.rs`
  (`set_operations_on_an_insert_mapping_is_rejected`,
  `set_operations_on_a_delete_mapping_is_rejected`) for the refusal.

**One claim in ADR 0020 this pass could not independently confirm.** The ADR
states, citing "`docs/verification.md`, mutation M3": "with `pre_check`
removed from a keyed delete, `ndc-postgres` v3.1.0 still deleted exactly one
row, because the `articles` primary key is `(id, tenant_key)` and the key
arguments alone named it" — offered there as the measured reason the
discriminator is still sent a second time inside `pre_check` rather than
relied on to be redundant. No mutation table entry labelled M3 existed in
this document before this pass, `cargo test -p fabric-ndc-acceptance` was not
run for this pass (see the Gates table), and mutating `translate/mutation.rs`
to drop `pre_check` from a keyed delete and re-running the acceptance suite
against real Docker is what it would take to reproduce this directly. Recorded
as the ADR states it, not independently re-verified here.

## Observed platform deployments (PR #72, ADR 0022)

Before PR #72 the Components page always said `Running: Unknown` — desired
state in Git was proven, but nothing observed what was actually serving.
ADR 0022 (Accepted for the LucentRoot readiness milestone) adds an optional,
read-only `DeploymentObserver` port that `fabric-deployment-kubernetes`
implements: bounded `GET`s against named Deployments, Pods and ReplicaSets,
using the pod's own projected service-account token, with no create, update,
delete, exec, secret-read or cluster-wide permission. The runtime plane
cannot depend on it (`check_the_planes_do_not_meet` and the crate graph
above), and Kubernetes representations stay inside the adapter
(`check_adapter_containment`'s third rule, added with this crate).

**What counts as a healthy running version, proven at the unit level** (all
in `crates/fabric-deployment-kubernetes/src`):

- *A version requires a current observed controller generation, not just
  ready-looking replicas* — `evaluate_tests.rs::requires_controller_and_actual_container_evidence`
  drives a fixture through `observedGeneration` lagging `generation` (reports
  `Progressing`) and a running container whose image ID disagrees with the
  deployment's pinned digest (reports `Progressing` with no version claimed)
  — the exact shape of "desired moved, running has not caught up yet" that a
  live rollout produces.
- *A pod must be owned through an unbroken controller chain, ready, and not
  terminating* — `evaluate_tests.rs::foreign_pods_cannot_satisfy_rollout`,
  `terminating_and_unready_pods_are_not_healthy`.
- *A scale-to-zero reports stopped only once the controller agrees, not on
  the request alone* — `evaluate_tests.rs::stopped_workloads_require_completed_scale_down`.
- *A failed rollout condition overrides ready counts that look fine in
  isolation* — `evaluate_tests.rs::failed_controller_overrides_ready_counts`.
- *A tag-only image, or an image from a different repository, is not version
  evidence* — `evaluate_tests.rs::tag_only_and_wrong_repository_images_are_not_version_evidence`.
- *Mixed releases across workloads never collapse into one reported version,
  a stopped runtime never hides a healthy control plane, and missing or
  failed evidence is never papered over with an old version* —
  `summary_tests.rs::mixed_releases_never_become_a_single_running_version`,
  `stopped_runtime_does_not_hide_a_healthy_control_plane`,
  `missing_or_failed_evidence_cannot_reuse_a_version`.
- *The client re-reads its projected token, never caches a stale credential
  or leaks a cluster response verbatim, and refuses a paginated list rather
  than acting on a partial one* — `client_tests.rs::credentials_rotate_and_only_get_requests_are_sent`,
  `errors_do_not_expose_cluster_response_or_credentials`,
  `refuses_partial_lists_and_encodes_selectors`.

**Observation is independent of desired-state selection**, proven in
`crates/fabric-platform-management/src/service/service_tests.rs::observation_does_not_change_desired_state_or_update_selection`:
a fixture observer reports `0.3.0-preview.1` running while desired state is
already `0.3.0-preview.2` (and a reconcile moves desired state on to
`0.3.0-preview.3`); the test asserts `status.running` reflects only what was
observed, `status.desired` and the update-selection path never read it back,
and no desired-state write happens as a side effect of asking. This is the
same desired-ahead-of-running shape LucentRoot went on to show for real — see
below.

The console's half: `apps/control-plane-ui/src/components/DeploymentEvidence.test.tsx`
asserts the display distinguishes "not configured" from "configured but the
observation itself failed" (`distinguishes missing configuration from an
observation failure`) and shows mixed releases and a stopped runtime as what
they are, without implying a convergence the evidence does not support
(`shows mixed releases and a stopped runtime without claiming convergence`).

**What only LucentRoot could show.** The readiness record kept in the
platform repository (`saas-fabric-platform`'s `docs/fabric-readiness.md`) is
reported to show exactly the sequence the unit tests above predict rather
than merely permit: desired state advanced to `preview.13`
(`fabric-platform-git`'s `ac0985f`) while the observer still reported
`preview.12` running, because Argo had not yet applied the new manifest: the
same "generation observed, but not yet the new one" state
`requires_controller_and_actual_container_evidence` exercises with a
fixture. A later observation reported both desired and running agreeing on
`preview.13`. This document has not read `docs/fabric-readiness.md` directly
— it is a record in a different repository — so this paragraph states what
is reported there, not something re-derived from this repository's own
tests; the unit tests above are what this repository can independently
stand behind.

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

890 Rust source files under `crates/*/src` across the workspace's 23 crates
(up from 815 files when this section was last counted) — 366 the runtime
plane's, 349 the control plane's, and 175 in neither, using
`scripts/check_architecture.py`'s own `RUNTIME_PLANE`/`CONTROL_PLANE` sets to
place each crate; `scripts/check_file_sizes.py` measures 777 — the 890 less the 114
`*_tests.rs` siblings it excludes by rule, plus
`crates/fabric-control-plane-api/examples/console_workbench.rs`, which sits
outside `src/` and is measured because the script excludes no `examples/`
directory (the script also walks
`tests/`, `benches/` and `examples/`, but per-crate integration tests under
`crates/<name>/tests/` are excluded the same way, so they never entered the
890 in the first place). The policy (`docs/architecture/file-size-policy.md`)
treats 150 production lines as a hard limit and 120 as advisory; test lines
never count, whether they live in a sibling `*_tests.rs` or in a trailing
`#[cfg(test)]` module.

Counted by running the script on `main` at `f4a39d1` (PR #73), 2026-09-18.
Re-run it rather than trusting the numbers: they are a snapshot, and the
point of recording them is that the next snapshot can be compared.

- **Over 150 lines: none unexplained.** Two files hold an exemption, each with
  its reason recorded beside it in the script:
  `fabric-control-plane/src/errors.rs` (267, up from 175) and
  `fabric-fga-auth/src/cache.rs` (155, unchanged).
- **Over 120 lines: 132 files**, of which 130 are inside the hard limit. The
  largest unexempted are three at exactly 150 —
  `fabric-client-model/src/document.rs`,
  `fabric-connector/src/errors/connector_error.rs`, and
  `fabric-platform-management/src/desired_state/port.rs`. Every file in the
  121–150 band that this lane wrote or touched states its reason at the top of
  the file, which is what the policy asks of that band.

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

Two more reached the limit in this lane, each because something had to be added
to a file that was already at exactly 150, and each was split rather than
grown: `fabric-client-model/src/identity/redirect_uri/host_kind/registered_domain.rs`
kept the rule about a whole *name* and gave `registered_domain/label.rs` the
rule about one *label*; `fabric-identity/src/logging.rs` kept the domain's
refusals and gave `logging/startup.rs` the one event there that is not one.

**130 files sit in the 121–150 band**, and the reason is the one it has always
been: rustdoc. Every file in the band is a type or a function set whose prose
outweighs its code — `redirect_uri.rs` is 138 lines for a newtype over a
`String`, and most of that is the argument for why a wildcard in the host is
refused. Splitting prose away from the thing it explains would satisfy the
counter and make the code worse.

The console's source has grown to 101 non-test `.ts`/`.tsx` files under
`apps/control-plane-ui/src` (up from 17), held to the same 150-line limit by
ESLint's `max-lines` (`eslint.config.js`, `max: 150`, test files exempted the
same way `*_tests.rs` siblings are on the Rust side). Not individually
re-measured for this pass; the Gates table's Console lint row (`npm run
lint`, 0 findings) is what stands behind "held to the limit", since a
violation is an error under that config, not a warning.

## Dependency licences

218 packages in the resolved graph (`cargo metadata --format-version 1
--all-features`, workspace members plus every resolved dependency) — 195
third-party and 23 of this workspace's own — every one carrying an
OSI-approved permissive licence. `deny.toml`'s `exceptions` list is empty,
and its `allow` list is the set of licences actually present — not a set
approved in principle, so an unmatched entry never sits there as noise.
`cargo deny check` reports advisories, bans, licences and sources all ok on
this tree (2026-09-18).

The third-party count (195) is unchanged since this table was last counted —
ten more workspace crates have joined since (13 → 23: `fabric-runtime-publication`,
`fabric-ndc-acceptance`, `fabric-openbao`, `fabric-fga-auth`,
`fabric-fga-auth-api`, `fabric-git-host`, `fabric-platform-git`,
`fabric-platform-management`, `fabric-registry`, `fabric-deployment-kubernetes`),
and every one of them was built entirely out of crates already in the graph.
That is also why only the `Apache-2.0` row below changed: it carries every
workspace crate's own licence (`license.workspace = true`, `Apache-2.0`) plus
the one third-party crate whose licence is that string alone, so it rises by
exactly the ten new crates, 14 → 24.

| Count | Licence |
| --- | --- |
| 107 | MIT OR Apache-2.0 |
| 35 | MIT |
| 18 | Unicode-3.0 |
| 24 | Apache-2.0 |
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

The control plane's third-party additions, historically, were exactly two
crates, both verified per crate and per version: `serde_norway` 0.9.42 (MIT
OR Apache-2.0) and `unsafe-libyaml-norway` 0.2.15 (MIT). See
`docs/architecture/dependency-policy.md`. Notably it has still added **no**
Git library and **no** Kubernetes client — checked again for this pass
because `fabric-deployment-kubernetes` (new in PR #72, ADR 0022) is exactly
the kind of crate that would tempt one: it reads the Kubernetes API directly
over the `reqwest` that was already in the graph
(`crates/fabric-deployment-kubernetes/Cargo.toml` declares `fabric-core`,
`fabric-platform-management`, `async-trait`, `reqwest`, `serde`,
`serde_json`, `tokio` and nothing else), and `scripts/check_architecture.py`'s
"No drivers, no control-plane clients" invariant — which bans `kube`,
`k8s-openapi`, `kube-client`, `kube-runtime`, `git2`, `gix` and `gitoxide`
from the whole resolved graph, not just direct dependencies — still holds.

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

23 crates now, up from the 6-and-6 split of 13 this section originally
described. Each plane has grown two members since 2026-08-29 —
`fabric-fga-auth`/`fabric-fga-auth-api` (the authorization trust boundary,
ADR 0016) joined the runtime plane, and `fabric-openbao` and
`fabric-deployment-kubernetes` joined the control plane — and the third
group, in neither plane, has grown from one crate (`fabric-core`) to seven:
the four platform-side crates below sit there on the same footing as
`fabric-core` (`docs/architecture/crate-dependencies.md`), reachable from the
control plane and never from the runtime plane. Every
edge below is exactly what `scripts/check_architecture.py`'s own
`expected` table (its "Dependency direction" check) declares, cross-checked
against `docs/architecture/crate-dependencies.md`, which the script also
verifies; a new edge that is not in the document fails CI.

```
fabric-core                    (no internal dependencies; the kernel every other crate can reach)

  runtime plane
fabric-identity                → core
fabric-connector               → core
fabric-tenant-runtime          → core, connector
fabric-connector-ndc           → core, connector, tenant-runtime
fabric-data-api                → core, identity, tenant-runtime, connector
fabric-fga-auth                → core                                    (the trust boundary, ADR 0016)
fabric-fga-auth-api            → core, fga-auth                          (composition root, own process)
fabric-api                     → core, identity, tenant-runtime,
                                  connector, connector-ndc, data-api      (composition root)

  control plane
fabric-client-model            → core
fabric-reconciliation          → core, client-model
fabric-control-plane           → core, client-model, reconciliation,
                                  platform-management
fabric-keycloak                → core, client-model, reconciliation,
                                  control-plane                          (implements IdentityProvider, OperatorSignIn)
fabric-client-git               → core, client-model, control-plane, git-host
                                  (implements the client desired-state port, ADR 0008)
fabric-deployment-kubernetes   → core, platform-management               (implements DeploymentObserver, ADR 0022)
fabric-openbao                 → core, control-plane                     (implements SecretStore, IntegrationStore)
fabric-control-plane-api       → core, git-host, platform-git,
                                  platform-management, registry,
                                  deployment-kubernetes, client-model,
                                  reconciliation, control-plane,
                                  keycloak, client-git, openbao           (composition root)

  neither plane
fabric-platform-management     → core                                    (update-policy rules; no transport)
fabric-git-host                → core                                    (App-credential exchange, shared)
fabric-platform-git            → core, git-host, platform-management     (implements the platform desired-state port)
fabric-registry                → core, platform-management               (implements Registry)
fabric-runtime-publication     → core (non-dev); dev: tenant-runtime, data-api, identity, connector
fabric-ndc-acceptance          → runtime-publication, connector-ndc,
                                  tenant-runtime, data-api, identity,
                                  connector                               (dev-only; test-only, no production code)
```

`fabric-ndc-acceptance` and `fabric-runtime-publication` are the two crates
whose dependency table looks unlike the rest: both are in neither plane, and
`fabric-ndc-acceptance` declares no non-dev dependency at all — it is a
named, visible exception in `check_ndc_containment` and
`check_runtime_plane_cannot_reach_the_publisher`, not a relaxation of either
(`docs/architecture/crate-dependencies.md`, "`fabric-ndc-acceptance` is
test-only, and also in neither plane").

Also checked structurally, because none of these can be caught by a test:

- **The two planes do not meet.** No crate in either plane depends on a crate
  in the other. This is the increment's central structural claim: the runtime
  plane must keep serving tenants while Git and Keycloak are unreachable, and
  one edge would put control-plane availability behind every tenant request.
- **Plane reachability is transitive, and no runtime-plane crate can reach
  the publisher.** Two checks added with `fabric-runtime-publication` (ADR
  0018): a crate cannot join a plane by way of an edge that skips a
  plane-mate, and nothing in the runtime plane may gain a path to the crate
  that publishes the files it reads — the one edge that would make the
  runtime plane a second writer of its own input.
- **Keycloak representations stay in `fabric-keycloak`, Git-hosting details
  in `fabric-client-git`, and Kubernetes deployment evidence stays in
  `fabric-deployment-kubernetes`.** Three adapters, one rule
  (`check_adapter_containment`), checked as vocabulary, not just as
  dependency edges — `*Representation`, `publicClient`, `openid-connect`,
  `ContentsEntry`, `PutContents`, `contents/`, `owner_references`,
  `resource_version`, `ReplicaSet`, `/apis/apps/v1/` may not appear anywhere
  outside their owning crate. Only the control plane's composition root may
  depend on any of the three.
- **The operator console reaches only the control-plane API.** No file under
  `apps/control-plane-ui/src` may name `client_secret`/`clientSecret`,
  `/admin/realms`, `api.github.com`, or a Keycloak admin endpoint. Checked as
  a property of the console's own source rather than of what a response
  happened to contain, because that is the form the rule takes: a fetch to
  another origin is the violation, whether or not a credential is in the
  same commit.
- **The console's Content-Security-Policy widens for exactly one thing.**
  New with the in-product GitHub App flow: `nginx.conf`'s CSP must permit
  `form-action https://github.com` (creating the App is a cross-origin form
  POST of the App manifest) and must not permit `github.com` in any other
  directive — widening `connect-src` or `default-src` would let the console
  fetch from an origin the control plane exists to keep it away from, the
  same claim the bullet above makes about source code, held here about the
  policy that enforces it in the browser.

- **NDC vocabulary stays in `fabric-connector-ndc`.** `fabric-api` may name
  exactly two symbols from it — `NdcConnectorConfig` and
  `build_ndc_connector`, both startup wiring — and `fabric-ndc-acceptance` is
  the one named test-only exception (above). Nothing else in the workspace
  may name an `Ndc*` type at all. Prose is exempt: several crates explain the
  boundary without being permitted to cross it.
- **No transport in the domain crates.** `fabric-core`, `fabric-connector`,
  `fabric-tenant-runtime`, `fabric-client-model` and `fabric-reconciliation`
  declare no HTTP client or server — the runtime-plane and control-plane
  halves of the same claim. `fabric-identity` does depend on Axum,
  deliberately — see the dependency document for why that is the crate's job
  rather than a leak.
- **No database driver anywhere in the graph.** Checked against the full
  resolved set, not just direct declarations — a driver arriving transitively
  compiles into the binary exactly as much as one declared directly, and no
  manifest here would mention it. The runtime plane opens no
  database connections; every physical connection lives inside a connector
  process.
- **No Kubernetes or Git client anywhere in the graph** — still, checked
  again with `fabric-deployment-kubernetes` in the tree, which is exactly
  where somebody would reach for `kube`. It reads the Kubernetes API
  directly over `reqwest`, the same way `fabric-client-git` speaks its Git
  host's contents API over HTTPS: the platform needs no clone, no working
  copy, and no disk, and the strongest form of "never in the request path"
  (§6) is a client that is not linked into any binary this workspace builds.
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

**Updated since this run.** The App this table says does not exist evidently
now does: on 2026-09-17, `fabric-client-git`'s own write path (not the
local-directory development adapter) updated `fabric-catalogue.yaml` on
`github.com/FieldstateNZ/saas-fabric-clients` (commit `d5bdee8`), and
Platform Management's separate Git integration advanced
`saas-fabric-platform` for real (`3fa06bc`, `ac0985f`). Those three commits
are not in this repository; they were read from the hosts on 2026-09-18 with
`gh api repos/FieldstateNZ/saas-fabric-clients/commits/d5bdee8` and
`gh api repos/FieldstateNZ/saas-fabric-platform/commits/<sha>`, which is the
reproducible command, and the second host's author is
`saas-fabric-platform-lucentroot[bot]`, the App identity. Neither is a test
this repository runs, so "Git adapter: protocol fake only" is no longer
accurate as stated but "no automated test against a real Git host" still is
— see "What is not verified" below for the fuller, more careful version of
this update. "Deployment through `saas-fabric-platform`" has also moved: PR
#72 (ADR 0022) added observed deployment evidence, and LucentRoot's own
readiness record shows a desired-vs-running convergence — see "Observed
platform deployments" above.

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

## Keycloak 26.0.8 probe, 2026-09-06 (issue #61)

ADR 0019 §3 and §6 make three claims about Keycloak's own behaviour that
nothing in this workspace could have proven: what an any-port loopback
redirect actually matches, whether `GET /clients` returns `attributes` and
`protocolMappers` without a per-client read, and whether a `PUT` replaces a
client's mapper set or requires the `/protocol-mappers/models` sub-resource.
Slice 4 (the Keycloak adapter change) settled all three, plus four bonus
questions, against a real **Keycloak 26.0.8** — `quay.io/keycloak/keycloak:26.0`,
the image `scripts/e2e-services.sh` uses — running as `start-dev` with realm
`probe`. The socket-level fake in `tests/keycloak_adapter.rs` can confirm any
answer to these and prove none of them; only the real instance can.

| # | Question | Observed |
|---|---|---|
| 1 | Any-port loopback | Over **http**, a loopback URI registered **without a port** (`http://127.0.0.1/cb`, `http://localhost/cb`, `http://[::1]/cb`) matches the same path on any port; the path is compared exactly (`/other` refused). A registered **port** is compared exactly (`http://localhost:5173/cb` does not match `:9999`). Over **https**, the port is always compared exactly: `https://localhost/cb` does not match `https://localhost:5173/cb`, nor does `https://127.0.0.1/cb`. `http://127.0.0.1:*/cb` matches **nothing** (both `:54321/cb` and the portless form are refused) — confirming §3's `Development` row and why `:*` left the model in the prior fix slice. |
| 2 | Mapper read-back | `GET /admin/realms/{realm}/clients` (the list call) returns `protocolMappers` and `attributes` in full, for every client, in one call. No per-client `GET /clients/{id}` is needed — `observe::clients` reads the list once. |
| 3 | Mapper update | `PUT /clients/{id}` carrying `protocolMappers` **replaces the set**: a changed audience is read back changed; an empty list removes the mapper; an omitted key leaves existing mappers alone; an id-less mapper with an existing name replaces it in place (no duplicate). No `/clients/{id}/protocol-mappers/models` call is needed — `declaration()` sends the same full representation for create and update alike. |
| 4 | `attributes` read-back | Yes, on both the list and a single read: `pkce.code.challenge.method=S256` and `post.logout.redirect.uris=+` both round-trip unchanged. |
| 5 | Pagination (bonus) | `GET /clients?first=0&max=2` returned 2 of 13 seeded clients. `admin::paths::clients_page(realm, max)`, mirroring `roles_page`, is valid; `observe::clients` refuses a response that reaches the page cap exactly as `observe::roles` already does. |
| 6 | PKCE enforcement (bonus) | With `pkce.code.challenge.method=S256` set: an authorization request with no `code_challenge` is refused with `error=invalid_request&error_description=Missing parameter: code_challenge_method`; `code_challenge_method=plain` is refused with `Invalid parameter: code challenge method is not matching the configured one`; `S256` reaches the login page. All three are enforced at the authorization endpoint, before any user interaction. |
| 7 | Omitted-field update semantics (bonus) | A client created with no `frontchannelLogout` key holds it `false`. A `PUT` of the same minimal body (still no `frontchannelLogout` key) flips it to `true` on the *first* write; a second, identical `PUT` leaves it `true` — byte-stable from there. So "an omitted field is left alone" is Keycloak's usual behaviour, not a guarantee for every field: some fields are filled with Keycloak's own default the first time a client is written through, rather than left at their create-time value. Nothing this crate models reads `frontchannelLogout`, so the flip changes no decision here — recorded because `wire.rs`'s "deliberately partial" claim needed the caveat. |
| 8 | Second mapper survives observation, not a `PUT` (observed live during review: an `oidc-hardcoded-claim-mapper` added out of band read back as count 2, and the next declared-only `PUT` left count 1) | A protocol mapper added directly through the Admin API — alongside the client's declared `oidc-audience-mapper`, the way an operator working around the platform would — is returned by `GET /clients` next to it: both appear in the list, confirming `other_protocol_mappers` (A13b) counts something Keycloak actually reports, not something the fake merely echoes back. A subsequent `PUT` carrying only `declaration()`'s own shape — the one audience mapper — removes it, per finding 3's replace-the-set semantics: nothing added outside Fabric survives the next sweep. |

**This crate's own wire body, round-tripped.** Beyond the shell-script probe
above, the exact JSON `declaration()` produces for a `claimedHttps` client was
posted to the same instance and read back, to confirm the hand-written
`NewClientRepresentation`/`ClientRepresentation` shapes agree with what
Keycloak actually sends and accepts — not only with what the probe script's
own request bodies did.

POST `http://127.0.0.1:18400/admin/realms/probe/clients`, body exactly as
`declaration()` serialises it:

```json
{"clientId":"web-probe-s4","enabled":true,"protocol":"openid-connect","publicClient":true,"standardFlowEnabled":true,"redirectUris":["https://www.example.com/callback"],"attributes":{"pkce.code.challenge.method":"S256","post.logout.redirect.uris":"+"},"protocolMappers":[{"name":"fabric-audience","protocol":"openid-connect","protocolMapper":"oidc-audience-mapper","config":{"access.token.claim":"true","id.token.claim":"false","included.custom.audience":"saas-fabric-data-api"}}]}
```

`201`, then read back via `GET /admin/realms/probe/clients?clientId=web-probe-s4`
— the relevant fields, exactly as sent:

```json
"attributes": {
    "post.logout.redirect.uris": "+",
    "pkce.code.challenge.method": "S256"
},
"protocolMappers": [
    {
        "name": "fabric-audience",
        "protocol": "openid-connect",
        "protocolMapper": "oidc-audience-mapper",
        "config": {
            "id.token.claim": "false",
            "access.token.claim": "true",
            "included.custom.audience": "saas-fabric-data-api"
        }
    }
]
```

Keycloak added its own defaults around this (`webOrigins`, `clientAuthenticatorType`,
default client scopes, and so on) — none of them modelled here, all of them
ignored by `ClientRepresentation`'s lack of `deny_unknown_fields`, which is
deliberate: this adapter reads only what it wrote. The test client was deleted
after the check; the probe fixture was otherwise left as found.

**Mutations run against this slice's own tests**, per `docs/delivery.md`'s
rule ("mutate the thing, watch the test fail, then keep the test"). D13 is
proved at **two** layers — the adapter's read and the diff's comparison —
because an unmodellable redirect URI has to survive both to go unnoticed:
dropped instead of counted on read, or counted but never compared on diff.
Each mutation below turns off one of the two, independently, and each is
caught:

| Row | Mutation | Test that went red | Restored |
|---|---|---|---|
| C4 | Deleted the `pkce.code.challenge.method` insertion from `provider/declaration.rs`'s `declaration()` | `a_declared_client_is_written_with_the_s256_challenge_method` (`tests/keycloak_adapter_identity.rs`) | Yes, confirmed identical to the pre-mutation file by diff |
| D13, adapter | Restored the old `.filter_map(\|uri\| RedirectUri::try_new(uri).ok())` silent drop in `provider/observe/clients.rs`, in place of `partition_uris` | `an_unmodellable_redirect_uri_is_counted_rather_than_dropped` (`tests/keycloak_adapter_identity.rs`) | Yes, confirmed identical to the pre-mutation file by diff |
| D13, diff | Deleted the `existing.unmodellable_redirect_uris == 0` conjunct from `plan::diff::matches` (`crates/fabric-reconciliation/src/plan/diff.rs`) | `a_redirect_uri_this_model_cannot_parse_is_drift` (`crates/fabric-reconciliation/src/plan/diff_tests.rs`) — reported `plan.actions() == []` where `[UpdateOidcClient(web_client())]` was expected, because every other term still matched | Yes, confirmed identical to the pre-mutation file by diff |

**The version caveat.** §3's any-port rule and §6's mapper-replace behaviour
were observed here on Keycloak **26.0.8**. LucentRoot runs **26.7.2** (see
"Real integration: LucentRoot" above, run 2026-08-28) — the platform lane
re-runs this same probe there and records the result beside these findings
(§G17). Any-port and mapper-replace semantics are unchanged in Keycloak since
long before 26, so the caveat is honesty about what was actually run, not
doubt about the answer.

**The terms `matches` gained, across both slice 5 commits (`1e6fb24`, and the
review follow-up `d1a395b`) plus one more in the final code round
(`1d2fc2c`).** `matches` held two terms — `public` and `redirect_uris ==
declared_uris` — since `4d508e7`, the commit that founded this crate. Slice 5
added six, and the final round added a ninth:

| Term | Added in | Proof |
|---|---|---|
| `unmodellable_redirect_uris == 0` | `1e6fb24` | **Mutation-proved** — the D13, diff row above |
| `challenge_method == Some(declared.pkce)` | `1e6fb24` | Single-field drift test — `a_client_without_a_recognised_challenge_method_is_corrected`, plus `a_v1_client_is_still_reconciled_with_the_s256_challenge_method` (E15) |
| `audience_mapper.as_deref() == Some(configured_audience)` | `1e6fb24` | Single-field drift tests — `a_client_whose_audience_mapper_was_removed_is_corrected` and `a_client_whose_audience_mapper_names_another_audience_is_corrected` |
| `enabled` | `d1a395b` | Single-field drift test — `a_client_disabled_by_hand_is_corrected` |
| `standard_flow_enabled` | `d1a395b` | Single-field drift test — `a_client_whose_standard_flow_was_switched_off_is_corrected` |
| `post_logout_redirect_uris_is_every_registered_uri` | `d1a395b` | Single-field drift test — `a_client_whose_post_logout_set_was_narrowed_is_corrected` |
| `other_protocol_mappers == 0` | `1d2fc2c` | Single-field drift test — `a_client_carrying_a_mapper_nobody_declared_is_corrected` (A13b) |

(all tests in `crates/fabric-reconciliation/src/plan/diff_tests.rs`, plus the
existing `a_declared_client_switched_to_confidential_is_corrected` and the
positive control `a_converged_client_is_left_alone`). Only
`unmodellable_redirect_uris == 0` has a mutation run against it, because it
is the term this section's own probe work turned up as a real, previously
silent gap (D13); the other six are proved by a drift test that flips one
field and asserts the plan corrects it, which is the ordinary proof for a
term nobody has reason to doubt is load-bearing.

**Redirect-URI equality is not a term this slice added.** `existing.redirect_uris
== declared_uris(&declared.redirect)` has compared the two sets for equality
since `4d508e7` (the commit that founded this crate), not since this slice —
an earlier draft of this document said otherwise, and that was wrong.
`an_extra_redirect_uri_the_provider_holds_is_drift` is a new *test* against
that pre-existing term, proving it already catches an extra URI the provider
holds and not only a missing one, not a new term.

The end-to-end path — a correction actually reaching the provider, not only
the plan — is `a_client_whose_mapper_was_removed_is_corrected_on_the_next_pass`
and `a_client_whose_mapper_names_another_audience_is_corrected_by_the_reconciler`
in `reconciler/reconciler_tests.rs`.

## What is not verified

Named here rather than left for a reader to discover.

**Control plane:**

- **The installation token's expiry is honoured but never observed.** The
  adapter reads GitHub's `expires_at` and caches to it, with a margin, a floor
  and a ceiling — all tested against a socket. What no test can show is that
  GitHub's stated expiry matches when the token actually stops working. The
  `401` retry is what covers the difference.
- **No automated test against a real Git host — but real writes have now
  been observed on two.** The automated suite still has none: the
  concurrency mechanism is tested only against a stateful fake that moves
  blob hashes and refuses stale ones, which is what the contents API does,
  and "the host answers `409` for a stale `sha`" is still read from
  documentation, not observed by anything this document can point at.
  What has changed since "Git: the adapter is proven, the deployment is
  not" above (2026-08-28, when the App did not yet exist) is that real
  writes are no longer hypothetical. `fabric-catalogue.yaml` — the file
  `docs/architecture/control-plane.md` names as `fabric-client-git`'s own
  write, at the client repository's root — was updated on
  `github.com/FieldstateNZ/saas-fabric-clients` on 2026-09-17 (commit
  `d5bdee8`, "SaaS Fabric: update product catalogue"), which is that
  adapter's own write path, not the local-directory development one the
  2026-08-28 run used. Separately, and against the *other* Git integration
  (`fabric-platform-git`, a different App and a different repository, per
  "Two Git integrations, one flow"), Platform Management has made real
  commits to `saas-fabric-platform` — for example `3fa06bc` and `ac0985f`,
  advancing an environment to `preview.12` and `.13`. Both are commits read
  from a real host's own history on 2026-09-18, with
  `gh api repos/FieldstateNZ/saas-fabric-platform/commits/<sha>`, not from a
  fake's in-memory state; neither
  is a test this repository runs, and this document has not read a git log
  entry recording `409` on a stale `sha` from either host, so that specific
  path stays unproven. See "Real integration: LucentRoot" above for the
  Keycloak adapter's equivalent proof and what distinguishes it from this.
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
- **`saas-fabric-clients` exists and has been written to for real, which is
  new.** `github.com/FieldstateNZ/saas-fabric-clients` holds
  `fabric-catalogue.yaml`, updated by commit `d5bdee8` on 2026-09-17 — see
  the Git-host bullet above for what that does and does not establish. The
  document contract it follows is `docs/architecture/client-desired-state.md`,
  and the shipped examples conform to it under test; what remains unproven is
  still the same automated coverage named above, not the repository's
  existence.

**Runtime plane:**

- **The connector integration gap is closed, F3 included.** As of issue #62
  slice 4, something in this workspace has spoken to a running NDC
  connector: `crates/fabric-ndc-acceptance/tests/published_state_reaches_a_real_connector.rs`
  composes the real publisher, runtime, Data API and NDC adapter against an
  actual `ghcr.io/hasura/ndc-postgres:v3.1.0` and `postgres:16-alpine`, and
  proves tenant isolation, fail-closed behaviour, and a real write against
  them rather than a fake. "Connector acceptance (issue #62)" above is the
  detail — the composed test's own description and its mutation table, which
  falsifies "no predicate reached the connector" by disabling the predicate
  and watching the isolation assertions fail against the real database.

  Round six of review found two defects invisible to every unit test because
  the requests were well-formed and the logic correct: a connector that
  declares no request-level arguments silently ignores the per-tenant
  routing sent to it, and `affected_rows` is not an NDC concept on
  `/mutation` at all. Both are now checked at startup or refused, and both
  are now observed directly against a running connector rather than read
  from source or documentation. Every assumption that observation
  falsified, and the commit that corrected it, is F1 (`6defacb`), F2
  (`6defacb`), and F4 (`e5e2d73`) in the table above.

  **F3 is now closed too**, by PR #71 (`72aac20`, 2026-09-17, ADR 0020,
  superseding ADR 0004). Neutral update and delete could not be expressed
  against `ndc-postgres`'s generated procedures — `update_articles_by_id_and_tenant_key`
  and `delete_articles_by_id_and_tenant_key` require `key_id`/`key_tenant_key`
  arguments a neutral `MutationSpec` had nowhere to carry — so ADR 0020 lets
  a procedure mapping name which of its arguments carry the logical key and
  how an update payload is shaped, with key values taken only from the
  tenant-scoped predicate's own direct equalities, never from the request
  body. See "Keyed writes reach ndc-postgres (issue #62's F3, PR #71)" above
  for what closes it, cited by test.

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
