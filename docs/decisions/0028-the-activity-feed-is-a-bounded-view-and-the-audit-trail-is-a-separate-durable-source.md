# ADR 0028 — The activity feed is a bounded view; the audit trail is a separate durable source

- **Status:** Proposed
- **Date:** 2026-10-12
- **Applies to:** `fabric-control-plane` (the `audit_trail` module, `audit`,
  `GET /api/activity`), `fabric-control-plane-api` (`local_audit_trail`, the
  `[audit_trail]` configuration section, startup), `apps/control-plane-ui`
  (the activity table and feed)
- **Related:** [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md)
  §6 and owed decision 6; [ADR 0008](0008-desired-state-is-the-authority.md);
  [M0 contract decisions](../roadmap/m0-contract-decisions.md) D01-19c,
  disposed in [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)

## Context

ADR 0021 §6 records operator activity inside desired state: the catalogue's
`activity` and each client's `spec.product.activity`. `GET /api/activity`
merges them. Nothing trims them, so they are bounded only by the document size
limit: once a document reaches it, writes are refused with
`422 document_too_large` until someone trims the file by hand in Git. ADR 0021
already says activity "is a view, not the audit trail", but the audit events it
names went only to the log pipeline, and where they were retained was not
built.

D01-19c disposition (#108): *"Activity is a bounded view, not the audit trail.
Design a durable audit source with integrity/size tests; do not silently prune
authoritative history. Retention, storage activation and any customer-data
commitments remain separate."*

## Decision

### 1. Two things, kept apart

| | Activity feed | Audit trail |
|---|---|---|
| What it is | a convenience view of recent operator changes | the authoritative record of what operators did |
| Where | inside desired state (ADR 0021 §6), unchanged | behind the `AuditTrail` port |
| Who can change it | anyone with write access to the desired-state repository | nobody: append-only, and verifiable |
| Bounded by | `GET /api/activity` returns the newest 200 and says how many it left out (`limit`, `omitted`) | nothing prunes it; each entry is capped |
| Covers | the writes ADR 0021 §6 lists | every `control_plane.audit.*` event, refusals included |

The feed is bounded **as a view**: the response is cut, not the stored
history. Nothing is removed from desired state by this decision. The console
labels the feed "Recent activity", says it is not the audit trail, and shows
how many older actions are not shown.

### 2. The audit trail

Every function in `fabric-control-plane`'s `audit` module emits its log line
exactly as before and returns the same values as an `AuditRecord`. The caller
hands it to the `AuditRecorder`, which appends it to the trail when the
deployment has one. That covers all ten events: identity, creation,
product, catalogue, secret, publication trigger, registry (refusals
included), component selection (refusals included), and the lifecycle events
added by #137, `client_tombstoned` (S16) and `client_deletion_confirmed`
(S17).

**Append-only.** The `AuditTrail` port has `append` and `read`. There is no
update, delete, truncate or expiry, and an adapter must not offer one by
another route.

**Hash-chained per stream.** A stream is one resource: `client.<id>`,
`catalogue`, `registries` or `publication` — a closed set, so a record cannot
name a stream that does not exist, and each stream's text is safe as a file
name. Each entry carries its sequence (from 1), its time, the previous
entry's hash (all zeros for the first), the record, and its own hash: SHA-256
(`ring`) over a canonical JSON encoding of everything but the hash. `verify`
walks a stream and reports the first break as itself:

| Break | Meaning |
|---|---|
| `Tampered` | an entry's content no longer matches its hash |
| `BrokenLink` | an entry names a predecessor other than the entry before it |
| `Missing` | a sequence is absent and never appears later: an entry was dropped |
| `OutOfOrder` | the expected sequence appears later: entries were reordered |
| `ForeignEntry` | an entry belongs to another stream |
| `Truncated` | the stream ends before the head recorded for it |
| `AnchorMismatch` | the entry at the recorded head has a different hash: the chain was rewritten and re-hashed |

Per stream rather than one global chain, so one client's history verifies
without reading every other client's, and unrelated appends do not contend on
one head.

**Bounded entry size.** An encoded entry over `MAX_AUDIT_ENTRY_BYTES`, 4 KiB,
is refused with `AuditError::EntryTooLarge`. It is never shortened: a
shortened record would be a different record carrying a valid hash. Every
record holds validated names, revisions, digests and codes, a few hundred
bytes, so the cap is ample headroom and refuses free text that slips in.

**No silent pruning.** Nothing prunes, trims or expires. A stream that does
not verify is refused, not repaired: the local adapter refuses to open, and
any adapter refuses to extend a broken stream, because a new entry chained
onto it would lend it a valid head. A last line left half-written is reported,
not skipped.

**Behind a port.** `AuditTrail` (`fabric-control-plane`) is a boxed,
stateless I/O seam. `InMemoryAuditTrail` serves tests; `LocalAuditTrail`
(`fabric-control-plane-api`) is the development adapter.

### 3. When an append fails

The record is appended after the change it records, because it carries the
revision the write produced. Failing the request then would report a change
that landed as one that did not, and invite a retry that makes a second
change. So an append failure does not fail the operation. It is logged at
`error` as `control_plane.audit.append_failed`, or
`control_plane.audit.refused` for an oversize entry, and the event's own log
line, already emitted, keeps the content in the log pipeline. A successful
append logs `control_plane.audit.appended` with the stream, sequence and
hash. That line is a second anchor for the head, outside the store.

The append runs in a task the request only awaits, so a disconnect cannot
stop it between writing an entry and recording the head.

### 4. The development adapter: a file, not Git

`LocalAuditTrail` keeps one JSON-lines file per stream, opened for append,
one entry per line, synced before an append returns. The append that creates
a stream also syncs the directory, since a file's sync does not make its name
durable; if that sync fails the append fails, and the stream takes no more
entries until the trail is reopened. Beside each log is `<stream>.head`,
replaced atomically after each append: written, synced, renamed, then the
directory synced. A head that fails any of those leaves the entry appended
and the head stale. An OS lock on the directory refuses a second process, as
`LocalClientRepository` does, and is held by every queued or running append
as well as the trail, so it outlives a dropped trail until that work ends.
Opening verifies every stream against its head and syncs the directory,
along with any ancestor it creates. A stream past its head is a stop or a
failure between the two writes: it is complete, and the head catches up. A
head with no log is a deleted stream, refused like any truncation.

Git was considered and not chosen. Desired state reaches Git through a
hosting provider's contents API, which rewrites a whole file per write and
cannot read one back past about 1 MB. A trail stored that way would be
rewritten, not appended, and would hit the limit that motivated this ADR.
Commits are also authored by the platform's machine identity, and a refused
operation leaves no commit. Git history stays a second copy of attribution
(ADR 0021 §6), not the trail.

### 5. Not activated anywhere

`[audit_trail]` defaults to `mode = "disabled"`. Records then reach the log
pipeline only, as before, and startup logs
`control_plane.audit_trail_disabled`. The only storage on offer is
`mode = "local_directory"`, which startup warns is development only. The
loopback console workbench records to `<workbench data>/audit`, a local
development path. No deployed environment is changed by this ADR.

## What the chain does and does not prove

It proves the entries read back are the entries sealed, in order, up to the
recorded head. It does not prove the head itself is genuine. Someone who can
rewrite a stream's log **and** its head file can produce a stream that
verifies. The defences are outside the store: the `appended` log line carries
every head to the log pipeline, and anchoring heads somewhere the store's
writer cannot reach is part of storage activation (below).

## Deferred, and not decided here

- **Storage activation.** Where the trail lives in a live environment: a
  volume, an object store, or an external append-only service. That includes
  who may write it, how heads are anchored outside it, and multi-replica
  writers (the local adapter is single-process by its lock).
- **Retention.** How long entries are kept, and whether any are ever removed.
  Nothing removes any today.
- **Customer-data commitments.** What the trail may hold about a client, and
  what a client may ask to have exported or erased from it. Records today
  carry operator subjects, client ids, revisions, paths, hosts, digests and
  codes, and never a secret value. This ADR makes no commitment beyond that.
- **Trimming the in-document activity feed.** Once a durable trail is active,
  the activity stored in desired state could be bounded rather than refused
  at the size limit. Until then it is the only persisted copy in a deployed
  environment, so nothing trims it, and ADR 0021's `422 document_too_large`
  bound stands.

## Consequences

- `GET /api/activity` gains `limit` and `omitted` and returns at most 200
  actions. A console that read `activity` alone still works.
- `ControlPlaneDeps` and `RegistryServiceParts` take an `AuditRecorder`.
  `ClientService::new` takes one too.
- A registry refusal made by an extractor is now awaited while it is
  recorded, so `admitted` is `async`.
- Verified by tests: an intact chain verifies; a tampered, dropped, reordered,
  renumbered, truncated, rewritten or foreign entry is each detected
  (`audit_trail/verify_tests.rs`); an oversize entry is refused and nothing is
  written; 32 and 64 concurrent appends leave one unbroken chain; a restart
  continues from the head; edits, drops, reorders, truncation, deletion and a
  torn last line on disk are refused at open
  (`fabric-control-plane-api/tests/local_audit_trail.rs`); S16 and S17 land in
  the client's stream with their revisions, a refused lifecycle write appends
  nothing, and 205 catalogue commands leave a view of 200 and a trail of 205
  that verifies (`fabric-control-plane/tests/audit_trail.rs`).
