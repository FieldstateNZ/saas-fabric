# M2 runtime publication: gap report against #112

- **Issue:** #112, "Prove atomic runtime publication and compatible reader rollout" (W3, M2).
- **Scope:** an audit plus tests that pin current behaviour. This branch changes no
  production code, accepts no contract and decides nothing for D01-1 or D01-2.
- **Base:** `origin/main` at the commit this branch forks from. Every `file:line`
  below refers to that tree. Paths are under `crates/` unless they say otherwise.
- **Code read:** `fabric-runtime-publication` (`plan.rs`, `validate.rs`,
  `published_revisions.rs`, the filesystem adapter), `fabric-publication-kubernetes`,
  and the `fabric-tenant-runtime` reader. The catalogue reader in `fabric-api` and
  the controller in `fabric-platform-management` were read where they decide an
  answer.
- **Decisions referred to:** ADR 0018 and ADR 0023 (both Proposed);
  `docs/roadmap/m0-contract-decisions.md` D01-1 (accept or amend ADR 0018, including
  runtime-document rollback, U5.3) and D01-2 (accept or amend ADR 0023, including
  §5 against D01-18).

## Summary

Each document is replaced atomically. The three documents together are not.

- **Within one document**, a reader never sees a torn write. The filesystem adapter
  renames a fully synced sibling file over the target. The Kubernetes adapter
  writes the payload and its manifest as one `ConfigMap` in one request, and the
  kubelet swaps a mounted volume with one symlink rename.
- **Across documents**, the publisher's write order (data sources, then catalogue,
  then tenants) holds only at the API server or on the directory it writes to.
  Nothing carries that order to the reader. The runtime reloads tenants and data
  sources on two independent refreshers. In Kubernetes each document is a separate
  volume. The catalogue is read once, at process start. So a reader can serve
  documents at different revisions in either order. The tests in this branch
  reproduce this deterministically.
- **The mixed states found fail closed.** A tenant whose binding names a DataSource
  the reader has not loaded, or has already dropped, gets `MissingDataSource`, which
  is a 500. No mixed state was found that sends one tenant's request to another
  tenant's rows. The window is "a tenant is down", not "a tenant boundary is
  wrong". The window has no upper bound the publisher enforces.
- **The single writer is assumed, not enforced.** Two overlapping publishers can
  each pass every guard and together publish a binding to a DataSource that no
  longer exists. The `resourceVersion` check in the Kubernetes adapter covers only
  the objects a publication writes.
- **Last-known-good behaviour lives only in a running reader's memory.** A reader
  started afterwards cannot recover it. A held document the publisher cannot parse
  blocks every later publication until an operator repairs it.
- **Reader compatibility is a refusal, not a negotiation.** `contract_version` is
  written and never read. An unknown field stops every new reader from priming.

Against #112's acceptance criteria:

| Acceptance criterion | Status | Gaps |
|---|---|---|
| Published tenant, data-source and catalogue revisions form a consistent contract | **Not met** across documents; met within one | G1, G2, G4, G7 |
| Compatible readers precede schema activation | **Not met.** No mechanism and no version handshake | G6 |
| Interrupted publication cannot expose mixed revisions | **Not met.** It can, and so can uninterrupted publication | G1, G3 |
| Last-known-good and failure behaviour are explicit | **Partly met.** The reader side is explicit in code; restart, publisher and rollback behaviour are not | G5 |
| Catalogue selection is not claimed to deploy a product | **Met in the README** (`README.md:468-470`). Console copy was not audited | — |

## What is atomic today

