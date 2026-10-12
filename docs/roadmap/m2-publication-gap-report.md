# M2 runtime publication: gap report against #112

- **Issue:** #112, "Prove atomic runtime publication and compatible reader rollout" (W3, M2).
- **Scope:** an audit plus tests that pin current behaviour. This branch changes no
  production code, accepts no contract and decides nothing for D01-1 or D01-2.
- **Base:** `origin/main` at the commit this branch forks from. Every `file:line`
  below refers to that tree. Paths are under `crates/` unless they say otherwise.
- **Since then:** the engineering-only parts of G1, G3, G4 and G4a are fixed, and
  under D01-2's disposition (#108) the reader-side parts of G1b, G2, G3 and G6
  are addressed by a publication record and one resolution snapshot per
  generation, within the limits listed under
  [Reader generations](#reader-generations). Resource revision history, the
  placement and catalogue guards, and Git-revert characterization tests
  follow, under [Revision history, placements and the catalogue](#revision-history-placements-and-the-catalogue)
  (O3, O4, O5, G8). The release rollback floor (D01-21, #124) follows
  under [Release rollback floor](#release-rollback-floor-d01-21). The
  reader's persisted last-known-good generation, its hold and fail-closed
  rules and its recovery command (G5, reader side) follow under
  [Persisted last-known-good](#persisted-last-known-good-g5). G3's panel
  state, the target's observed completeness, follows under
  [Publication completeness (G3)](#publication-completeness-g3). The
  publisher's recovery command for a held document it cannot parse (G5,
  publisher side) follows under
  [Publisher-side recovery (G5)](#publisher-side-recovery-g5). Test names in the gap sections are as first
  written; see [Engineering fixes](#engineering-fixes),
  [Reader generations](#reader-generations) and the section after it for
  what changed and what each test is now called.
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

With a single writer, each document is replaced atomically. The three documents
together are not atomic, even with a single writer.

- **Within one document, with exactly one writer**, a reader never sees a torn write.
  - The filesystem adapter renames a fully synced sibling file over the target.
  - The Kubernetes adapter writes the payload and its manifest as one `ConfigMap`
    in one request, and the kubelet swaps a mounted volume with one symlink rename.
  - The Kubernetes write is atomic at the API server whatever the number of writers.
  - The filesystem adapter's is not. Two overlapping writers of one document share
    one staging file, and they can publish an empty or interleaved, non-JSON
    document (G4a). Nothing enforces the single writer (G4).
- **Across documents**, the publisher's write order (data sources, then catalogue,
  then tenants) holds only at the API server or on the directory it writes to.
  Nothing carries that order to the reader. The runtime reloads tenants and data
  sources on two independent refreshers. In Kubernetes each document is a separate
  volume. The catalogue is read once, at process start. So a reader can serve
  documents at different revisions in either order. The tests in this branch
  reproduce this deterministically.
- **The mixed states tested fail closed.**
  - A tenant whose binding names a DataSource the reader has not loaded, or has
    already dropped, gets `MissingDataSource`, which is a 500 (G1, G2).
  - A catalogue entry on a logical data source the tenant does not bind gets
    `UnboundDataSource`, also a 500 (G8).
  - No mixed state tested sends one tenant's request to another tenant's rows. The
    window is "a tenant is down", not "a tenant boundary is wrong".
  - The window has no upper bound the publisher enforces.
  - Not tested: a *coupled* change served half-applied (G3), such as a DataSource
    connection that moves together with a change to tenant isolation.
- **The single writer is assumed, not enforced.** Two overlapping publishers can
  each pass every guard and together publish a binding to a DataSource that no
  longer exists. The `resourceVersion` check in the Kubernetes adapter covers only
  the objects a publication writes. In the filesystem adapter, two overlapping
  writers of one document can also publish an empty or non-JSON file (G4a).
- **Last-known-good behaviour lives only in a running reader's memory.** A reader
  started afterwards cannot recover it. A held tenants or data-sources document the
  publisher cannot parse blocks every later publication until an operator repairs it.
- **Reader compatibility is a refusal, not a negotiation.** `contract_version` is
  written, and no runtime consumer reads it. An unknown field stops every new reader from priming.

Against #112's acceptance criteria:

| Acceptance criterion | Status | Gaps |
|---|---|---|
| Published tenant, data-source and catalogue revisions form a consistent contract | **Not met** across documents. Met within one document only while there is a single writer (G4a) | G1, G2, G4, G4a, G7, G8 |
| Compatible readers precede schema activation | **Not met.** No mechanism and no version handshake | G6 |
| Interrupted publication cannot expose mixed revisions | **Not met.** It can, and so can uninterrupted publication | G1, G3 |
| Last-known-good and failure behaviour are explicit | **Partly met.** The reader side is explicit in code, including across a restart when a last-known-good directory is configured (off by default; no platform mount yet). A held document the publisher cannot parse can be restored from the bytes the record names (source only). The publisher hold, and any discard, are not | G5 |
| Catalogue selection is not claimed to deploy a product | **Met in the README** (`README.md:468-470`). Console copy was not audited | — |

## What is atomic today

| Layer | Mechanism | Evidence |
|---|---|---|
| Filesystem adapter, one file, **one writer only** | Writes a sibling temp file, `fsync`s it, renames it over the target, then `fsync`s the directory. The temp path is fixed per target, so this holds only while one writer stages that file; see G4a. The function's rustdoc ("nothing in between, on every platform", `:11-15`) states it without that condition | `fabric-runtime-publication/src/filesystem/atomic_write.rs:38-54`, `:80-85` |
| Filesystem adapter, one document | Payload is renamed before the manifest; two renames, not one | `filesystem/write.rs:23-30` |
| Kubernetes adapter, one document | Payload and manifest are two `data` keys of one `ConfigMap`, written in one `PUT` or `POST`, so the API server replaces the object whole whatever the number of writers | `fabric-publication-kubernetes/src/object.rs:69-105`, `src/publish.rs:44-60`; ADR 0018 amendment `:696-703` |
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

- **G1a, reader-side mitigation: engineering, as a runtime-plane change to review
  against ADR 0018.**
  - Option 1: one refresher reloads data sources and then tenants in one pass. This
    helps only where both files change together, as in the filesystem layout. It
    does not help in Kubernetes, where each document is its own volume and the
    kubelet projects each one on its own.
  - Option 2: the tenants refresher defers a binding whose DataSource is not yet
    loaded and keeps the held copy, as it already does for an invalid entry
    (`registry/merge.rs:49-57`). This helps in both layouts. It changes documented
    reader behaviour (`errors/resolve_error.rs:50-53`).
  - Neither option changes the wire shape. Both touch the runtime plane, which ADR
    0018's Consequences (`:765-770`) ask to keep frozen.
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
- **Recovery** is the next pass, provided the held tenants and data-sources
  documents still parse. A held document that does not parse, for example one
  corrupted by G4a, refuses every pass (G5). Republishing the same snapshot
  finishes the job (existing test `filesystem_runtime_publication.rs::a_publication_that_failed_between_documents_is_completed_by_the_next_one`,
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
  (`fabric-platform-management/src/publication/publisher.rs:85-88`,
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
    the same document truncate and rename the same temp file. See G4a for the
    published corruption this causes.
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

**Classification: mostly engineering.** Enforcing the single writer the ADR already
assumes. Options:

- A `coordination.k8s.io` `Lease` held by the publishing replica. ADR 0018 `:861`
  names leader election as "the shape of the answer".
  - The publisher's RBAC is specified in ADR 0018 (`:614-618`, `:674`, and the
    amendment at `:619-628`) and in ADR 0023 (`:290-291`). That text covers
    `ConfigMap` verbs only.
  - Adding `Lease` verbs, including a `create` that `resourceNames` cannot scope,
    amends that text. This option is therefore **D01-1** (amend "as built"), plus
    the platform repository's Role.
- Asserting the `resourceVersion` of the objects a plan read but does not write. No
  multi-object transaction exists in the Kubernetes API, so this narrows the window
  but does not close it.
- An exclusive lock file for the filesystem adapter.

**Needs D01-1 only** if the decision is to support multiple writers rather than
enforce one.

#### G4a. Overlapping filesystem writers can publish an empty or non-JSON document

**Evidence**

- `atomic_write` stages through `sibling_temp_path(target)`, which is the same path
  for every writer of that target (`filesystem/atomic_write.rs:80-85`).
- `write_and_sync` starts with `File::create` (`:88-91`). That call truncates
  whatever inode is at the staging path, even another writer's staged bytes.
- The rename (`:42`) moves that inode over the target. A writer that still holds
  an open handle to it then writes into the published file.
- This breaks the per-document atomicity that `atomic_write`'s rustdoc promises
  (`:11-15`) and that ADR 0018 part 5 (`:219-223`) relies on.
- The result is not a stale or mixed document but an invalid one:
  - every runtime reader refuses it (`json_file.rs:62-65`) and keeps its last good
    snapshot;
  - a reader that starts afterwards fails to prime (G5);
  - what the publisher does next depends on the document:
    - **tenants or data sources:** every later pass is refused as `Unreadable`, even
      at a higher revision, because the plan parses both held payloads before
      anything else (`plan.rs:60-69`, `plan/parse.rs:42-50`). An operator must
      repair or remove the file;
    - **catalogue:** the plan never parses the held catalogue, only compares its
      bytes. An ordinary publication at a higher revision replaces it; one at the
      held revision is refused as `DivergentPayload`, and the controller then
      re-offers at the next revision (`protocol.rs`).
  - Since the G4a fix (see [Engineering fixes](#engineering-fixes)), overlapping
    writers through the adapter can no longer produce such a file. A document
    corrupted another way, by a hand edit or a writer that bypasses the adapter,
    still recovers as above.
- The Kubernetes adapter does not have this failure mode. The API server replaces
  one object whole.

**Tests** (both in `fabric-runtime-publication/src/filesystem/atomic_write_tests.rs`;
deterministic: writer A's steps are the literal statements of `write_and_sync` and
`atomic_write` run one at a time, and writer B is the real `atomic_write`)

- `::current_behaviour_overlapping_writers_of_one_document_can_publish_interleaved_bytes`
  (:59):
  - Writer A opens the staging file.
  - Writer B completes a whole `atomic_write`.
  - A's write lands in the now-published inode, and A's rename fails.
  - B reported success. The published file is A's bytes followed by the tail of
    B's, which is not JSON.
- `::current_behaviour_overlapping_writers_of_one_document_can_publish_an_empty_file`
  (:86):
  - Writer A finishes staging.
  - Writer B's `File::create` truncates A's staged bytes.
  - A's rename publishes an empty file. It stays empty if writer B stops at that
    point.

**Classification: engineering.** ADR 0018 part 5 requires a *sibling* temporary
file in the same directory; it does not require a fixed name. Either of these fits
the existing text:

- a unique staging name per write, created with `create_new`;
- an exclusive lock held across `read_held` and the writes, which also closes the
  skew in G4.

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
- With the default `fail_fast_on_prime = true` (`config.rs:38`), `build_runtime`
  returns an error (`registration.rs:113-119`). Its caller propagates the error and
  the process fails to start (`fabric-api/src/startup/application.rs:57-61`).
  Under a Deployment, that is a crash loop (inferred; compare ADR 0023 `:323-325`).
- A malformed catalogue fails startup outright (`fabric-api/src/startup/catalog.rs:24-29`).

**Evidence (publisher)**

- There is no last-known-good record and no hold. A valid but wrong snapshot
  publishes and is served at the next refresh.
- A held tenants or data-sources payload that does not parse blocks **every** later
  publication, even at a newer revision. The catalogue is only byte-compared, so a
  corrupt held catalogue does not block.
  - `plan_publication` parses held tenants and data sources before anything else
    (`plan.rs:60-69`, `plan/parse.rs:42-50`) and returns `Unreadable`.
  - A held manifest whose payload is gone blocks in the same way, as
    `HeldPayloadLost` (`plan/parse.rs:85-86`).
  - Either way the operator must repair or remove the held files. No supported
    recovery path exists.
- Rollback is unresolved: U5.3 recommends roll-forward only, and it is part of
  D01-1 (`m0-contract-decisions.md:654-665`, `:861`).

**Test:** `publication_atomicity.rs::current_behaviour_last_known_good_does_not_survive_a_reader_restart`
(:389). A running reader keeps serving a torn data-sources document. A fresh
`build_runtime` against the same files fails. The publisher refuses a revision-2
publication as `Unreadable`.

**Classification**

- **D01-1:** the rollback and failure semantics: roll-forward only, what a refused or
  unparseable held document means, and whether a reader should persist or prefer a
  last-known-good.
- **D01-1:** the pre-publication hold #112's scope names. A hold is publisher
  behaviour, which ADR 0018 owns. How a hold relates to ADR 0023 §5's raise gate is
  D01-18, which D01-2 sequences against §5 (`m0-contract-decisions.md:862`).
- **Engineering, once those are decided:** a documented recovery command for an
  unparseable held document; surfacing reader `refresh_failed` counts on the
  platform panel.

### G6. Reader compatibility is enforced by refusal; nothing orders reader rollout before a new shape

**Evidence**

- The consumer's types deny unknown fields (`tenant/tenant_runtime_binding.rs:27`,
  `data_source/data_source_resource.rs:31`,
  `fabric-data-api/src/catalog/resource_definition.rs:27`; ADR 0018 `:100-110`).
- `contract_version` is written into every manifest
  (`fabric-runtime-publication/src/manifest.rs:63-92`). No runtime consumer reads
  any manifest; only the publisher and its adapters do. A search of `fabric-tenant-runtime`, `fabric-data-api` and `fabric-api` finds no
  reference to manifests or `contract_version`. ADR 0018 says this is deliberate
  (`:349-351`).
- ADR 0018 §9's migration path is "new file names and new `ConfigMap` keys alongside
  the old ones" (`:354-356`). The publisher has one fixed key per document
  (`fabric-publication-kubernetes/src/object.rs:23-39`) and cannot dual-write.
- Nothing tells the publisher which reader versions are running. That belongs with
  the release compatibility manifest (#124, D01-21).

**Test:** `publication_atomicity.rs::current_behaviour_a_forward_incompatible_document_keeps_running_readers_and_stops_new_ones`
(:306). A tenants document gains one unknown field and drops acme, so acceptance
would be visible. The running reader keeps its
last good snapshot. A new reader with `fail_fast_on_prime = true` fails to build,
and the error names the field. With `false` it starts unprimed and answers
`RuntimeUnavailable`.

**Reader-compatibility matrix (current behaviour)**

| Producer change | Running reader (holds a snapshot) | Reader starting now | Evidence |
|---|---|---|---|
| New revision, same shape | Applied at next refresh (catalogue: never, until restart) | Primes | existing suite; `catalog.rs:7-16` |
| Field added to tenants or data sources | Whole document refused as `Malformed`; stays on its last good snapshot and logs `refresh_failed`; **silently stale** | `fail_fast_on_prime = true`: fails to start. `false`: unprimed, 503 | Tested at the top level of a tenant binding (new test, :306). Nested types also deny unknown fields: `tenant_data_binding.rs:26`, `pool_settings.rs:21`, `capabilities.rs:26`, `residency.rs:32`, and the tagged isolation and connection documents (`fabric-connector/src/execution/tagged_documents.rs:59`, `:87`) |
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

**Since then:** under the G7 disposition on #108, the following are built in
source:
- each replica's served generation, taken from the snapshot resolution reads;
- an off-by-default listener that reports it;
- the publisher's record stamp;
- a read-only observer that samples every replica set's pods;
- the assessment, with reader evidence shown beside the documents gate.

Live use of the listener is not approved. See
[Reader generation reporting (G7)](#reader-generation-reporting-g7).

### G8. The catalogue and the tenants document are never checked against each other

**Evidence**

- `validate_snapshot`'s only catalogue rule is `refuse_empty_catalogue`
  (`validate.rs:39`, `:59-64`).
- A catalogue entry on a logical data source that some tenant does not bind is
  published, and that tenant gets `UnboundDataSource`, a 500
  (`fabric-data-api/src/errors/status_mapping.rs:41`). This can happen within one
  internally consistent publication.
- ADR 0018 part 4 chooses this deliberately (`:204-217`), as "a reconciliation gap
  on the platform's side".
- Across revisions it compounds G1. The catalogue is read only at start, so a
  restarted replica pairs the newest catalogue with whatever tenants revision its
  volume holds. A running replica keeps its startup catalogue against newer tenants.

**Test:** none new. ADR 0018 part 4 cites the shipped `examples/catalog.json` as
this shape.

**Classification**

- **D01-1:** refusing or holding such a publication reverses part 4.
- **D01-1:** a generation that ties the catalogue to the tenants revision is G1b.
- **Engineering:** surfacing the unbound pairs on the client's Data tab, which ADR
  0023 already owes (`0023:411-415`).

**Since then:** under D01-1's disposition, an entry that *no* tenant binds is
refused, the controller keeps the last published catalogue while one exists,
and a runtime takes its catalogue from the same read and record as the
generation it primes. An entry that *some* tenant does not bind is still
published, as part 4 says. The Data tab item and catalogue refresh in a
running replica are still owed. See
[Revision history, placements and the catalogue](#revision-history-placements-and-the-catalogue).

## What this branch adds

**Tests:** twelve new tests. All run by default; none is ignored. Eleven are named
`current_behaviour_*` and pin a gap above. One is a positive control.

| File | Tests |
|---|---|
| `crates/fabric-runtime-publication/tests/publication_atomicity.rs` | 5 (G1, G2, G3, G5, G6), driven through the real `FilesystemRuntimePublication`, `build_runtime` and `JsonFileSource` |
| `crates/fabric-runtime-publication/src/filesystem/concurrency_tests.rs` | 2 (G4), a deterministic interleaving of two plans through the adapter's own read, plan and write steps |
| `crates/fabric-runtime-publication/src/filesystem/atomic_write_tests.rs` | 2 (G4a), a deterministic interleaving of two writers' staging of one document |
| `crates/fabric-publication-kubernetes/src/concurrency_tests.rs` | 3 (G3, G4, plus the positive `resourceVersion` control), against a stateful fake API server that enforces `resourceVersion` and can hold one writer mid-publication |

**Production code:** none changed. The only non-test edits are three `#[cfg(test)]`
module declarations.

## Engineering fixes

The parts classified **engineering** above, needing no change to ADR 0018 or
ADR 0023 text and no RBAC change, are fixed. Everything classified D01-1 or
D01-2 is untouched.

| Gap | Fix | Fixed-behaviour tests | Residual `current_behaviour_*` test |
|---|---|---|---|
| G1a | One refresh loop reloads data sources, then tenants, on every pass (`fabric-tenant-runtime` `ResourceRefresher::spawn_in_order`). Option 1 of G1a; option 2 (defer a binding whose DataSource is not loaded) is not taken, because it would serve a binding the publisher has replaced instead of failing closed, which changes the reader behaviour ADR 0018 part 3 states. Each reload is abandoned after one refresh interval, so a hung data-sources load cannot hold back tenant refreshes; a panic in either reload stops both. No wire shape or consumption rule changes, but this edits `crates/fabric-tenant-runtime/src`, which ADR 0018's Consequences (`:765-770`) ask to keep frozen, and replaces the public `RuntimeHandles { tenants, data_sources }` fields with `refresh_now()`/`shutdown()`, a breaking API change for in-workspace callers only (`fabric-api`). It needs explicit reviewer sign-off against ADR 0018 | `publication_atomicity.rs::a_reader_applies_new_data_sources_before_the_tenants_that_name_them`; `registration_tests.rs::every_refresh_reloads_data_sources_before_tenant_bindings`, `::a_hung_data_source_load_does_not_hold_back_tenant_refreshes` | `publication_atomicity.rs::current_behaviour_a_reader_can_apply_tenants_that_reach_it_before_the_data_sources_they_name` (per-volume skew, G1b). Also residual, untested: a publication that lands between the two reads of one pass, so the reader takes old data sources and new tenants until the next pass; reading tenants first would instead break G2's retirement order. Both are G1b (D01-1) |
| G3 | The controller re-offers an `Unwritable` snapshot at once, up to twice, in the same pass (`fabric-platform-management` `protocol.rs`); the pass outcome still reports documents the interrupted offer wrote (`report_outcome.rs`) | `protocol_tests.rs::an_interrupted_publication_is_reoffered_at_once_and_completes`, `::an_interruption_that_persists_is_reported_after_the_retry_budget`, `::a_divergence_and_an_interruption_in_one_pass_each_use_their_own_budget`; `publisher_tests.rs::a_pass_interrupted_part_way_is_completed_in_the_same_pass_and_reports_every_write` | Both G3 tests unchanged: the adapters still leave mixed revisions until the re-offer |
| G4 (filesystem) | `publish` holds an exclusive advisory lock on `.{tenants file}.lock` from `read_held` to the last write; a second publication is refused with `Unwritable`, nothing written (`filesystem/lock.rs`). Also removes the shared temp-path collision for publishers through the adapter | `filesystem/concurrency_tests.rs::a_second_publisher_is_refused_while_the_first_holds_the_lock`, `::the_lock_is_released_after_every_publication`. Since the follow-up to #163, the lock is unlocked explicitly when released, not only closed. A process spawned from another thread inherits the locked open file description until it execs, and that copy used to keep the lock held, refusing the next sequential publication: `::a_copy_of_the_lock_inherited_by_a_spawned_process_does_not_outlive_the_publication`; `publication_lock_and_spawned_processes.rs` | `::current_behaviour_two_publishers_that_bypass_the_lock_can_publish_a_dangling_binding`, `::current_behaviour_two_publishers_that_bypass_the_lock_at_one_revision_both_succeed_and_the_last_wins` |
| G4a | `atomic_write` creates its staging file with `create_new`, so a second writer of the same document is refused with `AlreadyExists` and neither truncates nor writes into the first writer's file; it never removes a staging file it did not create. `publish` clears a staging file a crashed writer left (`remove_stale_staging`) while it holds the G4 lock | `atomic_write_tests.rs::a_second_writer_cannot_stage_over_the_first_and_one_whole_document_is_published`, `::a_second_writer_cannot_truncate_the_first_writers_staged_bytes` (both flipped from `current_behaviour_overlapping_writers_*`), `::a_refused_writer_does_not_remove_the_staging_file_it_found`, `::a_staging_file_left_by_a_crashed_writer_is_cleared_by_remove_stale_staging`; `filesystem/concurrency_tests.rs::a_staging_file_left_by_a_crashed_writer_does_not_block_the_next_publication` | None: a second writer is refused, not interleaved |
| G4 (Kubernetes) | Before the first write, re-read every object the plan read but will not write; refuse `Unwritable` if its `resourceVersion` moved (`fabric-publication-kubernetes` `confirm.rs`). Narrows the write-skew window; cannot close it | `concurrency_tests.rs::a_writer_whose_read_only_object_moved_before_it_writes_is_refused`; `publish_tests.rs::a_held_object_is_replaced_at_the_version_it_was_read_and_an_unchanged_one_is_left` (asserts the re-reads) | `concurrency_tests.rs::current_behaviour_writers_overlapping_after_their_checks_can_leave_a_dangling_binding_in_the_cluster` |

G2's test now stands in an unprojected tenants volume by restoring the
previous tenants bytes, since one refresh pass no longer applies data
sources alone. G5 and G6 tests change only to the single refresh handle.

Still deferred:

- **D01-1:** G1b (a cross-document generation), G2 (retirement waits for
  readers), G3's "interruption invisible to readers", G4's `Lease` (RBAC
  text in ADR 0018 and ADR 0023) or support for multiple writers, G5's
  rollback, failure and hold semantics, G6's versioning rule, G7's reader
  revision reporting, G8's refusal or generation.
- **D01-2:** G7's §5 gate (D01-18), and G5's hold against §5.
- **Engineering, not in this change:** G3's "incomplete" on the platform
  panel, G5's recovery command and `refresh_failed` counts, G6's per-row
  matrix tests and CI check, G8's unbound pairs on the Data tab. Each needs
  console or CI work beyond the publication path, or waits on a D01-1
  decision as G5 says.

## Reader generations

D01-2 (#108, comment 6109518415) directs reconciling #112's remaining
reader-visible and atomicity gaps in bounded PRs. This change does that with a
publication record, `publication.json`:

- **Publisher.** Every publication ends by writing a record naming each
  document's revision and the SHA-256 of its bytes, and a monotonic
  `generation`. The filesystem adapter writes it beside `tenants.json`, after
  the three documents. The Kubernetes adapter writes it as a third key of the
  tenants `ConfigMap`, so it is projected in the same volume swap as the
  tenants payload; a publication that changes only data sources or the
  catalogue rewrites the tenants object with its payload and manifest as held.
  No new `ConfigMap`, so no RBAC change.
- **Reader.** `fabric-api` now builds the runtime with
  `build_published_runtime` over `PublishedFiles`. A pass writes the two
  documents to their registries only when both match the record (data
  sources first, then tenants); otherwise both registries keep the last
  complete generation. With no record (a publisher older than this change)
  each document is read on its own, as before.
- **One resolution snapshot.** The registries are only the staging area.
  After both writes, the refresher publishes what they hold as one
  `ResolutionView` (tenant snapshot and DataSource snapshot together) into
  `PublishedGeneration`, a single `ArcSwap`. The published resolver reads
  only that view, and reads it once per call. The tenant binding, the
  DataSource, both exclusivity facts and the `ExecutionTarget` are all taken
  from that one view. A request therefore resolves entirely against one
  generation, before or after the swap, including while the registries hold
  new data sources and old tenants between the two writes.
- **Floors.** The record carries `minimum_reader_contract` (a reader below it
  refuses the generation: the explicit version handshake G6 lacked) and
  `minimum_publisher_contract` (a publisher below it refuses with
  `BelowRollbackFloor` before writing: D01-21's floor for the published
  state).
- **Publisher guards owed by #135.** O1: a held tenant binding or DataSource
  offered at an older revision, or changed at the same one, is refused as
  `ResourceRevisionNotAdvanced`, because a reader would keep its held copy and
  part-apply the generation. O2: a lost manifest is held at the revision the
  record names, so an offer at revision 1 is refused as `StaleRevision` and
  `current()` reports the recorded revision.

| Gap | Fixed-behaviour tests | Residual `current_behaviour_*` test |
|---|---|---|
| G1b | `publication_atomicity.rs::a_reader_holds_its_generation_while_tenants_reach_it_before_the_data_sources_they_name`, `::a_reader_holds_its_generation_while_data_sources_reach_it_before_the_tenants`; `published_files_tests.rs` (8 tests); `generation_isolation_tests.rs::a_tenant_resolves_to_one_complete_generations_target_at_every_point_of_a_refresh` (deterministic: two dedicated tenants swap DataSources while the DataSources swap databases, resolved before, between and after the two registry writes, comparing the whole `ExecutionTarget`), `::a_sampler_never_sees_a_mixed_target_while_the_published_generation_advances` | `generation_isolation_tests.rs::current_behaviour_resolving_across_the_registries_between_the_two_writes_reaches_the_other_tenants_database` (what `build_runtime`'s resolver over two independent sources still does; production does not use it) |
| G2 | `publication_atomicity.rs::a_reader_never_applies_a_retirement_before_the_unbinding_that_preceded_it` | none at the reader; the publisher still does not wait for readers |
| G3 | `publication_atomicity.rs::an_interrupted_publication_leaves_readers_on_the_previous_complete_generation` (five failure points, with a sampler); `concurrency_tests.rs::an_interrupted_publication_leaves_the_previous_generations_record_in_place` | `publication_atomicity.rs::current_behaviour_an_interrupted_publication_leaves_the_published_set_at_mixed_revisions` (the files, not readers) |
| G4 (Kubernetes) | readers refuse the dangling pair | `concurrency_tests.rs::current_behaviour_writers_overlapping_after_their_checks_leave_objects_no_record_names_as_one_generation` (objects still dangle; a `Lease` is still owed) |
| G6 | `reader_compatibility.rs::a_reader_at_contract_n_serves_a_publication_at_n_plus_one`, `::a_reader_at_contract_n_plus_one_serves_a_publication_at_n`, `::a_publication_above_the_reader_floor_keeps_running_readers_on_their_generation_and_stops_new_ones` | `reader_compatibility.rs::current_behaviour_a_shape_change_published_without_raising_the_floor_is_refused_only_by_unknown_fields` |
| D01-21 (published state) | `reader_compatibility.rs::a_publisher_below_the_held_records_rollback_floor_refuses_and_writes_nothing`; `plan_tests.rs::a_held_record_above_this_publishers_rollback_floor_refuses_the_whole_publication` | `reader_compatibility.rs::current_behaviour_a_publisher_rolled_back_below_the_record_leaves_readers_on_the_last_recorded_generation` |

Still open after this change:

- **What the snapshot does not cover.** It holds within one process, for
  tenants and data sources. Different replicas can still serve different
  complete generations until each has refreshed; nothing reports which one
  each serves (G7). The catalogue is outside the generation (below).
  `build_runtime`, the per-document builder over two independent sources,
  still resolves across its registries as they stand, so it can pair one
  source's new data with the other's old data. Only in-memory and test
  compositions use it. Request-atomic application is claimed only for
  `build_published_runtime`, and has not been verified live.
- **Catalogue.** Still read once at start. Checked against the record since
  the change below (G8).
- **Release floor (D01-21).** A publisher rolled back below this change does
  not update the record, so readers hold the last recorded generation until an
  operator removes it. The minimum rollable release belongs in the release
  compatibility manifest (#124). Since recorded and enforced in-product; see
  [Release rollback floor](#release-rollback-floor-d01-21).
- **Readiness.** A process starting while a publication is incomplete fails
  its prime (`fail_fast_on_prime`) until the volumes converge.
- **Platform mount.** Readers find the record only where the tenants
  `ConfigMap` is mounted as a whole volume (not by `items` or `subPath`). Not
  verified against the platform repository.
- Still deferred: G4 `Lease`, G5, G7, G8, O3, O4. The next section takes up
  O3, O4, O5 and part of G8.

## Revision history, placements and the catalogue

D01-1 requires tenant and resource revisions to stay monotonic, not only
document revisions. D01-27 allows no blind placement revert or removal, and
asks for break-glass Git effects to be characterized in tests. This change
adds the following. None of it is verified live.

- **Resource history in the record (O3).** `publication.json` gains a
  `resources` key: for every tenant binding and DataSource any recorded
  generation published, the highest revision it was published at. Entries
  are never removed. The plan builds it from the held record and the held
  documents, so a record written before it existed starts from what is held.
  An offered resource that the held document does not hold must come back
  above that revision. Otherwise the publication is refused as
  `ResourceRevisionNotAdvanced`, naming the revision it was last published
  at. This covers a DataSource retired and declared again, and a tenant
  emptied out and placed again.
- **Declarations above what was published (O3).** `DataSources::declare`
  reads the target's `published_resources()`, a new port method that every
  adapter implements from its held state. It writes a declaration one above
  the higher of the held and the published revision. This covers a new
  declaration of a retired id, a correction made after a revert lowered
  `data-sources.yaml`, and an unchanged entry held at or below a retired
  revision. The control plane wires the target in at registration. Without
  a target, revisions start at 1 as before. If the target cannot be read,
  the declaration is refused as `PublishedStateUnreadable` (503).
- **Placements held to the last published tenants (O4).** Before composing,
  `compose` checks the placement records against the held tenants document.
  - A published tenant or logical binding with no record left is refused as
    `PlacementRemoved`.
  - The records for the logical bindings a tenant was published with must
    sum to at least its published revision, and to more if any of those
    bindings changed. Otherwise the pass is refused as `PlacementLowered`.
  - Summing only those records catches a revert that lowers one record while
    adding others. The publisher sees only the tenant's total, which still
    rises in that case.
  - The explicit way past both refusals is a revision-bumping write.
- **No silent tenant removal at the publisher (O4).** A held tenant that is
  missing from the offered tenants document is refused as `TenantRemoved`.
  The one exception is emptying the whole document with
  `Emptying::Intended`.
- **Catalogue against bindings and the record (G8).**
  - The publisher refuses a catalogue entry on a logical data source that no
    offered tenant binds (`CatalogueEntryUnbound`). A tenants document with
    no tenants is exempt, because it serves no request. An entry that some
    tenant binds and another does not is still published, as ADR 0018
    part 4 allows.
  - When the derived catalogue has such an entry, the controller offers the
    held catalogue at its held revision instead, so the catalogue stays at
    its last complete generation and tenant and data-source changes still
    publish.
  - `fabric-api` takes its catalogue from the same read the runtime primes
    from (`PublishedFiles::with_catalogue`, `load_at_start`,
    `PublishedRuntime::catalogue`). The catalogue's bytes must match the
    digest named by the same record as the tenants and data sources the
    resolver primed, or the prime fails. A publication that lands after the
    prime therefore cannot pair its catalogue with the previous generation's
    tenants. Review of #138 found that an earlier version of this change
    read the catalogue separately after the prime and was open to exactly
    that. Only a runtime that started unprimed (`fail_fast_on_prime = false`
    and no complete generation at start) reads the catalogue on its own,
    checked against the record it finds then. The tenants it later applies
    may come from a newer generation.

| Item | Guard | Tests (each fails with its guard removed; checked by reverting the guard locally) |
|---|---|---|
| O3, publisher | `resource_history.rs`, `resource_revisions.rs`, `plan/held.rs::resources` | `plan_tests.rs::a_data_source_retired_and_declared_again_at_or_below_its_last_revision_is_refused`, `::a_record_from_before_resource_history_starts_it_from_the_held_documents`, `::a_tenant_emptied_with_intent_and_placed_again_below_its_last_revision_is_refused`; `record_tests.rs::a_record_carries_resource_history_under_resources_and_reads_without_it` |
| O3, declaration | `fabric-platform-management` `data_sources/plan.rs`, `data_sources/service/published.rs` | `data_sources/plan_tests.rs::an_id_removed_and_declared_again_starts_above_the_revision_it_was_last_published_at`, `::a_correction_after_a_revert_lowered_the_held_revision_moves_above_the_published_one`, `::an_unchanged_declaration_held_below_the_published_revision_is_written_above_it`, `::a_reverted_entry_at_the_revision_it_was_retired_at_is_written_above_it` |
| O4, controller | `publication/placement_history.rs` | `snapshot_tests.rs::a_placement_reverted_out_of_a_published_tenant_is_refused`, `::a_published_tenant_reverted_out_entirely_is_refused`, `::a_record_reverted_to_its_earlier_binding_and_revision_is_refused`, `::a_lowered_record_is_refused_even_when_an_added_record_raises_the_tenants_total`, `::a_binding_changed_without_raising_its_records_revision_is_refused`, `::a_revision_bumping_write_may_change_a_binding_and_an_added_placement_is_accepted` |
| O4, publisher | `resource_revisions.rs::refuse_removed_tenants` | `plan_tests.rs::a_held_tenant_dropped_from_the_offer_is_refused` |
| O5 | the guards above, driven through the router, the pass and a real filesystem target | `fabric-control-plane/tests/platform_publication_revert.rs`: `reverting_placements_yaml_to_drop_a_published_placement_is_refused_and_writes_nothing`, `reverting_a_placement_record_is_refused_even_when_the_revert_adds_another`, `reverting_a_data_source_correction_is_refused_until_a_correction_moves_past_it`, `a_data_source_removed_and_declared_again_comes_back_above_its_last_published_revision`, `a_release_no_tenant_is_placed_on_waits_while_the_last_catalogue_stays_published`. Each refusal asserts all four published files unchanged |
| G8, publisher | `catalogue_bindings.rs` | `plan_tests.rs::a_catalogue_entry_on_a_logical_data_source_no_tenant_binds_is_refused` |
| G8, controller | `publication/catalogue_hold.rs` | `platform_publication_revert.rs::a_release_no_tenant_is_placed_on_waits_while_the_last_catalogue_stays_published` |
| G8, reader | `fabric-tenant-runtime` `publication/published_files.rs::load_at_start`, `registration/build_published.rs`, `publication/catalogue.rs`; `fabric-api` `startup/application.rs`, `startup/catalog.rs` | `catalogue_generation.rs::a_publication_between_priming_and_the_catalogue_read_does_not_reach_the_catalogue` (a deterministic interleaving: generation 2 renames `articles` to `private_articles` and lands between the prime and the catalogue read), `::a_catalogue_from_another_generation_than_the_documents_fails_the_prime`, `::a_catalogue_from_another_generation_is_refused_at_start`, `::a_catalogue_ahead_of_the_record_is_refused_at_start`, `::the_catalogue_the_record_names_is_read`, `::with_no_record_the_catalogue_is_read_as_it_stands` |

Limits of this change:

- **History has a start.** Retirements published before this change are not
  remembered. Losing the whole tenants `ConfigMap` loses the record and its
  history, as for O2.
- **An older publisher.** A publisher built before `resources` existed (#136)
  refuses a record that carries it as `Unreadable` (`deny_unknown_fields`).
  It cannot drop the history, but the refusal is not the explicit
  `BelowRollbackFloor`. `minimum_publisher_contract` is not raised. Recording
  the minimum rollable release is #124's.
- **Platform contract.** `saas-fabric-platform`'s `scripts/runtime_contract.py`
  requires the record's keys to be exactly the five #136 wrote, so it would
  refuse a record with `resources`. The platform contract must accept the key
  before this publisher is activated. That is a cross-repository follow-up,
  not made here.
- **O4.** If one record of a tenant is lowered while another of the same
  tenant is raised in the same write, the lowering is not detected; the raise
  is itself an explicit revision-bumping write. Removing a placement or a
  tenant has no supported path. Deprovisioning (D01-14) is not built, and the
  only removal the publisher accepts is emptying the whole tenants document
  with intent. Client retirement (#137) records intent only and leaves the
  client's placements in composition, because no withdrawal path exists
  yet. Retiring a client therefore never removes a published tenant and
  cannot stall publication on these guards. A withdrawal, when one is built,
  is a separate reviewed operation that has to meet these guards.
- **G8.**
  - An entry that some tenant does not bind still gets `UnboundDataSource`
    (500) for that tenant (part 4).
  - The catalogue is not refreshed in a running replica, which is out of
    scope here. A replica keeps the catalogue of the generation it started
    on, while its tenants and data sources refresh to newer generations. A
    catalogue change therefore reaches a replica only through a restart.
  - A replica that starts while the catalogue volume lags the record fails
    its prime until the volumes converge, as the readiness limit under
    [Reader generations](#reader-generations) already says.
  - With no catalogue held, a first catalogue with an unbound entry is
    refused, and that refusal holds back the whole pass.
- **O5, partly.** No test characterizes a Git revert of the catalogue or of
  client documents, or restoring either through the console with conditional
  writes and audit evidence. **Since then:** both are characterized in
  `fabric-control-plane/tests/catalogue_revert.rs` and
  `client_document_revert.rs`, and the Git adapter's revision on a revert
  in `fabric-client-git/tests/desired_state_repository.rs`. ADR 0018's O5
  lists what they prove and the findings open on #108.

Still deferred after this change: G4 `Lease` (single-writer enforcement),
G5 (persisted last-known-good, recovery command, failure semantics), the
release rollback floor (#124, D01-21), G7, G6's remaining items, G2's
publisher wait, G3's panel state, O5 for the catalogue and client documents,
and the platform contract follow-up above.

## Release rollback floor (D01-21)

D01-21's disposition (#108) is to record and enforce a minimum rollable
version in the compatibility manifest, with format and migration
compatibility tests. The approvals of #136, #137 and #138 each left the
release floor owed. #137's review adds that an older Fabric ignores
`spec.lifecycle`, so the floor must sit at or above the release that
carries it. This change does the following. None of it is verified live.

- **The manifest.** `compatibility.yaml` at the repository root records the
  floor, `minimumRollableVersion: 0.3.0-preview.16`, for the component that
  pulls `ghcr.io/fieldstatenz/saas-fabric`. Beside it are the publication
  contracts the floor rests on. It is the rollback-floor part of #124's
  manifest. Source commits, digests and migrations are still #124's to add,
  and no release renders or attaches the file yet.
- **Why `0.3.0-preview.16`.** `0.3.0-preview.15` (cut 2026-09-22) and every
  earlier release predate three persisted formats, and each misreads the
  newer state. They ignore the publication record (#136), so readers hold a
  stale generation. They drop or refuse resource history (#138), so a
  retired resource can return at a lower revision. They ignore
  `spec.lifecycle` (#137), so a retired client reads as usable. The next
  release cut from `main` is the first that can include all three. If it is
  cut before this change merges, it still reads all three, so the floor
  stays safe. `fabric-component`'s constants are held to the file by
  `tests/this_repositorys_compatibility.rs`. That test also checks the floor
  is above `0.3.0-preview.15` and on the workspace's version line.
- **Enforced where the newer binary acts.** A binary older than the floor
  enforces nothing, so the floor cannot be enforced inside it. Platform
  Management, built with the floor (`RollbackFloors::for_this_release`, wired
  at the composition root), applies it to whichever component pulls Fabric's
  runtime repository, whatever the environment calls that component:
  - the rollback listing leaves out every version below the floor, and
    clears `more` once the floor is reached;
  - `POST .../rollback` naming a version below the floor is refused as
    `422 below_rollback_floor` (`PlatformError::BelowRollbackFloor`), before
    any registry is asked and before anything is written.
- **The published state's side of the floor.** The publication contract is
  raised from 1 to 2, and every record states it as
  `minimum_publisher_contract`. The first pass of this publisher rewrites a
  contract-1 record as the next generation at contract 2, with no document
  rewritten. Publishers from #136 to #138 were built at contract 1. Each
  reads the floor first and refuses as `BelowRollbackFloor`, where a #136
  publisher used to refuse #138's history only as `Unreadable`. #140 raises
  the contract again, to 3, for the record's `registry` key, so #139's
  contract-2 publisher refuses a registry-bearing record by its floor too. Readers are
  not raised: `minimum_reader_contract` stays 1. Manifests keep the contract
  they were written at until the document's next revision, and no reader
  reads them.

| Item | Guard | Tests (each fails with its guard removed; checked by reverting the guard locally) |
|---|---|---|
| Manifest agrees with the binaries | `fabric-component` `compatibility.rs` | `this_repositorys_compatibility.rs::the_floor_is_the_one_the_binaries_enforce`, `::the_floor_is_above_every_release_that_misreads_the_current_formats`, `::the_floor_is_on_the_line_this_workspace_releases`, `::the_floor_applies_to_the_runtime_image_component_yaml_names`, `::the_publication_floor_is_the_publishers_contract_and_readers_are_not_raised`, `::the_floor_is_a_version_a_release_can_carry` |
| Rollback listing | `rollback_floors.rs` (`RollbackFloors::cut`, `History::at_or_above`), called from `service/rollback.rs::rollback_candidates` | `service_tests.rs::the_rollback_listing_offers_nothing_below_the_floor_and_nothing_more`; `fabric-control-plane/tests/platform_rollback_floor.rs::the_listing_stops_at_the_floor_and_says_there_is_nothing_older` |
| Rollback request | `rollback_floors.rs::RollbackFloors::admit`, called first in `service/rollback.rs::roll_back` | `service_tests.rs::a_rollback_below_the_floor_is_refused_before_any_registry_is_asked_and_writes_nothing`, `::a_rollback_to_the_floor_itself_is_written`, `::a_floor_on_another_repository_leaves_this_component_alone`; `platform_rollback_floor.rs::a_rollback_below_the_floor_is_refused_as_its_own_code`, `::a_rollback_to_the_floor_itself_passes_the_floor`; `rollback_floors.rs` unit tests |
| Composition | `fabric-control-plane-api` `startup/platform.rs::establish` | `startup/platform.rs::the_shipping_binding_refuses_rollbacks_below_fabrics_own_floor`; `rollback_floors.rs::this_release_floors_the_component_that_pulls_saas_fabrics_runtime_image` |
| Published-state floor and migration | `manifest.rs::CONTRACT_VERSION` (3 since #140), `rollback_floor.rs::check_publisher_floor` | `format_compatibility_tests.rs`: `a_record_in_the_136_format_is_read_and_rewritten_at_this_contract_without_touching_a_document`, `a_record_in_the_138_format_keeps_its_resource_history_across_the_floor_raise`, `a_record_this_release_writes_is_refused_by_a_contract_one_publisher_before_its_history_is_read`, `a_record_this_release_writes_admits_this_release`, `a_record_from_a_later_floor_is_refused_by_this_release_whatever_else_it_carries`, `a_contract_one_manifest_is_still_read_as_the_documents_revision`, `this_contract_is_above_the_one_the_136_to_138_publishers_were_built_at`; existing `plan_tests.rs::a_held_record_above_this_publishers_rollback_floor_refuses_the_whole_publication` and `reader_compatibility.rs::a_reader_at_contract_n_serves_a_publication_at_n_plus_one` (contract-1 readers serve contract-2 records) |

Limits of this change:

- **Break-glass is not covered.** Editing `components.yaml` in the platform
  repository to pin a version below the floor bypasses Platform Management,
  and the older binary then runs as before. For the publisher, that is
  `reader_compatibility.rs::current_behaviour_a_publisher_rolled_back_below_the_record_leaves_readers_on_the_last_recorded_generation`.
  Below the floor there is no rollback, only a reviewed, backed-up and
  rehearsed recovery (ADR 0026).
- **Client documents carry no floor of their own.** A control plane from
  before #137 ignores `spec.lifecycle`, and nothing written into a client
  document can stop it. The in-product refusal above is the only guard.
- **The platform contract.** `saas-fabric-platform`'s
  `contracts/runtime-documents.yaml` pins `publicationContractVersion`, and
  its checker holds the record's publisher floor to it. The platform
  contract must move to 2 before this publisher is activated. That is a
  cross-repository follow-up, not made here.
- **Release automation.** The release workflow does not yet read
  `compatibility.yaml`. A tag below the floor is not refused there, and the
  manifest is not attached to a release (#124).
- **The floor is raised by a pass.** Until a pass of this publisher runs,
  the held record still says contract 1.

Still deferred after this change: G5 (persisted last-known-good, a recovery
command, failure semantics), G4 `Lease`, G7, G6's remaining items, G2's
publisher wait, G3's panel state, O5 for the catalogue and client documents,
and both platform contract follow-ups.

## Persisted last-known-good (G5)

G5's reader side, under D01-1's disposition (#108): runtime documents roll
forward only, and failure behaviour is explicit. #139 left G5 out because
it needs a writable reader volume, which is a platform mount change, and
because its failure semantics fall under D01-1. This change builds the
Fabric side and leaves the mount as a follow-up. None of it is verified
live, and it is **off by default**.

- **The copy.** With `[tenant_runtime] last_known_good_dir` set, the reader
  (`PublishedFiles::with_last_known_good`) persists every complete
  generation it reads, with its catalogue, to `last-known-good.json` in that
  directory: the record and the exact tenants, data-sources and catalogue
  bytes it names, plus the record's own SHA-256. One file, so one rename
  replaces it whole. It is written by staging a uniquely named sibling with
  `create_new`, writing, `fsync`, rename, then `fsync` of the directory. A
  write stopped at any step leaves the previous copy whole; a staging file
  left by a crash is cleared at the next start. A generation whose
  catalogue lags the record is applied as before and persisted on a later
  pass, once the catalogue matches. `fabric-api` refuses a directory that
  is empty or holds a published file: the reader never writes where the
  publisher does.
- **Validation, before anything is chosen, raised or persisted.** A
  published generation and the copy are both checked whole
  (`ParsedGeneration::validate`), not only by digest: the record and every
  digest match (plus the record's own digest, for the copy), the reader
  floor admits this reader, both documents decode through the real
  decoders, every resource passes its own validation, no key appears twice,
  every binding names a DataSource the generation carries, and the catalogue
  passes the host's own decoder (`fabric-api`'s `catalogue::HostCatalogue`,
  the one startup uses, handed to the runtime as a `CatalogueCheck` so the
  runtime plane never links `fabric-data-api`). A published generation that
  fails is refused as `SourceError::Unusable`: the copy's bytes and the
  floor stay as they were, or, with no copy, the prime fails. Review of
  98e9ab1 found the first version chose and persisted on digests alone, so
  a higher publication with matching-hash malformed tenants overwrote a
  valid copy and then failed the prime.
- **A refused copy is kept.** It is reported and left in place, byte for
  byte, even when a valid publication is served instead; the reader never
  repairs, replaces or deletes it. Only an audited discard removes it, after
  which the next validated generation is persisted.
- **Physical separation.** `check_separation` resolves every path through
  the filesystem component by component (symlinks, then `..` as the kernel
  takes it), including directories that do not exist yet through their
  existing ancestors, and refuses a copy directory that is, contains, or
  sits inside the directory of any published file (tenants, data sources,
  catalogue, record), whether named directly or through a symlinked file.
  `fabric-api` runs it at configuration, and the reader runs it again
  before every write. Review of 98e9ab1 found the first version compared
  paths lexically, so a symlink alias of the published directory and a
  published file named `last-known-good.json` let the copy overwrite it.
- **Hold and fail-closed rules** (`last_known_good/choice.rs`):

  | When | Published files | Valid copy | Applied |
  |---|---|---|---|
  | Start | complete generation N | none, or M ≤ N | N, persisted |
  | Start | complete generation N | M > N | M: a generation never goes back |
  | Start | torn, mismatched, unreadable, above this reader, or no record | M | M, held |
  | Start | torn, mismatched, unreadable or above this reader | none | nothing: the prime fails (fail closed) |
  | Start | no record | none | each document on its own, as before |
  | Running | complete generation ≥ the highest served or persisted | — | applied at once, persisted |
  | Running | complete generation below it, or no record after a recorded one | — | held: `SourceError::Regressed` |
  | Running | torn, mismatched, unreadable, above this reader | — | held, as before |

  Roll forward only: the moment a published generation at or above the
  copy validates, it wins.
- **Readiness and observability.** A replica serving its copy is **ready**
  (it serves a complete generation it once validated, and pulling it would
  turn a torn volume into an outage) and **degraded**. The detailed `/ready`
  body gains `publication`: the generation served, its origin
  (`published` or `last_known_good` with `held_because`), `held_passes`,
  `last_hold`, and the copy's state (`disabled`, `absent`, `stored`,
  `refused`, `persist_failed`). The minimal body is unchanged. There is no
  metrics exporter in this workspace, so the metric source is the log
  events with stable IDs: `runtime.last_known_good_persisted` (2006),
  `_superseded` (2007), `_served` (2208, error level), `_refused` (2209),
  `_persist_failed` (2210), beside the existing `runtime.generation_*`
  events.
- **Recovery command.** `fabric-api last-known-good inspect | validate |
  discard --dir <dir>` runs before configuration and telemetry, with no
  network. `inspect` and `validate` write nothing; `validate` exits 1 unless
  a reader would serve the copy. `discard` requires `--operator` and
  `--reason`, appends a JSON line to `audit.jsonl` (who, why, when, the file
  digest and the stated generation) and syncs it before moving the copy
  aside to `last-known-good.json.discarded-<time>-<digest>`. Nothing is
  written in its place. No action writes, restores or promotes a copy, so
  the command cannot fabricate a generation.

| Item | Guard | Tests |
|---|---|---|
| A generation that matches its digests but cannot be served never displaces, raises or replaces anything | `parsed_generation.rs::ParsedGeneration::validate`, called from `holder.rs` before `choice.rs`; `store.rs::verify` for the copy | `holder_tests.rs::at_start_an_unservable_higher_generation_holds_the_copy_and_leaves_its_bytes`, `::at_start_an_unservable_generation_with_no_copy_fails_closed_and_writes_nothing`, `::on_refresh_an_unservable_generation_is_held_without_raising_the_floor_or_touching_the_copy`, `::after_a_held_refresh_a_restart_still_starts_from_the_copy_and_a_later_valid_generation_wins` (each over malformed tenants, a binding to an absent DataSource, a tenant with no binding, malformed data sources and an undecodable catalogue, all with matching digests); `last_known_good_restart.rs::a_higher_generation_whose_tenants_match_their_digest_but_do_not_decode_never_displaces_the_copy`, `::a_running_replica_holds_against_a_higher_generation_whose_catalogue_the_host_cannot_decode`; `store_tests.rs::a_copy_that_matches_its_digests_but_cannot_be_served_is_refused_and_reported_invalid`; `last_known_good_command.rs::validate_fails_on_a_self_consistent_copy_whose_catalogue_the_host_cannot_decode` |
| A refused copy is kept until discarded | `holder/accept.rs` | `holder_tests.rs::a_refused_copy_is_kept_byte_for_byte_beside_a_valid_publication_until_it_is_discarded` |
| Physical separation | `separation.rs::check_separation`, from `fabric-api` `config/validation/last_known_good.rs` and `holder/accept.rs` | `separation_tests.rs` (symlink aliases, `..` taken after a symlink, `..` through a missing directory, prospective directories, a published file named like the copy, a published file symlinked into the copy's directory, a Kubernetes-style `..data` mount); `holder_tests.rs::a_copy_directory_aliased_onto_the_published_directory_is_never_written_into`, `::a_published_file_named_like_the_copy_under_an_aliased_directory_is_left_untouched`; `validation_tests.rs::a_last_known_good_directory_aliasing_the_published_one_is_refused_however_it_is_named` |
| Restart on a torn volume serves the copy | `last_known_good/holder.rs::at_start`, `choice.rs::at_start` | `fabric-runtime-publication/tests/last_known_good_restart.rs::a_replica_restarting_on_a_torn_volume_serves_its_last_known_good_generation` (real publisher and reader; asserts the copy's catalogue is the one served); `choice_tests.rs::at_start_a_torn_volume_is_held_against_with_a_valid_copy`, `::at_start_a_lost_record_is_held_against_with_a_copy_and_read_per_document_without_one` |
| No copy and a torn volume fails closed | `choice.rs::at_start` | `last_known_good_restart.rs::a_restart_on_a_torn_volume_with_no_last_known_good_fails_closed` (and unprimed, 503, with `fail_fast_on_prime = false`); `choice_tests.rs::at_start_a_torn_volume_with_no_copy_fails_closed`; `fabric-api/tests/last_known_good_command.rs::discard_audits_moves_the_copy_aside_and_a_restart_on_a_torn_volume_then_fails_closed` |
| A corrupted copy is refused | `last_known_good/store.rs::verify` | `last_known_good_restart.rs::a_corrupted_last_known_good_is_refused_left_in_place_and_the_restart_fails_closed`; `store_tests.rs::a_copy_whose_document_no_longer_matches_its_digest_is_refused`, `::a_copy_whose_record_was_altered_is_refused_by_the_records_own_digest`, `::a_copy_whose_record_requires_a_newer_reader_is_refused`, `::a_copy_in_a_format_this_reader_does_not_know_is_refused`, `::a_truncated_copy_is_refused_as_malformed` |
| A stale copy loses to a newer published generation | `choice.rs::at_start`, `::on_refresh` | `last_known_good_restart.rs::a_stale_last_known_good_loses_to_a_newer_published_generation_and_is_replaced`, `::a_replica_holding_last_known_good_rolls_forward_the_moment_a_newer_generation_validates`; `choice_tests.rs::at_start_a_newer_published_generation_beats_the_copy`, `::at_start_a_published_generation_below_the_copy_does_not_roll_the_replica_back` |
| Roll forward only while running | `choice.rs::on_refresh` | `last_known_good_restart.rs::a_running_replica_holds_against_a_complete_generation_below_the_one_it_holds`; `choice_tests.rs::on_refresh_a_lower_generation_or_a_lost_record_is_held_against` |
| Atomic write under interruption | `last_known_good/atomic_replace.rs` | `atomic_replace_tests.rs::a_write_stopped_before_the_rename_leaves_the_previous_copy_whole_and_cleans_up` (stopped after the write and after the `fsync`), `::a_write_stopped_after_the_rename_has_published_the_whole_new_copy`, `::a_staging_file_left_by_a_crash_never_reaches_the_copy_and_is_cleared_at_start`; `store_tests.rs::an_interrupted_persist_leaves_the_previous_copy_valid`, `::a_generation_read_without_its_catalogue_is_not_persisted` |
| Recovery command | `fabric-api` `recovery/`, `LastKnownGoodStore::inspect`, `::discard` | `last_known_good_command.rs` (the built binary): `inspect_and_validate_report_a_copy_the_reader_persisted_and_write_nothing`, `validate_fails_on_a_corrupted_copy_and_on_no_copy`, `discard_needs_an_operator_and_a_reason_and_touches_nothing_without_them`, `discard_audits_moves_the_copy_aside_and_a_restart_on_a_torn_volume_then_fails_closed`, `an_unknown_action_is_a_usage_error_and_no_action_can_write_a_copy`; `store_tests.rs::discarding_audits_first_then_moves_the_copy_aside_and_writes_nothing_in_its_place`, `::a_refused_copy_can_be_discarded_too`, `::discarding_nothing_changes_nothing_and_audits_nothing`, `::inspecting_reports_absent_valid_and_refused_copies_and_writes_nothing`; `recovery/arguments_tests.rs` |
| Off by default | `RuntimeConfig::last_known_good_dir` (`None`) | `config.rs::the_last_known_good_copy_is_off_unless_a_directory_is_configured`; `last_known_good_restart.rs::without_a_last_known_good_directory_the_reader_writes_nothing_and_behaves_as_before`; `publication_atomicity.rs::current_behaviour_last_known_good_does_not_survive_a_reader_restart` (unchanged: no directory) |
| Readiness | `fabric-api` `health/readiness_state.rs::is_degraded`, `readiness_body.rs` | `readiness_state_tests.rs::a_replica_serving_its_last_known_good_copy_is_degraded_even_with_every_connector_healthy`; `readiness_body_tests.rs::the_detailed_body_says_which_generation_is_served_from_where_and_why_it_is_held`, `::the_minimal_body_says_nothing_about_the_publication`; `readiness_probe.rs::an_administrator_sees_what_diagnosis_needs` |
| The copy is apart from the published files | `fabric-api` `config/validation/last_known_good.rs` | `validation_tests.rs::the_last_known_good_directory_may_not_be_where_the_published_files_are` |

Limits of this change:

- **Platform mount (follow-up, cross-repository).** No platform deployment
  mounts a writable reader directory, so the copy is off everywhere. The
  follow-up in `saas-fabric-platform`: mount an `emptyDir` (survives a
  container restart, which is the crash loop G5 describes) or a
  per-replica volume (also survives rescheduling) at a path outside the
  published `ConfigMap` mounts, and set
  `tenant_runtime.last_known_good_dir` to it. A pod rescheduled onto a new
  `emptyDir` starts with no copy and fails closed on a torn volume, as
  before. Not made here.
- **The publisher side of G5 is unchanged.** No pre-publication hold
  (D01-1, and D01-18 against ADR 0023 §5). A held tenants or data-sources
  document the publisher cannot parse still blocks every later
  publication as `Unreadable`, and there is still no supported recovery
  command for it; `fabric-api last-known-good` covers only the reader's
  own copy.
- **No maximum hold age.** A replica holds its copy until a published
  generation at or above it validates or an operator discards it. It
  reports degraded and logs at error level the whole time; nothing
  escalates further.
- **Running replicas keep what they serve.** The copy matters only at
  start. A replica that holds a generation in memory refuses a lower one
  only when a directory is configured; without one, behaviour is as before.
- **Replicas may hold different copies.** Each replica persists what it
  read, so after a restart two replicas can serve different generations
  until the published files converge. Nothing reports this across replicas
  (G7).
- **Stricter than a reader without a copy.** With a directory configured,
  a generation in which any one resource fails validation is held whole;
  without one, the registry still applies the valid resources and keeps
  its held copy of the others, as before.
- **A discard under a running replica.** The command is meant for a
  crash-looping or stopped replica. A running replica that already
  persisted the discarded generation does not write it again until a newer
  generation validates, and its readiness keeps reporting `stored` until
  then. (A refused copy discarded under a running replica is noticed: the
  next validated generation is persisted.)
- **Separation is checked, not locked.** The directory is resolved at
  configuration and again before every write; an alias created between
  that check and the rename is not excluded.
- **No metrics exporter.** Readiness detail and log events only; the
  platform panel does not show `held_passes` (G5's `refresh_failed` counts
  on the panel remain owed).

Still deferred after this change: G5's publisher hold and publisher-side
recovery, the platform reader mount above, G4 `Lease`, G7, G6's remaining
items, G2's publisher wait, G3's panel state, O5 for the catalogue and
client documents, and the platform contract follow-ups.

## Reader generation reporting (G7)

The disposition on [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6118073758)
chose option A: the control plane pulls a small report from each replica
through the existing read-only observer, ADR 0022's. It allows source-only
preparation now. It does not approve live use of the separate,
unauthenticated port. The control plane needs no runtime write RBAC, no
`TokenReview`, no human or platform-admin bearer and no push credential.
Nothing below is verified live, and nothing is wired in any environment.

- **The report (`fabric-core` `ReaderGenerationReport`).** It is versioned
  (`contract: 1`) and has exactly four keys: `contract`, `generation`,
  `record_sha256` and `origin` (`published` or `last_known_good`). It
  carries no tenant, issuer, data source, path, token or hold reason. It
  lives in the shared kernel because both planes must agree on it, and they
  cannot depend on each other. A reader refuses any other contract, any
  unknown key and generation 0.
- **Taken from the installed snapshot.** The view that resolution reads now
  carries the generation's stamp: its number, the SHA-256 of the exact
  record bytes, and its origin. The stamp is installed by the same atomic
  store as the tenant and DataSource snapshots
  (`PublishedGeneration::publish`). `RuntimeResolver::served_generation`
  reads that view once. So the report is always what resolution reads
  against, never a configured, mounted or about-to-be-applied generation.
  - The stamp is absent when nothing is installed, when the view holds
    documents with no record, and when a registry did not take every
    resource of the generation as offered (`ApplyReport::took_every_resource`).
    In that last case, held copies serve beside the generation, so the
    replica is not on that generation and must not report that it is.
  - `/ready` and its minimal body are unchanged.
- **The listener (`fabric-api` `reader_generation`).** It is off by default.
  It binds only when `reader_generation_listen` is set, and that setting
  must be an explicit `ip:port` apart from the main listener's port. It
  serves `GET /reader-generation` and nothing else:
  - `200` with the report, `no-store`;
  - `404` with no body when no whole recorded generation is installed.

  The main listener has no such route, and nothing in this repository sets
  the address.
- **The publisher's stamp (`RecordStamp`).** Every `PublicationReport` now
  names the record the target holds once the call returns: its generation,
  and the SHA-256 of the bytes written. An unchanged pass names the bytes
  already held. Both adapters stamp the same record. The control plane
  keeps the stamp of the last pass that published or found nothing to
  change (`PublicationState::expected_record`).
- **The sample (`fabric-deployment-kubernetes` `KubernetesReaderObserver`).**
  - It reads the configured Deployment, every replica set it controls and
    every pod those control, through the same bounded API reads as
    deployment evidence.
  - It asks each ready, non-terminating pod at the pod IP the API server
    reports and the configured port. It uses a separate HTTP client: no
    credential, no default header, no proxy, no redirect, a 2 s bound per
    probe and a 1 KiB body bound.
  - It then reads all three again. The sample is discarded as unobserved if
    the Deployment's identity or version, the set of replica sets, or any
    owned pod's identity or version moved in between.
  - A target is deployment configuration
    (`[platform_management.publication.readers]`: namespace, deployment,
    port). It is never taken from client desired state or a request.
  - A whole sample is bounded to 8 s and 64 pods.
- **The answer (`fabric-platform-management` `readers::assess`).**
  - **Applied:** every replica that could serve, from every replica set,
    reports the stamped generation and record digest, read from the
    published files. At least as many replicas are ready as desired, and
    none is terminating.
  - **Pending:** a replica is older, on its last-known-good copy, not ready
    or terminating, or fewer replicas are ready than desired.
  - **Unobserved:** a replica is ahead, at the stamped generation with
    another digest, serves nothing, gives no usable answer, or the
    membership changed. Also unobserved when no observer is configured,
    nothing is stamped yet, the controller has not observed the current
    spec, or no replica is ready to ask.
  - **No readers:** replicas are deliberately 0 and no pod remains. This is
    never applied and never withdrawal evidence.
  - Unobserved outranks pending. Every reason is kept, with the sample's
    time and each pod beside its replica set.
- **Readiness.** The documents gate stays publisher evidence, so a runtime
  at zero replicas can be raised against it with no bootstrap cycle. The
  readers row sits beside the three gates. `updateComplete` holds only when
  the documents gate is met and the readers are applied. The console shows
  both.

| Acceptance (#108 disposition) | Guard | Tests |
|---|---|---|
| Matched multi-pod control | `readers/assess.rs`; `reader/sample.rs` | `assess_tests.rs::every_ready_replica_on_the_stamped_record_is_applied`; `sample_tests.rs::every_ready_pod_of_the_current_replica_set_is_asked_and_matches`; `platform_publication.rs::the_stamped_record_is_the_one_on_the_target_and_matching_readers_complete_the_update` |
| An old replica set's reader during a rollout | `reader/membership.rs::owned_sets` (every set the Deployment controls) | `sample_tests.rs::a_ready_pod_of_an_older_replica_set_is_asked_too_and_holds_the_sample_back`; `assess_tests.rs::an_older_replica_set_still_ready_during_a_rollout_is_held_to_the_record_too`; `::pods_and_replica_sets_the_deployment_does_not_control_are_left_out` |
| Terminating, not ready and zero readers | `reader/sample.rs::unasked`; `readers/reasons.rs` | `sample_tests.rs::terminating_and_unready_pods_are_not_asked_and_a_pod_with_no_address_is_not_guessed_at`, `::zero_replicas_and_no_pods_is_no_readers`; `assess_tests.rs::a_terminating_or_unready_pod_or_too_few_ready_replicas_keep_it_pending`, `::zero_replicas_is_no_readers_and_never_applied`, `::desired_replicas_with_none_ready_is_never_a_vacuous_match` |
| Same generation, wrong digest; older, last-known-good, ahead, 404, unreachable and malformed reports | `readers/finding.rs::compare`; `reader/probe.rs` | `assess_tests.rs::an_older_generation_or_a_last_known_good_copy_is_pending`, `::an_answer_that_cannot_be_compared_makes_it_unobserved`, `::unobserved_outranks_pending_and_every_reason_is_kept`; `probe_tests.rs::nothing_served_is_its_own_answer`, `::a_pod_nothing_listens_on_is_unreachable`, `::an_answer_that_is_not_a_report_this_side_reads_is_malformed` |
| Membership or UID changes during the sample | `reader/membership.rs::Membership` | `sample_tests.rs::anything_that_moves_during_the_sample_discards_it` (Deployment version, pod version, pod UID replaced, replica set added, pod added) |
| No Kubernetes bearer and no redirect forwarding | `reader/probe.rs::HttpProbe` (its own client) | `probe_tests.rs::a_report_is_read_with_a_plain_get_that_carries_no_credential`, `::a_redirect_is_never_followed` |
| Bounded body and time | `reader/probe.rs` (`PROBE_TIMEOUT`, `READER_GENERATION_MAX_BYTES`); `reader.rs` (`SAMPLE_BUDGET`, `MAX_MEMBERS`) | `probe_tests.rs::an_answer_past_the_size_bound_is_refused`, `::a_pod_that_does_not_answer_in_time_is_timed_out`; `sample_tests.rs::more_pods_than_one_sample_asks_is_refused_without_asking_any` |
| Installed-snapshot coherence | `resolution/published_generation.rs::publish`; `resolution/served_generation.rs`; `refresher/prime_published.rs::served` | `served_generation_tests.rs::the_report_moves_in_the_same_store_as_the_view_resolution_reads`, `::the_report_follows_every_kind_of_read_through_the_production_builder` (fails with the whole-generation guard removed; checked locally); `reader_generation_report.rs` (real publisher and reader: the replica reports the publisher's stamp at start, after a refresh, unchanged, and for a respaced record) |
| The publisher's stamp is the record held | `plan/record.rs` (`RecordPlan::stamp`) | `reader_generation_report.rs::an_unchanged_publication_stamps_the_record_already_held`; Kubernetes `publish_tests.rs::the_record_stamp_names_the_record_key_written_and_matches_the_filesystem_adapter`; `published_files_tests.rs::documents_matching_the_record_are_read_as_one_generation` |
| Minimal `/ready` and default exposure unchanged | `fabric-api` `reader_generation.rs`, `config/validation/reader_generation_listen.rs`, `startup/serve.rs` | `reader_generation_listener.rs::omitted_configuration_binds_nothing`, `::the_main_listener_has_no_reader_generation_route`, `::the_listener_serves_its_one_route_and_nothing_else`, `::the_answer_names_no_tenant_data_source_path_or_hold_reason`, `::nothing_served_whole_is_a_404_with_no_body`; `validation_tests.rs::a_reader_generation_listener_must_be_an_explicit_socket_apart_from_the_main_one`; existing `readiness_body_tests.rs::the_minimal_body_says_nothing_about_the_publication` |
| Documents gate kept; reader evidence beside it; update completion needs applied readers | `readers/assess.rs::update_complete`; `readiness_row.rs` | `assess_tests.rs::an_update_is_complete_only_with_the_documents_published_and_the_readers_applied`; `platform_publication.rs::with_no_reader_observer_the_readers_are_unobserved_and_no_update_is_complete`, `::at_zero_replicas_the_documents_gate_still_stands_and_there_are_no_readers`; `ReaderRows.test.tsx` |

Owed before any live use. Each is a separately reviewed platform or
security step. None is approved here:

- **Caller and target selectors.** The control plane's namespace and pod
  selector, and the runtime's.
- **Port.** The reader-generation port: the runtime's
  `reader_generation_listen`, the container port, and the control plane's
  `[platform_management.publication.readers]`.
- **Read grants.** The observer's grants on the runtime namespace: `get` on
  the named Deployment, `list` on replica sets and pods. These are the
  ADR 0022 grants, extended to that namespace if it differs.
- **NetworkPolicies.** Admit only the control plane to that port. A
  NetworkPolicy isolates traffic. It does not prove which pod answered.

Limits:

- **Withdrawal.** No withdrawal is built, so nothing yet claims a withdrawal
  is applied. `lifecycle.withdrawal` still says no reader is observed.
- **The edge.** The edge's applied registry generation is the platform's
  check. Fabric still observes no edge (`registryApplication` stays
  unobserved).
- **Sampling.** Sampling happens on `GET /api/platform` and after
  `POST /api/platform/publication`. There is no schedule and no history.
- **The stamp is per process.** It is the last accepted pass in this
  control-plane process, so after a control-plane restart readers are
  unobserved until a pass runs.

Still deferred after this change: G7's live wiring (above), G5's publisher
hold and publisher-side recovery, the platform reader mount, G4 `Lease`,
G6's remaining items, G2's publisher wait, G3's panel state, withdrawal,
and the platform contract follow-ups. O5 is characterized since #159.

## Publication completeness (G3)

The disposition on [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119949193)
takes the six defaults asked in
[#108 comment 6119681433](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6119681433),
with one correction: completeness is inspected at the adapter, not inferred
from `current()`. That is because `current()` falls back to the record's
revision when a manifest is lost and never checks a digest. This change is
source only. Nothing is wired differently in any environment, and none of it
is verified live.

- **The retry is unchanged.** It is #132's: `publish_with_retry` re-offers
  an `Unwritable` snapshot at once, up to twice, with no delay, and retries
  no other refusal. Atomic writes, CAS, divergence handling and monotonic
  revisions are untouched.
- **The inspection (`RuntimePublication::inspect`).** Each adapter reads,
  without a lock and without writing:
  - the held record;
  - every payload and manifest;
  - the issuer registry, when the record names one;
  - the record again.

  The answer comes from one shared rule, `TargetObservation::assess`:
  - **Complete:** every part is at the record's revision and SHA-256.
  - **Incomplete:** a part is missing, its manifest is missing or
    unreadable, or its revision or digest differs from the record. The
    answer names the part and the gap, never its bytes.
  - **Unpublished:** no record is held.
  - **Unobserved:** a part could not be read, the record moved between the
    two reads, or the record does not parse.

  The filesystem adapter brackets with two reads of `publication.json`. The
  Kubernetes adapter reads the tenants `ConfigMap` before and after the
  other two. It treats a moved `resourceVersion` as unstable, and an
  object with a missing or blank one as unobserved.
- **When it is inspected.** Every publication pass ends with one
  inspection, whatever the pass decided. That includes an `Unwritable`
  that outlived its re-offers, and a pass that offered nothing.
  `PublicationState` keeps the last inspection beside, not inside, the
  last pass's outcome. A failed pass that wrote nothing can therefore
  still observe `complete`.
- **No target read on `GET`.** `GET /api/platform` renders the recorded
  inspection and never reads the target.
- **Restarts and clearing.** A restarted control plane holds no
  inspection, so it reports `unobserved` until a pass inspects. Neither a
  restart nor elapsed time clears `incomplete`, and only a verified
  `complete` does.
- **Readiness.** The documents gate is met only when the accepted
  evidence names all three documents **and** the last inspection is
  `complete` at exactly the accepted record, generation and digest.
  Every other state makes it unmet, with a sanitised reason, and makes
  `updateComplete` false.
- **Another coherent generation is held, not adopted.** A target observed
  complete at another record still reads `complete` on the completeness
  row. The documents gate stays unmet, with a reason naming both records,
  until a pass accepts matching evidence. That holds back the accepted
  pass's identity, connector and G7 reader evidence from certifying a
  generation they do not describe (review 5488114355, P2). The evidence
  and the observation are read under both locks together, so they come
  from one moment.
- **The panel.** The publication row gains `completeness`: `state`,
  `observedAtUnixSeconds`, the record inspected against, and `reason`. The
  console shows it as its own **Completeness** row, under **Last pass**.
- **Completeness is not reader evidence.** Readers keep their last complete
  generation (#136). Completeness says nothing about what they applied.

| Acceptance (#108 disposition) | Guard | Tests |
|---|---|---|
| Partial writes after exhausted retries make the documents gate unmet | `publisher.rs::publish_once` (`inspect_target`); `documents_gate.rs` | `publisher_tests.rs::an_interruption_that_outlives_its_reoffers_leaves_the_target_incomplete_and_the_documents_gate_unmet`; `platform_publication.rs::a_pass_that_leaves_the_target_part_written_shows_it_incomplete_and_holds_the_documents_gate`; `publication_completeness.rs::a_publication_interrupted_part_way_is_incomplete_until_it_is_completed` |
| A refusal that wrote nothing leaves a complete target `complete` while the last pass reads `failed` | completeness kept apart from `LastPass` (`state.rs`) | `publisher_tests.rs::a_refusal_that_wrote_nothing_leaves_a_complete_target_complete_while_the_pass_reads_failed`; `platform_publication.rs::a_pass_refused_before_it_wrote_anything_leaves_a_complete_target_complete` |
| A missing payload or manifest despite fallback revisions | `completeness/observation.rs` | `publication_completeness.rs::a_lost_payload_is_incomplete_though_its_manifest_still_reports_the_revision`, `::a_lost_manifest_is_incomplete_though_current_falls_back_to_the_records_revision`; `completeness_tests.rs::a_manifest_for_another_document_or_that_does_not_parse_is_unreadable` |
| Equal revisions with a wrong digest | `completeness/observation.rs::payload_gap` | `publication_completeness.rs::equal_revisions_with_other_bytes_are_incomplete`; Kubernetes `inspect_tests.rs::an_object_left_by_an_interrupted_publication_is_incomplete` |
| Issuer registry mismatch | the registry is checked when the record names one | `publication_completeness.rs::an_issuer_registry_the_record_names_is_held_to_its_digest` |
| Unavailable or unstable inspection is `unobserved` | `filesystem/inspect.rs`; Kubernetes `inspect.rs` | `publication_completeness.rs::a_part_that_cannot_be_read_leaves_the_target_unobserved`, `::a_record_this_publisher_cannot_read_leaves_the_target_unobserved`; `completeness_tests.rs::a_record_that_moved_while_the_target_was_read_is_unstable_whatever_the_parts_say`; Kubernetes `inspect_tests.rs::a_tenants_object_that_moved_during_the_inspection_is_unstable`, `::an_object_the_cluster_cannot_supply_leaves_it_unobserved`, `::a_tenants_object_with_no_resource_version_to_bracket_by_is_unobserved`, `::no_objects_are_unpublished` |
| A restart followed by an observation | `PublicationState` starts with no inspection | `publisher_tests.rs::a_restart_knows_nothing_until_a_pass_inspects_and_does_not_clear_an_incomplete_target`; `platform_publication.rs::a_restarted_control_plane_shows_completeness_unobserved_until_a_pass_inspects_the_target` |
| Retry recovery | #132's `publish_with_retry`, unchanged | `publisher_tests.rs::an_interruption_the_immediate_reoffer_completes_leaves_the_target_complete`; existing `protocol_tests.rs` G3 tests |
| Only verified coherence clears the condition | `state.rs::record` replaces the observation only with another inspection | `publisher_tests.rs::only_an_inspection_that_finds_the_target_whole_clears_an_incomplete_target`; `readiness_tests.rs::the_documents_gate_needs_the_target_observed_whole_and_says_why_it_is_not`, `::a_complete_target_does_not_stand_in_for_an_accepted_pass` |
| A target complete at another record is held: a newer generation, or the same generation with another digest; the same accepted record after a zero-write failure is still met; old accepted or G7-reader evidence cannot certify a new target | `documents_gate.rs::record_mismatch`; `state.rs::readiness` reads the evidence and the observation under both locks | `readiness_tests.rs::a_target_complete_at_another_record_holds_the_documents_gate_and_says_which`; `assess_tests.rs::readers_applied_to_the_accepted_record_cannot_certify_a_target_complete_at_another`; `publisher_tests.rs::a_refusal_that_wrote_nothing_leaves_a_complete_target_complete_while_the_pass_reads_failed` |
| `GET` performs no extra target reads | `body/publication/completeness_row.rs` renders `PublicationState` | `platform_publication.rs::a_pass_that_leaves_the_target_part_written_shows_it_incomplete_and_holds_the_documents_gate` (counts every port call across the `GET`), `::a_restarted_control_plane_shows_completeness_unobserved_until_a_pass_inspects_the_target` |
| The readers' last complete generation stays installed | #136's record-gated reader, unchanged | `publication_atomicity.rs::an_incomplete_target_is_reported_while_readers_stay_on_the_last_complete_generation` |
| No payload or sensitive detail in responses | gaps are typed; read errors are dropped | `completeness_tests.rs::what_an_operator_reads_names_the_part_and_never_its_bytes`; the first `platform_publication.rs` test above checks the body names neither the changed label nor the staging path |
| The console shows the state apart from the last pass | `CompletenessCell.tsx` | `PublicationPanel.test.tsx` "completeness" (three cases) |

Limits:

- **The target is still left mixed.** A publication that is interrupted
  and then outlives its re-offers still leaves the files or objects mixed
  until a later pass completes them. This change reports that state. It
  does not prevent it. Preventing it needs one transaction across three
  documents, as before.
- **A completing pass reads `unchanged`.** When a pass finds the documents
  already written by an interrupted one, it writes only the record. The
  last-pass outcome counts documents, not the record, so that pass reads
  `unchanged`. Only the completeness row shows the generation completing.
- **The inspection is per process and per pass.** No schedule beyond the
  existing publication schedule inspects the target, and nothing is kept
  across a restart.

Still deferred after this change: G7's live wiring, G5's publisher hold and
publisher-side recovery, the platform reader mount, G4 `Lease`, G6's
remaining items, G2's publisher wait, withdrawal, the O5 continuity and
issuance guards, and the platform contract follow-ups.

## Publisher-side recovery (G5)

G5's publisher side, without the pre-publication hold. A held tenants or
data-sources document that the publisher cannot parse blocks every
publication, at any revision:
- `plan/parse.rs` refuses a payload that does not parse with `Unreadable`,
  and a manifest held without its payload with `HeldPayloadLost`;
- each adapter's held read refuses an unreadable manifest the same way.

Until now, the only way out was to edit the files or objects by hand. This
change adds a supported command. It is source only and untested against a
live cluster. Its write semantics are the defaults asked in
[#108 comment 6121748619](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6121748619)
and are **not yet confirmed**.

- **The command.** It runs before configuration and telemetry, mirroring
  `fabric-api last-known-good`:

  ```text
  fabric-control-plane-api held-publication inspect|validate <target>
  fabric-control-plane-api held-publication restore <target> --document tenants|data-sources --from <file> --audit <file> --operator <who> --reason <why>
  ```

  `<target>` is either `--tenants`, `--data-sources` and `--catalog` (the
  filesystem adapter's paths), or `--namespace` (the Kubernetes adapter, in
  the pod).
  - `inspect` and `validate` write nothing. They report every reason the
    held state blocks publication (`diagnose`), not only the first one the
    plan would hit.
  - `validate` exits 1 while anything blocks.
  - Exit 2 is a usage error, a target that cannot be read, or a failed
    audit or write.
- **Restore, and only restore.** `plan_restore` puts back exactly the bytes
  whose SHA-256 the held publication record names for that document. The
  revision is the one the record names, and that is the revision the held
  state already reports, so **no revision moves**. Every guard of the next
  publication then reads the content the record attests. A restore is
  refused, with nothing written and nothing audited, when:
  - the document is not blocking publication, or it is the catalogue,
    which is only byte-compared;
  - no record is held, or the record does not parse or is above this
    publisher's rollback floor;
  - the bytes are not the ones the record names;
  - a readable manifest states a revision other than the record's, as an
    interrupted publication would leave it.

  An unreadable or missing manifest is rewritten at the record's revision.
  The bytes can come from any copy whose digest matches. A reader's
  `last-known-good.json` holds them, under its `tenants` and
  `data_sources` keys.
- **Exclusion and atomicity are unchanged.** The filesystem restore holds
  the G4 publication lock from its read to its last write, and clears a
  crashed writer's staging file first. It writes through `atomic_write`:
  payload, then manifest, as `write_if_needed` does.

  The Kubernetes restore replaces one object at the `resourceVersion` it
  read. That object keeps every other key, including the record and the
  registry on the tenants object. A data-sources restore re-reads the
  tenants object, which holds the record, before it writes, as
  `confirm.rs` does. It uses `get` and `update` only, which the publisher
  already holds.
- **Audit first.** One JSON line is appended to `--audit` and synced
  before the first write. It records who, why, when, the target, the
  source file, and the plan: the revision, generation, digests and what
  is written. A write that fails after it appends a `restore_failed`
  line. An audit log in a published document's directory is refused.
- **No discard.** Nothing removes, quarantines or replaces a held document
  the record cannot vouch for. Such a document still needs an operator, as
  before. The reason: the next publication would plan against an empty
  document, and `TenantRemoved`, the emptying guard and
  `RetiredDataSourceStillBound` could not see what was discarded. Whether
  to support a discard, and with which guard, is open on #108.

| Item | Guard | Tests |
|---|---|---|
| Only the bytes the record names, at its revision; nothing else is written | `held_recovery/restore_plan.rs::plan_restore` | `restore_plan_tests.rs` (13 tests: torn payload, lost payload, unreadable and mis-kinded manifests, digest mismatch, not blocking, catalogue, no record, unreadable record, record above the floor, manifest ahead of and behind the record, recorded bytes that do not parse, a report without bytes) |
| Publication resumes, no revision moves, and every guard reads the restored content | the shared plan, unchanged | `held_recovery.rs::a_torn_tenants_document_is_restored_from_the_recorded_bytes_and_publication_resumes`, `::a_lost_tenants_payload_is_restored_under_its_manifest`, `::a_torn_data_sources_document_is_restored_the_same_way`, `::after_a_restore_every_guard_still_reads_the_restored_content` (`TenantRemoved`, `StaleRevision`), `::bytes_the_record_does_not_name_are_refused_and_nothing_is_written_or_audited`, `::an_interrupted_publication_ahead_of_its_record_is_not_restored` |
| A reader's last-known-good copy supplies the bytes | digest match against the held record | `held_recovery.rs::a_readers_last_known_good_copy_holds_the_bytes_a_restore_needs` (real reader persists, publisher restores, revision 2 publishes) |
| Filesystem: G4 lock, staging cleanup, atomic writes, audit before write | `filesystem/recovery.rs` | `recovery_tests.rs::a_restore_is_refused_while_a_publication_holds_the_lock_and_writes_nothing`, `::the_audit_is_made_before_the_first_write`, `::an_audit_that_cannot_be_made_durable_stops_the_restore_with_nothing_written`, `::a_staging_file_left_by_a_crashed_writer_does_not_block_the_restore`, `::a_failed_write_after_the_audit_is_audited_as_failed` |
| Kubernetes: replace at the read version, carried keys kept, record re-read | `fabric-publication-kubernetes` `recovery.rs`, `recovery/held_object.rs` | `recovery_tests.rs::a_torn_tenants_object_is_replaced_at_its_version_keeping_the_record_and_registry`, `::a_data_sources_restore_rereads_the_record_before_replacing_its_object`, `::a_data_sources_restore_whose_record_moved_writes_and_audits_nothing`, `::bytes_the_record_does_not_name_are_refused_with_only_reads_sent`, `::a_replace_the_cluster_refuses_as_a_conflict_is_audited_as_failed`, `::observing_reads_each_object_once_and_writes_nothing` |
| The command, through the built binary | `fabric-control-plane-api` `recovery/` | `held_publication_command.rs::inspect_and_validate_report_the_blocking_document_and_write_nothing`, `::restore_puts_back_the_recorded_bytes_after_auditing_and_publication_resumes`, `::restore_of_bytes_the_record_does_not_name_is_refused_and_writes_and_audits_nothing`, `::restore_refuses_an_audit_log_beside_the_published_files`, `::usage_errors_and_actions_that_would_discard_or_reset_exit_two_and_touch_nothing`; `recovery/arguments_tests.rs` |

Limits:

- **Recoverable only from attested bytes.** With no record, a record that
  names other bytes, or no copy of the bytes, a blocking document still
  needs a hand repair. That covers an interrupted publication whose
  manifest is ahead of its record (#108, question 2).
- **The record's window on Kubernetes is narrowed, not closed.** A
  data-sources restore re-reads the record before it writes. A writer that
  replaces the record between that read and the write is not excluded,
  as with `confirm.rs`.
- **A manifest-only write is still two writes on the filesystem.** If the
  payload write succeeds and the manifest write fails, the payload is
  restored and the manifest is not, and the audit log says so. A second
  restore completes it.
- **Not run live.** No environment has run the command, and no runbook
  step has been added for it.

Still deferred after this change: G5's publisher hold and any discard
(#108), G7's live wiring, the platform reader mount, G4 `Lease`, G6's
remaining items, G2's publisher wait, withdrawal, the O5 continuity and
issuance guards, and the platform contract follow-ups.

## Not covered here

- **Live publication.** #112 requires separate authorization. Kubelet projection
  order is stated from Kubernetes behaviour and ADR 0018. No cluster tests it.
- **The control-plane replica count on LucentRoot,** and whether platform #40/#41
  wiring matches ADR 0018's RBAC amendment.
- **Console copy** about catalogue selection and deployment.
- **A coupled change served half-applied** (G3), and whether every such mixed state
  fails closed.
- **The Data API's request-path tests** beyond `MissingDataSource` → 500.