| Layer | Mechanism | Evidence |
|---|---|---|
| Filesystem adapter, one file | Writes a sibling temp file, `fsync`s it, renames it over the target, then `fsync`s the directory | `fabric-runtime-publication/src/filesystem/atomic_write.rs:38-54` |
| Filesystem adapter, one document | Payload is renamed before the manifest; two renames, not one | `filesystem/write.rs:23-30` |
| Kubernetes adapter, one document | Payload and manifest are two `data` keys of one `ConfigMap`, written in one `PUT` or `POST` | `fabric-publication-kubernetes/src/object.rs:69-105`, `src/publish.rs:44-60`; ADR 0018 amendment `:696-703` |
| Kubernetes adapter, one object, one writer at a time | A `PUT` carries the `resourceVersion` it read; a 409 becomes `Unwritable` | `src/client.rs:64-107`, `src/publish.rs:53-58`; test `concurrency_tests.rs::a_second_writer_of_the_same_object_is_refused_by_its_resource_version` |
| Kubelet projection, one volume | The `..data` symlink swap is atomic per `ConfigMap` volume | ADR 0018 `:692-693`. This is Kubernetes behaviour and cannot be checked in this repository |
| Reader, one document | `JsonFileSource` reads one path in one call. A torn or partial read fails to parse, and the registry is not touched | `fabric-tenant-runtime/src/resource/sources/json_file.rs:49-66`, `resource/refresher.rs:109-114` |
| Reader, one registry swap | `apply_all` merges under a write lock and swaps one `Arc` | `resource/registry/apply_all.rs:77-112` |
| Planning | Every refusal is decided before any byte is written. The only exception is an I/O failure mid-publication | `plan.rs:56-96`, `validate.rs:34-55`, `errors.rs:19-20` |

## Gaps

Each gap gives its evidence, the test that pins it (tests named `current_behaviour_*`
pass *because* the gap exists), and a classification:

- **Engineering:** can be fixed without changing what ADR 0018 or ADR 0023 says.
- **D01-1 / D01-2:** fixing it changes or adds contract text that waits on that
  decision. Not implemented here.

### G1. Readers can apply the three documents out of order (mixed revisions)

**Evidence**

- **Publisher order:** data sources, then catalogue, then tenants
  (`filesystem/adapter.rs:66-70`; `fabric-publication-kubernetes/src/publish.rs:88-95`;
  ADR 0018 part 3, `:174-178`).
- **Reader at startup:** primes data sources before tenants
  (`fabric-tenant-runtime/src/registration.rs:91-95`). After that it starts **two
  independent refreshers**, each with its own timer and trigger
  (`registration.rs:97-100`; `resource/refresher.rs:90-116`). Nothing orders a
  data-sources reload before a tenants reload in the same cycle.
- **Kubernetes:** each document is its own `ConfigMap` and its own volume
  (ADR 0018 `:599-603`). The atomic symlink swap is per volume. ADR 0018's
  amendment (`:700-703`) says the cross-document order is "write data sources, then
  catalogue, then tenants". That orders writes at the API server, not projection
  into a pod. ADR 0018's own staleness budget (`:711-715`) treats kubelet sync as a
  per-volume delay.
- **Catalogue:** read once, at process start (`fabric-api/src/startup/catalog.rs:7-16`,
  `startup/serving.rs:63`). ADR 0018 says so (`:801-811`). A running replica pairs
  catalogue revision N with whatever tenants revision it has refreshed to. A replica
  started mid-publication pairs whatever each volume held when it mounted.
- **The code already names the window:** `ResolveError::MissingDataSource`'s own doc
  says "the DataSource registry has not caught up with a tenant binding that
  references a new DataSource" (`errors/resolve_error.rs:50-53`). The
  `RuntimeConfig` doc explains why both refreshers share one interval
  (`config.rs:5-9`). The Data API maps the state to 500
  (`fabric-data-api/src/errors/status_mapping.rs:42`).

**Effect:** a tenant moved onto a new DataSource fails closed with a 500 until the
data-sources refresher catches up. No mixed state found here serves another
tenant's rows. Two reasons:

- Resolution refuses a binding to a DataSource it does not hold
  (`resolution/runtime_resolver.rs:129-144`).
- ADR 0023 makes the discriminator value the tenant id (`0023:365-369`).

One case was not tested: if a provisioner ever reassigned discriminator values, a
pairing of a new DataSource with an old tenants revision could match rows the value
no longer owns. Re-check this when D01-3 defines provisioner output.

**Test:**
`fabric-runtime-publication/tests/publication_atomicity.rs::current_behaviour_a_reader_can_apply_new_tenants_before_the_data_sources_they_name`
(:139). It publishes a correct snapshot, refreshes only the tenants registry, and
observes `MissingDataSource` and a 500 for acme. globex is unaffected. Refreshing
data sources then resolves acme.

**Classification**

- **G1a, reader refresh order: engineering.** One refresher could reload data
  sources and then tenants in one pass. Alternatively, the tenants refresher could
  defer a binding whose DataSource is not yet loaded and keep the held copy, as it
  already does for an invalid entry (`registry/merge.rs:49-57`). Neither changes the
  wire shape. Both touch the runtime plane, which ADR 0018's Consequences
  (`:765-770`) ask to keep frozen for that milestone. Flag this in review.
- **G1b, one consistent generation across the three documents: D01-1.** A cross-document
  generation, a single object, or a reader that reads the manifests replaces "three
  independently versioned documents" (ADR 0018 Decision, `:84-86`; part 2,
  `:164-167`). The reader would also have to consume the manifests, which it
  deliberately does not (`:349-351`). Kubelet projection makes per-volume skew
  unavoidable without such a change.

### G2. Retiring a DataSource in two publications is ordered only at the publisher

**Evidence**

- `refuse_retired_data_source_still_bound` checks the incoming data sources
  against the **held** tenants document (`validate.rs:113-131`). This is ADR 0018
  part 3, `:183-195`.
- It proves that the unbinding publication was *written*. It does not prove any
  reader *applied* it. A second publication can follow immediately; nothing waits
  for the propagation budget in `:711-715`.
- A reader whose data-sources refresher runs first applies the retirement while its
  tenant registry still binds the retired DataSource.

**Test:** `publication_atomicity.rs::current_behaviour_a_reader_can_apply_a_retirement_before_the_unbinding_that_preceded_it`
(:194). It publishes the unbinding and the retirement back to back, both accepted,
then refreshes only the data sources. acme gets `MissingDataSource` until the tenants
refresh.

**Classification: D01-1.** ADR 0018 part 3 states the rule as "one publication
after". Making it "one publication after the unbinding is observed by readers", or
adding a minimum dwell, rewrites that rule. A dwell enforced by the controller is
engineering once the rule says so. The G1a reader fix also narrows this window.

### G3. An interrupted publication leaves the published set at mixed revisions

**Evidence**

- Every refusal is all-or-nothing **except** an I/O failure during writing. That
  surfaces as `Unwritable` and leaves the documents already written in place
  (`errors.rs:19-20`; `report.rs:34-50`). Both adapters behave this way:
  `filesystem/adapter.rs:68-70` and `fabric-publication-kubernetes/src/publish.rs:90-95`,
  where a 409 or 5xx on a later object aborts after earlier objects are replaced.
- **Recovery** is the next pass. Republishing the same snapshot finishes the job
  (existing test `filesystem_runtime_publication.rs::a_publication_that_failed_between_documents_is_completed_by_the_next_one`,
  :663). The controller's schedule provides that next pass
  (`fabric-control-plane-api/src/startup/platform/publishing.rs`).
- **Between the failure and the next pass:**
  - Readers serve the half-applied set.
  - `current()` reports the mixed revisions.
  - Nothing marks the publication as incomplete for a reader.
- Write order makes the half-applied state safe for *additions*. For a coupled
  change it is not intended to be served. One example: a DataSource whose connection
  moves, together with tenant isolation that changes to match.

**Tests**

- `publication_atomicity.rs::current_behaviour_an_interrupted_publication_leaves_readers_serving_mixed_revisions`
  (:247). The filesystem adapter fails on tenants after data sources land.
  `current()` reads tenants 1, data sources 2, catalogue 1. A refreshed reader holds
  the new DataSource and the old binding. A republish completes.
- `fabric-publication-kubernetes/src/concurrency_tests.rs::current_behaviour_an_interrupted_publication_leaves_the_cluster_at_mixed_revisions`
  (:366). This is the same check against a stateful fake API server that fails the
  tenants `PUT`.

**Classification**

- **Engineering:** an immediate retry inside the pass for `Unwritable`, rather than
  waiting for the next tick. Also surfacing "incomplete" on the platform panel.
- **D01-1:** making interruption invisible to readers, as #112 asks. This needs
  G1b's cross-document generation. No ordering of three independent objects
  achieves it.

### G4. "Exactly one writer" is assumed, not enforced; overlapping writers can publish a dangling binding

**Evidence**

- ADR 0018 answers concurrency with "exactly one writer" (`:605-607`) and puts
  multiple writers out of scope (`:859-862`).
- The controller's single-flight guard is **in-process only**
  (`fabric-platform-management/src/publication/publisher.rs:155-158`,
  `state.running.try_enter()`). It covers the schedule and
  `POST /api/platform/publication` in one process, not two control-plane replicas.
  The replica count on LucentRoot is not verified here.
- **Kubernetes adapter:**
  - Reads the three objects with three independent `GET`s; this is not a
    consistent read (`src/held.rs:28-34`).
  - Sends `resourceVersion` only on the objects it writes (`src/publish.rs:44-60`).
  - A plan that relied on an object it did not write, such as the held tenants
    document behind the retirement guard, is never checked against that object
    again. This is write skew.
- **Filesystem adapter:**
  - Holds no lock between `read_held` and the writes (`filesystem/adapter.rs:62-70`).
  - Stages every write of a given file through one fixed sibling path,
    `.{file}.tmp` (`filesystem/atomic_write.rs:80-85`). Two overlapping writers of
    the same document truncate and rename the same temp file.
  - The overlapping-writer tests below do not exercise this temp-path collision.
    It is stated from the code.
- **Within one document**, the divergence guard (ADR 0018 part 6, `verdict.rs:96-107`)
  only sees writers that run one after the other. Two writers planning the same
  next revision both get `Write`.

**Tests**

- `fabric-publication-kubernetes/src/concurrency_tests.rs::current_behaviour_overlapping_writers_can_leave_a_dangling_binding_in_the_cluster`
  (:280):
  - Writer A retires `pg-2`, which is legal against the tenants it read. It is held
    before its write.
  - Writer B moves acme onto `pg-2`, which is legal against the data sources it read.
  - Both succeed. The fake cluster ends with `tenants.json` naming `pg-2` and
    `data-sources.json` without it.
- `fabric-runtime-publication/src/filesystem/concurrency_tests.rs::current_behaviour_two_overlapping_publishers_can_publish_a_dangling_binding`
  (:168). This is the same skew through the filesystem adapter's own `read_held`,
  `plan_publication` and `write_if_needed`. The next publication of writer A's own
  snapshot is then refused as `RetiredDataSourceStillBound`, a state the plan
  exists to make unreachable.
- `filesystem/concurrency_tests.rs::current_behaviour_two_overlapping_publishers_at_one_revision_both_succeed_and_the_last_wins`
  (:226). This is a lost update at one document revision.
- Positive control:
  `concurrency_tests.rs::a_second_writer_of_the_same_object_is_refused_by_its_resource_version`
  (:321). Within one object, the API server's optimistic concurrency refuses the
  second writer.

**Classification: engineering, plus a platform RBAC change.** Enforcing the single
writer the ADR already assumes contradicts nothing written. Options:

- A `coordination.k8s.io` `Lease` held by the publishing replica. ADR 0018 `:861`
  names leader election as "the shape of the answer". This needs `Lease` verbs in
  the platform repository's Role.
- Asserting the `resourceVersion` of the objects a plan read but does not write. No
  multi-object transaction exists in the Kubernetes API, so this narrows the window
  but does not close it.
- An exclusive lock file for the filesystem adapter.

**Needs D01-1 only** if the decision is to support multiple writers rather than
enforce one.

### G5. Last-known-good is in-memory, one-sided and lost on restart

**Evidence (reader)**

- An unreadable or malformed document leaves the registry untouched and logs
  `refresh_failed` (`resource/refresher.rs:109-114`).
- An invalid single resource keeps its held copy (`registry/merge.rs:49-57`).
- A first load that would install nothing is refused, and the replica stays
  unprimed and answers 503 (`registry/apply_all.rs:89-91`). Existing tests:
  `published_state_serves_two_tenants.rs::a_failed_refresh_leaves_the_runtime_serving_the_last_good_snapshot`
  (:370) and `::a_malformed_published_document_does_not_deprovision_the_tenants_already_serving`
  (:419).
- None of this survives a restart. A replica started against the same files has no
  earlier snapshot.
- With the default `fail_fast_on_prime = true` (`config.rs:38`), the process exits
  (`registration.rs:113-119`), which is a crash loop under a Deployment.
- A malformed catalogue fails startup outright (`fabric-api/src/startup/catalog.rs:24-29`).

**Evidence (publisher)**

- There is no last-known-good record and no hold. A valid but wrong snapshot
  publishes and is served at the next refresh.
- A held document that does not parse blocks **every** later publication, even at a
  newer revision. `plan_publication` parses held tenants and data sources before
  anything else (`plan.rs:60-69`, `plan/parse.rs:42-50`) and returns `Unreadable`.
  The operator must repair or remove the held payload. No supported recovery path
  exists.
- Rollback is unresolved: U5.3 recommends roll-forward only, and it is part of
  D01-1 (`m0-contract-decisions.md:654-665`, `:861`).

**Test:** `publication_atomicity.rs::current_behaviour_last_known_good_does_not_survive_a_reader_restart`
(:376). A running reader keeps serving a torn data-sources document. A fresh
`build_runtime` against the same files fails. The publisher refuses a revision-2
publication as `Unreadable`.

**Classification**

- **D01-1:** the rollback and failure semantics: roll-forward only, what a refused or
  unparseable held document means, and whether a reader should persist or prefer a
  last-known-good.
- **D01-2:** the pre-publication hold #112's scope names. Who may hold a
  publication, and how it relates to ADR 0023 §5's raise gate (D01-18), is
  ownership text in ADR 0023 §4 and §5 (`0023:276-325`).
- **Engineering, once those are decided:** a documented recovery command for an
  unparseable held document; surfacing reader `refresh_failed` counts on the
  platform panel.

### G6. Reader compatibility is enforced by refusal; nothing orders reader rollout before a new shape

**Evidence**

- The consumer's types deny unknown fields (`tenant/tenant_runtime_binding.rs:27`,
  `data_source/data_source_resource.rs:31`,
  `fabric-data-api/src/catalog/resource_definition.rs:27`; ADR 0018 `:100-110`).
- `contract_version` is written into every manifest
  (`fabric-runtime-publication/src/manifest.rs:63-92`). No reader reads any manifest.
  A search of `fabric-tenant-runtime`, `fabric-data-api` and `fabric-api` finds no
  reference to manifests or `contract_version`. ADR 0018 says this is deliberate
  (`:349-351`).
- ADR 0018 §9's migration path is "new file names and new `ConfigMap` keys alongside
  the old ones" (`:354-356`). The publisher has one fixed key per document
  (`fabric-publication-kubernetes/src/object.rs:23-39`) and cannot dual-write.
- Nothing tells the publisher which reader versions are running. That belongs with
  the release compatibility manifest (#124, D01-21).

**Test:** `publication_atomicity.rs::current_behaviour_a_forward_incompatible_document_keeps_running_readers_and_stops_new_ones`
(:306). A tenants document gains one unknown field. The running reader keeps its
last good snapshot. A new reader with `fail_fast_on_prime = true` fails to build,
and the error names the field. With `false` it starts unprimed and answers
`RuntimeUnavailable`.

**Reader-compatibility matrix (current behaviour)**

| Producer change | Running reader (holds a snapshot) | Reader starting now | Evidence |
|---|---|---|---|
| New revision, same shape | Applied at next refresh (catalogue: never, until restart) | Primes | existing suite; `catalog.rs:7-16` |
| Field added at any level of tenants or data sources | Whole document refused as `Malformed`; stays on its last good snapshot and logs `refresh_failed`; **silently stale** | `fail_fast_on_prime = true`: exits. `false`: unprimed, 503 | new test (:306) |
| Required field removed | As above (missing-field error) | As above | ADR 0018 `:100-110` (not separately tested) |
| New enum variant (isolation kind, placement class, connection kind) | As above. Inferred from serde's tagged enums and `deny_unknown_fields`, not separately tested | As above | type definitions |
| One resource invalid (for example an empty `data` map) | That resource keeps its held copy; the rest apply | Dropped. If nothing survives: `UnusableFirstLoad`, unprimed | `registry/merge.rs:41-58` |
| Catalogue shape change | No effect: not re-read | Startup fails | `fabric-api/src/startup/catalog.rs:24-29` |
| `contract_version` bumped, shape unchanged | Ignored | Ignored | no reader of the field |
| v2 file names beside v1 (ADR 0018 §9) | Unaffected: reads its configured path | Unaffected | ADR 0018 `:677-678`. Publisher does not support it |

**Classification**

- **D01-1:** the versioning rule. That means either making the reader consume
  `contract_version`, or building §9's dual-key path and the rule for when the old
  key may stop being written. Either is ADR 0018 §9 text.
- **Engineering:** extending this matrix into a test per row, and a CI check that a
  producer change ships with the consumer change it needs (ADR 0018 Consequences,
  `:830-833`).

### G7. Nothing reports which revisions a reader is serving

**Evidence**

- `PublishedRevisions` is three independent `Option<DocumentRevision>`s, with no
  shared generation (`published_revisions.rs:12-20`).
- The reader never reads a manifest (G6), so no replica can report the document
  revisions it serves.
- The catalogue revision has no consumer at all (ADR 0018 `:806-811`).
- ADR 0023 §5's gate for raising the runtime reads the controller's report of what
  it **published** (`0023:316-325`), not what readers **loaded**.

**Test:** none. This is an absence. G1 to G3 show the consequence.

**Classification**

- **D01-1:** reader-side revision reporting changes the consumption shape.
- **D01-2:** what §5's gate checks (D01-18).

## What this branch adds

**Tests:** ten new tests. All run by default; none is ignored. Nine are named
`current_behaviour_*` and pin a gap above. One is a positive control.

| File | Tests |
|---|---|
| `crates/fabric-runtime-publication/tests/publication_atomicity.rs` | 5 (G1, G2, G3, G5, G6), driven through the real `FilesystemRuntimePublication`, `build_runtime` and `JsonFileSource` |
| `crates/fabric-runtime-publication/src/filesystem/concurrency_tests.rs` | 2 (G4), a deterministic interleaving of two plans through the adapter's own read, plan and write steps |
| `crates/fabric-publication-kubernetes/src/concurrency_tests.rs` | 3 (G3, G4, plus the positive `resourceVersion` control), against a stateful fake API server that enforces `resourceVersion` and can hold one writer mid-publication |

**Production code:** none changed.

## Not covered here

- **Live publication.** #112 requires separate authorization. Kubelet projection
  order is stated from Kubernetes behaviour and ADR 0018. No cluster tests it.
- **The control-plane replica count on LucentRoot,** and whether platform #40/#41
  wiring matches ADR 0018's RBAC amendment.
- **Console copy** about catalogue selection and deployment.
- **The Data API's request-path tests** beyond `MissingDataSource` → 500.
