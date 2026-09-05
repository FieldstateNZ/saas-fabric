# ADR 0018 — Runtime state is published as three independently versioned documents

- **Status:** Proposed
- **Date:** 2026-09-06
- **Applies to:** `fabric-runtime-publication` (new), `fabric-tenant-runtime`,
  `fabric-data-api`, `fabric-api`, the Kubernetes adapter that will follow, the
  as-yet-unnamed control-plane crate that will eventually call this port, and
  `saas-fabric-platform`
- **Related:** [The control plane](../architecture/control-plane.md) §"Runtime
  publication boundary" (corrected in part by this decision — see "The
  production owner"); [ADR 0003](0003-data-sources-are-first-class-resources.md);
  [ADR 0006](0006-a-shared-data-source-can-only-serve-discriminator-isolation.md);
  [ADR 0007](0007-isolation-is-checked-against-an-observed-fact-not-a-label.md);
  [ADR 0008](0008-desired-state-is-the-authority.md);
  [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md);
  the platform specification §6, §7, §16, §18, §20, §21, §28

> **Placement note.** This repository records decisions in
> `docs/decisions/NNNN-<declarative-title>.md`, numbered sequentially, and the
> highest number at `f9c5abf` is 0017. This is therefore 0018. There is no
> `docs/architecture/decisions/` directory and none should be created; the
> file-size policy already links to `docs/decisions/0001-...` by that path.

---

## Context

[ADR 0008](0008-desired-state-is-the-authority.md) closed by naming runtime
binding publication as a future reconciliation target and deliberately not
building it. [The control-plane architecture](../architecture/control-plane.md)
says the same at more length: *"This is the seam this increment deliberately
documents rather than builds."*

The cost of leaving it unbuilt is now the binding constraint on everything else.
The runtime plane is complete and tested — tenant resolution, the Data API,
connectors, the whole isolation story — and it sits at zero replicas for one
reason: **nothing writes the files it reads.** Both planned consumers (the
Synthesis Cloud record-isolation seam, the Slipway hosted API) are blocked
behind that single missing step, and so is M2's LucentRoot activation.

### What the runtime already does, and why almost none of it needs changing

The consumer half is not a sketch. `fabric-tenant-runtime`'s reconciled-resource
lifecycle already enforces, per resource, every rule this decision needs:

- a revision that only moves forward, with an older one ignored;
- a same-revision *divergent payload* refused, counted and logged, rather than
  quietly accepted (`ApplyReport::divergent_payload`);
- a full sync, so absence means deprovisioning;
- a failed load that leaves the last good snapshot serving, untouched;
- a first load that would install nothing refused outright, leaving the process
  unprimed and answering 503 rather than ready over an empty set.

So this decision is almost entirely about the **producer**, and its first
obligation is to add nothing to the consumer. The runtime's consumption shape is
treated here as **frozen**.

### The three facts that shape the answer

**One — the producer cannot share a type with the consumer.** `TenantRuntimeBinding`
and `DataSource` live in `fabric-tenant-runtime`; `IsolationModel` and
`ConnectionSelector` live in `fabric-connector`. Both are runtime plane, and
`scripts/check_architecture.py` fails the build on an edge from the control
plane to either. `ResourceCatalog` and `ResourceDefinition` are worse: they
derive `Deserialize` only. Nothing in this workspace can serialise a catalogue
at all.

**Two — the payload files have nowhere to put an envelope.** `tenants.json` and
`data-sources.json` are bare JSON arrays; `catalog.json` is a bare object,
deserialised through `#[serde(transparent)]`. Adding a `revision` or a
`contract_version` field to any of them changes the consumption shape.

**Three — the input that would make this useful does not exist.** A client
document declares `spec.data.primary: {class, provider, region}` — intent. A
published `DataSource` needs an id, a connector, a connection selector, a
placement class, residency and pool settings; a published tenant binding needs a
`DataSourceId` and, on a shared DataSource, a discriminator column *and this
tenant's value in it*. That is the output of provisioning, which does not exist.

---

## Decision

**Runtime state is published as three independently versioned documents, each
written atomically beside a sidecar manifest, by a producer that shares no Rust
type with the runtime and never derives physical placement from client intent.**

Nine parts.

### 1. The contract is the JSON, and one crate owns it

A new crate, **`fabric-runtime-publication`**, owns the wire contract, the
publication port, and the filesystem adapter. It belongs to **neither plane**:
its only non-dev internal dependency is `fabric-core`, exactly as `fabric-core`
itself belongs to neither.

It declares its own document types. Fidelity to the consumer is guaranteed by
several mechanisms working together, not by a shared type:

- **`#[serde(deny_unknown_fields)]` on the consumer's types makes the contract
  self-enforcing in both directions.** A field the producer adds that the
  consumer does not know is a deserialisation error; a required field the
  producer stops emitting is a missing-field error. Concretely, ten fields
  carry no default and so fail loudly on omission or misspelling: `tenant`,
  `revision` (a tenant binding); `data_source`, `isolation` (its logical
  binding); `id`, `connector`, `connection`, `placement`, `residency` (a
  DataSource); `data_source`, `collection` (a catalogue entry). Everything
  else defaults — including a binding's own `data` map, whose omission yields
  an empty map that then fails `RegistryResource::validate` and is dropped,
  with the held copy retained, logged and counted (`merge.rs:41-60`).
- **`queryable_fields` gets no such protection from the consumer, so the
  producer supplies its own.** The consumer's `ResourceDefinition` defaults it
  to empty — unrestricted — via `#[serde(default)]` (`resource_definition.rs:63`),
  right for a hand-written catalogue but wrong for a generated one: a producer
  bug that drops the field would silently unrestrict a resource. The
  producer's own type declares it **non-optional** and always emitted, so an
  omission is a compile error, not a permissive default reaching the wire.
- **Every identifier is validated through the same rules the consumer uses.**
  `ConnectorId`, `ConnectionName`, `FieldName` and `CollectionName` live in
  `fabric-connector`, which is runtime plane, so the producer cannot depend on
  them — it declares its own newtypes over the same public functions,
  `fabric_core::naming::parse_dns_label` and `parse_identifier`, exactly as the
  consumer's own `identifier_newtype!` macro does (`#[serde(try_from = "String")]`
  in both places). One invalid identifier fails the whole file the same way on
  both sides — `SourceError::Malformed` on the consumer's, a construction
  error on the producer's.
- **A composed acceptance test** publishes through the real port and drives the
  real `JsonFileSource`, the real `build_runtime` and the real `build_data_api`
  router. Tenants and data sources go through `build_runtime` directly; the
  catalogue leg cannot reuse `fabric-api`'s `startup::catalog::load` — it is
  `pub(super)` — so the test deserialises the published `catalog.json` into the
  real, public `ResourceCatalog` type itself, the same type `load` builds, and
  hands it to `build_data_api`. That is a test that the real catalogue type and
  router accept the producer's output, not a test that reuses the runtime's own
  startup path. The compiler pins one half; this test pins the other.

`fabric-control-plane` gains **no** dependency on `fabric-tenant-runtime`, and
the architecture check continues to refuse one.

### 2. Three documents, three revisions, one full snapshot each

| Document | Payload | Manifest |
|---|---|---|
| tenants | `tenants.json` — JSON array | `tenants.manifest.json` |
| data sources | `data-sources.json` — JSON array | `data-sources.manifest.json` |
| catalogue | `catalog.json` — JSON object | `catalog.manifest.json` |

Each document is **always a complete set**. There is no partial publication and
the port offers no way to express one. This is forced, not chosen:
`ResourceRegistry::apply_all` is a full sync, so a document missing a tenant
deprovisions that tenant.

The catalogue is the one exception, and it goes the other way: it may never
legitimately be empty. `fabric-data-api`'s own `build_data_api` refuses to start
against an empty catalogue (`registration.rs:30-32`) — a Data API with no
resources can serve nothing, which is a configuration error, not a valid state
to reconcile toward. So the bootstrap value that primes cleanly, `[]`, is
legitimate for tenants and for data sources — an empty tenant population and an
empty DataSource population are both things a fresh platform genuinely starts
from — but `catalog.json` has no such bootstrap value. The publisher refuses an
empty catalogue document outright, at the same place it refuses every other
whole-publication problem, before any byte is written.

Each document carries **its own revision**, in its own manifest, advanced
independently. That is ADR 0003's independence made operational: correcting a
shared DataSource's endpoint bumps one revision and rewrites one file, leaving
every tenant record and every tenant revision untouched.

The port takes all three on every call. Independence comes from the revisions,
not from optional arguments — an `Option` per document would let a caller
publish a tenants document referencing a DataSource it did not publish, and the
referential check below would have nothing to check against.

### 3. Write order: data sources, then catalogue, then tenants

A tenant binding naming a DataSource the registry has not loaded resolves to
`MissingDataSource` — a 500. So additions land bottom-up, which is the same
order `build_runtime` already primes in.

Removals cannot be ordered safely in the same pass by write order alone, and
this decision says so rather than pretending otherwise:

> **A DataSource is retired in the publication *after* the one that unbound its
> last tenant.**

That rule is enforced, not merely documented. Before writing anything, the
publisher reads the **held** tenants document — the one currently on disk, not
the incoming one — and refuses a data-sources document that drops a
`DataSourceId` the held document still references. Because the check reads the
*held* document, a caller cannot retire a DataSource in the same publication
that unbinds its last tenant: at the moment that publication is evaluated, the
held document still names it, and the whole publication is refused. Retiring a
DataSource is therefore genuinely two publications — one that unbinds it, then
a second that drops it once the first is held — and the wrong order is
unreachable, not merely inadvisable.

### 4. Referential integrity is checked before any byte is written

A publication is refused, in whole, if the tenants document names a
`DataSourceId` the data-sources document does not contain. That combination is
guaranteed to produce a 500 on the request path for those tenants
(`ResolveError::MissingDataSource`), and it is cheap to see from here.

A catalogue entry naming a logical data source that some tenant does not bind is
**not** refused — that is a legitimate intermediate state, in the same way a
client with no OIDC clients declared is legitimate. The runtime already answers
it as `ResolveError::UnboundDataSource`, which is a **500**, not a 4xx: nothing
the caller sent is wrong, the tenant is simply not reconciled onto that
resource yet, and `fabric-data-api`'s own status mapping treats it as a
platform failure like `MissingDataSource`
(`fabric-data-api/src/errors/status_mapping.rs:41`). Concretely: a tenant whose
catalogue includes `auditEvents` on logical `audit` but whose binding never
added an `audit` entry gets a 500 on that one resource, and the honest name for
that state is a reconciliation gap on the platform's side — not a client error,
and not something this decision refuses to publish. The shipped
`examples/catalog.json` ships exactly this shape today: `auditEvents` is bound
only by `acme`, and `globex`/`initech` would 500 on it.

### 5. Atomic replacement, payload before manifest

Each file is written to a sibling temporary file in the same directory,
`fsync`ed, and then `rename(2)`d over its target. Same directory, because
`rename` is atomic only within a filesystem.

Within a document, the **payload is replaced before its manifest**. Restated
precisely, because a crash mid-write is exactly the case an atomicity argument
has to survive: suppose the held state is revision `R` with payload bytes
`P_old`, and a publication moves to revision `R + 1` with payload `P_new`. If
the process crashes after the payload rename but before the manifest rename,
disk now holds payload bytes `P_new` under a manifest still claiming revision
`R`. Two things can happen next, and both are correct:

- **A retry of the same publication** — revision `R + 1`, payload `P_new` —
  compares against the held manifest's revision `R`. `R + 1 > R`, so the
  verdict is *write*: both files are rewritten, the payload byte-identical, and
  the manifest catches up to `R + 1`. That is recovery, indistinguishable from
  an ordinary publication.
- **A publication that instead restates the old revision** `R` with the old
  payload `P_old` compares, at equal revisions, against what is actually on
  disk — `P_new`, not `P_old`, because the crash left the payload ahead of the
  manifest. Bytes differ at an equal revision, so this is refused as
  `DivergentPayload`.

Neither outcome makes recovery impossible; the first *is* the recovery, and the
second refuses to silently resurrect a value already superseded on disk. What
atomicity buys beyond that is stated honestly: the consumer already survives a
torn read (`serde_json::from_slice` fails, `SourceError::Malformed`, the
registry untouched, the last good snapshot serving). Atomic replacement removes
a spurious alarm and a stale window, not a data-loss risk — there was not one.

### 6. Monotonic revision, and same-revision divergence, detected by bytes

Before writing anything, the publisher reads two independent facts per
document: whether a manifest is held, and whether a payload file is held. Only
one combination is the steady state; the other three are reachable and each has
a defined verdict.

| Held manifest | Held payload | Verdict |
|---|---|---|
| absent | absent | First publication. Write |
| absent | present | Write. There is no manifest to hold a revision, so this is treated as a first publication regardless of what the file already contains — **the divergence guard is off in this state.** This is the state of the shipped `examples/*.json` today: the payload files ship, no manifest ships beside them |
| present | absent | Write. There is nothing to diverge from, so a publication at exactly the held revision is accepted — this is a republication, not a divergence |
| present | present | compared below |

When both are held, the verdict follows the incoming revision against the held
one and, at equal revisions, a byte comparison against the held payload:

| Incoming vs held | Verdict |
|---|---|
| `revision` **older** than held | **Refuse the whole publication** — `StaleRevision` |
| `revision` **equal**, bytes differ | **Refuse the whole publication** — `DivergentPayload` |
| `revision` **equal**, bytes identical | No-op. Nothing is written, not even the manifest |
| `revision` **newer** | Write |

Divergence is detected by **comparing the canonical serialised bytes against the
bytes on disk**, not by a digest. The publisher is already holding both strings;
byte comparison has no collision question, needs no hash crate, and costs
nothing under the dependency policy. The Kubernetes adapter inherits the same
technique for free, because `ConfigMap.data` values are strings.

Refusing rather than accepting mirrors `ApplyReport::divergent_payload`
deliberately, and for the same reason recorded there: accepting the newer
payload would make the revision meaningless, and two writers racing at the same
revision would have their outcome decided by arrival order.

**Orthogonal to every row above, one more guard applies: a publication that
would take a currently non-empty document to an empty one is refused** unless
the caller states that intent explicitly — a per-document `Emptying` value on
the snapshot passed to the port, not a flag on the port itself. One rule, no
threshold, checked the same way as the rows above, and impossible to bypass by
forgetting it. This is the producer-side analogue of `UnusableFirstLoad`: that
guard stops an empty result from being mistaken for a legitimate first state;
this one stops an empty result from being mistaken for a legitimate *change*.
What it prevents is concrete — a scheduled publication whose input query
returned zero rows, a bug upstream of this crate entirely, would otherwise
deprovision every tenant to a 403, silently, on the next sweep.

**The caller states the revision.** The publisher does not generate it. That is
what makes "a stale publication is refused" a reachable state rather than an
unreachable branch, and it is the same shape as `ClientRepository::update`,
which takes the revision the caller believed it was editing. The type is a new
newtype in `fabric-runtime-publication`, **`DocumentRevision`**,
`#[serde(transparent)]` over `u64`, only ever moving forward — deliberately not
`fabric_core::BindingRevision` (`crates/fabric-core/src/ids/binding_revision.rs`),
which is the *resource's* revision carried inside a document. The two numbers
measure different things, and sharing one type would make that distinction a
matter of convention rather than of the type system.

The production shape this supports: the scheduler that eventually calls this
port holds no durable state of its own. It reads the held manifest's revision
each sweep and publishes at `held + 1` — safe only because there is exactly one
writer, which is what the RBAC's `resourceNames` scoping in "The production
owner" below guarantees.

### 7. No secret ever enters a published document

The document types have no field that could hold one. A connection is a
selector — a name the connector already holds configuration for, or a reference
to a secret (§21). A tenant's `secrets` field is a base path. Nothing in
`fabric-runtime-publication` is handed a secret resolver, and it depends on no
crate that has one.

### 8. Serialisation is deterministic and safe for ConfigMap data

- Every map is a `BTreeMap`, so key order is total and stable.
- Resource arrays are sorted by key (`TenantId`, `DataSourceId`) before
  serialisation, so an unrelated edit produces no diff.
- Two-space pretty-printing and a trailing newline, so a human reading a
  `kubectl get -o yaml` sees something readable and a diff is line-oriented.
- No floating-point value appears in any document.
- Output is UTF-8 JSON, which is what `ConfigMap.data` requires (`binaryData` is
  never used).
- Every file name is a valid ConfigMap data key (`[-._a-zA-Z0-9]+`).

Serialising the same snapshot twice produces identical bytes. That property is
load-bearing, and it means the exact rules above — key ordering, sort order,
whitespace, the trailing newline — are part of the wire contract, not an
implementation detail: the divergence check in part 6 is a byte comparison, so
a change to *how* a document is formatted, with no change to what it says, is
indistinguishable from a change to its content. A hand-edited file that only
reformats whitespace refuses the next same-revision publication as
`DivergentPayload`, exactly as a real content change would; the recovery is
either to publish at `revision + 1` or to restore the file's canonical bytes.
A publication at a genuinely newer revision overwrites a hand edit
unconditionally, canonical formatting included.

### 9. Versioning: a sidecar manifest, and a v2 that ships alongside v1

Each manifest carries `contract_version`. The runtime does not read it and does
not need to — an incompatible *shape* already fails loudly through
`deny_unknown_fields`. What the version buys is the migration path for a change
of *meaning* at an unchanged shape.

A breaking change ships as **new file names and new ConfigMap keys alongside the
old ones**, never as a reinterpretation of documents already published — the
same rule the client desired-state document already follows.

---

## The wire contract

Three payload documents and three manifests. Reproduced in full, because a
contract that has to be inferred from Rust is not a contract.

### `tenants.json`

A JSON array. Each element is one tenant's complete runtime binding.

```json
[
  {
    "tenant": "acme",
    "revision": 42,
    "data": {
      "primary": {
        "data_source": "shared-postgres-02",
        "isolation": {
          "kind": "discriminator",
          "column": "tenant_key",
          "value": "tenant-482"
        }
      }
    },
    "features": { "invoicing": true }
  },
  {
    "tenant": "globex",
    "revision": 7,
    "data": {
      "primary": {
        "data_source": "shared-postgres-02",
        "isolation": {
          "kind": "discriminator",
          "column": "tenant_key",
          "value": "tenant-915"
        }
      }
    },
    "features": { "invoicing": true }
  }
]
```

| Field | Required | Rule |
|---|---|---|
| `tenant` | yes | DNS label |
| `revision` | yes | `u64`, only ever increases for a given tenant |
| `data` | yes, non-empty | logical data source name → binding. An empty map fails `validate` and the binding is dropped |
| `data.*.data_source` | yes | a `DataSourceId` that appears in the same publication's `data-sources.json` |
| `data.*.isolation` | yes | `{"kind":"database"}`, `{"kind":"schema","schema":"..."}`, or `{"kind":"discriminator","column":"...","value":"..."}` |
| `configuration` | no | `{"store":"...","profile":"..."}` |
| `secrets` | no | a **reference** path such as `vault/tenants/acme`. Never a value |
| `features` | no | name → bool |
| `storage` | no | name → storage binding |

Unknown fields are rejected, at every level. A tenant binding may carry **no**
physical configuration: `connector`, `connection`, `pool` and the rest belong to
the DataSource, and a binding that tries to carry one is a hard error rather
than a silently ignored field.

### `data-sources.json`

```json
[
  {
    "id": "shared-postgres-02",
    "revision": 3,
    "connector": "postgres-au-east",
    "connection": { "kind": "named", "name": "shared-02" },
    "placement": "shared",
    "residency": { "region": "au-east", "jurisdiction": "AU" },
    "pool": {
      "max_connections": 50,
      "idle_timeout_seconds": 300,
      "acquire_timeout_seconds": 5
    },
    "capabilities": { "writable": true, "accepts_new_tenants": true },
    "labels": { "owner": "platform", "tier": "standard" }
  }
]
```

| Field | Required | Rule |
|---|---|---|
| `id` | yes | referenced by tenant bindings |
| `revision` | yes | independent of every tenant revision |
| `connector` | yes | a `ConnectorId` the host has registered |
| `connection` | **yes** | `{"kind":"default"}`, `{"kind":"named","name":"..."}` or `{"kind":"secret","reference":"..."}`. Deliberately not defaultable — two DataSources that both said nothing were two ids and one database |
| `placement` | yes | `shared`, `dedicated`, `high_availability`, `regulated`, `development`, `ephemeral` |
| `residency` | yes | `{"region":"...","jurisdiction":"..."}` |
| `pool` | no | defaults applied by the consumer |
| `capabilities` | no | **defaults closed** — `writable: false`, `accepts_new_tenants: false` |
| `labels` | no | operator taxonomy, emitted with telemetry |

Note the interaction with [ADR 0006](0006-a-shared-data-source-can-only-serve-discriminator-isolation.md):
a `placement: "shared"` DataSource may only serve tenants whose isolation is
`discriminator`. The publisher does not enforce it — the resolver does, per
request, against the set — but a publisher that produces the combination is
producing tenants that will fail closed.

### `catalog.json`

A JSON object keyed by logical resource name. Platform-level and identical for
every tenant.

```json
{
  "articles": {
    "data_source": "primary",
    "collection": "articles",
    "key_field": "id",
    "operations": ["read", "list"],
    "queryable_fields": ["id", "title", "body", "published_at"]
  }
}
```

| Field | Required | Rule |
|---|---|---|
| `data_source` | yes | a **logical** name, resolved per tenant through the binding. Never a `DataSourceId` |
| `collection` | yes | the physical collection the connector knows |
| `key_field` | no | defaults to `id` |
| `operations` | no | defaults to `["read","list"]` — a resource must be *deliberately* made writable |
| `queryable_fields` | **yes, in the producer's own document type** | on the consumer, empty means "no restriction" (`resource_definition.rs:63`); the producer emits the field unconditionally rather than relying on that default, so a producer bug that drops it is a compile error rather than a silently unrestricted resource on the wire |

The discriminator column must **not** appear in `queryable_fields`, and the
runtime hides it regardless — by an **exact-case** comparison
(`visible_fields.rs:55`), not a case-insensitive one. So the column name the
producer emits must match the schema's own spelling exactly: a discriminator
the schema calls `tenant_key` is not hidden by a catalogue or a provisioner
output that spells it `Tenant_Key`.

### `<document>.manifest.json`

```json
{
  "contract_version": 1,
  "document": "tenants",
  "revision": 9
}
```

Three fields, and no more. `revision` here is a `DocumentRevision` (part 6),
not the `BindingRevision` carried inside the payload's own resources — the two
numbers are unrelated and this file only ever holds the former. In particular
there is **no timestamp**: nothing branches on one, the audit event already
carries the time, and a timestamp in a ConfigMap is a diff that churns on every
publication for no reader's benefit.

---

## The input this decision does not have

**The publisher's input is not client desired state, and must never be derived
from it.**

A client document declares intent:

```yaml
  data:
    primary:
      class: dedicated
      provider: sql
      region: au-east
```

A published `DataSource` needs an id, a connector, a connection selector, a
placement class, residency, pool settings and capabilities. A published tenant
binding needs a `DataSourceId` and, on a shared DataSource, a discriminator
column and *this tenant's value in it*. None of that is derivable from the four
lines above. It is the output of provisioning — which the platform does not do
yet, and which is out of scope here.

The tempting shortcut is one line:

```rust
// Do not write this.
let id = DataSourceId::try_new(&format!("{client}-primary"))?;
let value = format!("tenant-{client}");
```

**That is inventing an observed fact from a label**, which is exactly what
[ADR 0007](0007-isolation-is-checked-against-an-observed-fact-not-a-label.md)
forbids, and the failure mode is the worst one this platform has: a tenant
boundary that looks configured and is not. A discriminator value that no
provisioner ever wrote into a database matches no rows for one tenant and, if
two clients ever normalise to the same string, matches another tenant's rows.

So this decision **names the missing input rather than filling it in**. The
unresolved input is a per-tenant provisioner output — working name
`ProvisionedPlacement`. Its fields below are **necessary, not sufficient** —
they are what the wire contract demonstrably needs, not a claim that nothing
else will turn out to be required — and it has no owner yet:

| Field | Why it cannot come from intent |
|---|---|
| `tenant: TenantId` | Which tenant this output belongs to — the join key back to the client that requested it |
| `logical: LogicalDataSourceName` | Which of the tenant's logical bindings (`primary`, `audit`, ...) this placement fills |
| `data_source: DataSourceId` | Which physical resource was actually allocated, or which existing shared one the tenant was placed on |
| `connector: ConnectorId` | Which connector process reaches it |
| `connection: ConnectionSelector` | The name the connector was configured with, or the secret path the credential was written to |
| `placement: PlacementClass` | What was *provisioned*, which may not be what was asked for |
| `residency: DataResidency` | Where it actually landed |
| `isolation: IsolationModel` | Structural or discriminator — and for a discriminator, the column the schema actually has (exact case: §"The wire contract" above) and the value the provisioner actually wrote |

One consequence worth stating where the next worker will look for it:
`DataSourceCapabilities` defaults closed (`writable: false`,
`accepts_new_tenants: false`), so a `ProvisionedPlacement` that allocated a
DataSource meant to accept writes must say so explicitly — silence there
produces a DataSource that publishes successfully and then refuses every
write, which is a confusing way to discover a missing field.

Until that input exists, `fabric-runtime-publication` has a port with no
production caller, and that is the correct state for this milestone rather than
a gap in it. The seam is what unblocks the four workers listed below; the
provisioner is a separate decision with its own workflow, its own idempotency
questions and its own failure semantics.

---

## The production owner

**A least-privileged Fabric controller writes three named ConfigMaps in
`platform-system`.** Not built here; specified here so it can be built without
reopening this decision.

The future caller does **not** live in `fabric-reconciliation` or an unnamed
"sibling", as
[the control-plane architecture](../architecture/control-plane.md)'s "Runtime
publication boundary" section currently says. This decision supersedes that
sentence: the caller lives in a **control-plane crate**, which may depend on
`fabric-runtime-publication` — a `check_dependency_direction` `expected` entry
to add when that crate is built, exactly as `fabric-reconciliation` already
depends on `fabric-client-model`. `fabric-control-plane` itself still gains no
dependency on `fabric-tenant-runtime`: the new crate belongs to neither plane
specifically so a control-plane caller can reach it without reaching the
runtime plane at all.

| ConfigMap | Data keys | Mounted at |
|---|---|---|
| `fabric-runtime-tenants` | `tenants.json`, `tenants.manifest.json` | the runtime's `tenants_path` directory |
| `fabric-runtime-data-sources` | `data-sources.json`, `data-sources.manifest.json` | its `data_sources_path` directory |
| `fabric-runtime-catalog` | `catalog.json`, `catalog.manifest.json` | its `catalog_path` directory |

- **Exactly one writer each** — the controller's ServiceAccount, and nothing
  else. Concurrency between control-plane replicas is answered by there being
  one writer, not by a merge.
- **Not Argo-owned.** The strongest form of that is not an annotation: **the
  three objects are not declared in the platform repository at all.** An Argo
  application holding a manifest for them would revert every publication on its
  next sync. `saas-fabric-platform` declares the ServiceAccount, the Role, the
  RoleBinding, and the runtime Deployment's volume mounts referencing them *by
  name* — the wiring, never the data.
- **RBAC:** `get, list, watch, create, update, patch` on `configmaps`,
  `resourceNames` restricted to exactly those three, in `platform-system` only.
  **No `delete`.** Deleting a ConfigMap the runtime mounts is an outage;
  deprovisioning is expressed as an empty set *inside* a document.
- **The controller may run on a schedule.** This is a deliberate asymmetry with
  identity reconciliation, which cannot: [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md)
  removed the scheduled sweep because Keycloak is acted on with a borrowed
  operator bearer and the platform holds no standing credential. Publication
  writes ConfigMaps with the controller's *own* ServiceAccount, so no borrowing
  is involved and a poll is both safe and correct.
- **The adapter refuses an oversized document rather than splitting it.** A
  ConfigMap is capped at 1 MiB for the whole object. Splitting a document across
  objects would break full-snapshot semantics and silently deprovision whatever
  fell off the end.
- **The volume mount is never `subPath`.** A `subPath` ConfigMap mount is
  resolved once at pod start and never receives updates for the life of the
  pod — the kubelet's live-update mechanism only refreshes whole-file mounts.
  Every mount here is the whole ConfigMap volume; the runtime reads the one
  file it needs out of a directory that may hold others, which is already how
  it is configured.
- **The controller creates valid empty documents at startup if none exist**,
  for tenants and data sources. A pod mounting a ConfigMap that does not exist
  stays in `ContainerCreating`. `[]` and `[]` are legitimate and prime cleanly,
  which is a better answer than `optional: true` volumes and a replica that
  starts unprimed. The catalogue has no such bootstrap value (part 2): the
  controller never creates an empty `catalog.json`, and the publisher refuses
  one. The catalogue ConfigMap's first value must be a real catalogue,
  published once before the runtime is first rolled out — a deployment-time
  step, not a controller default.

---

## What downstream work may rely on, without re-deriving any of this

### `saas-fabric-platform` (mounts and RBAC)

1. Three ConfigMaps, named as above, in `platform-system`.
2. Two data keys each: `<name>.json` and `<name>.manifest.json`. Both are UTF-8
   strings in `data`; `binaryData` is never used.
3. The platform repo owns the ServiceAccount, Role, RoleBinding and volume
   mounts. It does **not** declare the ConfigMaps and must not add them to any
   Argo application.
4. The Role needs no `delete`, and must be `resourceNames`-scoped.
5. The runtime Deployment's three paths are already configurable
   (`tenants_path`, `data_sources_path`, `catalog_path`).
6. Mounting a directory with extra files in it is harmless — the runtime reads
   the one file it was told about.
7. Every mount is a whole-volume mount. **Never `subPath`** — a `subPath` mount
   never sees an update, which would make every rule above true on disk and
   false in the running pod.

### The Kubernetes adapter

1. It implements the `RuntimePublication` port defined in
   `fabric-runtime-publication`; it does not invent a second contract.
2. It belongs in its **own crate**, because `kube` and `k8s-openapi` vocabulary
   must be contained the way `fabric-keycloak` contains Keycloak's — and note
   both are currently banned workspace-wide by
   `scripts/check_architecture.py`, so that ban has to be narrowed to the new
   adapter crate as part of that work. That narrowing is a decision of its own.
3. Atomicity is free: the kubelet swaps the mounted symlink atomically. The
   adapter still writes payload before manifest, for the crash-ordering reason
   in part 5.
4. Divergence detection is a string comparison of `ConfigMap.data["<name>.json"]`
   — no digest, no annotation.
5. Refuse a document that would take the object past 1 MiB. Do not split.
6. Create missing ConfigMaps with valid empty documents rather than erroring —
   tenants and data sources only, per "The production owner" above; the
   catalogue ConfigMap must already exist with a real catalogue before the
   adapter is asked to publish to it.
7. The staleness budget a caller may promise is *controller interval + kubelet
   ConfigMap sync (up to about a minute, plus cache TTL) +
   `tenant_runtime.refresh_interval_seconds` (default 30)*. There is no
   in-process trigger reachable from a controller; `RefreshHandle::refresh_now`
   is local to the runtime process.

### Client adoption (the control-plane side)

1. The caller states each document's revision and enforces nothing itself — the
   port refuses stale and divergent publications.
2. The caller must publish a **complete** set per document, every time. There is
   no incremental path and asking for one is asking for a mass deletion.
3. The caller must not derive physical placement from `spec.data`. The input it
   needs is the provisioner output named above and does not yet exist.
4. Retire a DataSource one publication *after* the one that unbinds its last
   tenant. The publisher enforces this (part 3): the wrong order is refused,
   not just discouraged.
5. A publication that changes nothing is free — it writes no bytes and produces
   no diff — so calling it on every sweep is correct and cheap.
6. A `409`-shaped outcome (`StaleRevision`, `DivergentPayload`) means *read the
   current revisions and try again*, exactly as a client-document write does.
7. Taking any document from non-empty to empty requires stating that intent
   explicitly (part 6); a caller that always treats "current state" as
   authoritative and never means to deprovision everything should never need
   to construct it.

### The Synthesis Cloud record-isolation seam

1. Two tenants on **one shared DataSource** with **different discriminator
   values** is the supported and tested configuration. It is also the *only*
   isolation model a shared DataSource may serve (ADR 0006) — `database` and
   `schema` contribute no predicate and are refused there.
2. The predicate reaching the connector is produced in exactly one place,
   `IsolationModel::tenant_predicate`, and applied unconditionally by
   `QuerySpec::for_target` / `MutationSpec::for_target`. A caller's own filter is
   conjoined with it and can never displace it.
3. The discriminator column never appears in a response, on any route, whatever
   `queryable_fields` says — matched by exact case, so the column name a
   provisioner writes must match the schema exactly.
4. The tenant comes from the bearer token and from nowhere else. `X-Tenant-Id`
   is refused with a `400`, and publication introduces no second selection path.
5. `articles` (or any logical resource) is one catalogue entry for every tenant.
   Where the rows live differs per tenant; the application contract does not.
6. A tenant whose binding is missing, whose DataSource has been removed, or
   whose registry has not primed **fails closed**, with distinct statuses — 403,
   500 and 503 respectively — that must not be collapsed. A tenant whose
   catalogue entry names a logical binding it never made also fails closed, at
   500 (`UnboundDataSource`), and that is a reconciliation gap to page on, not
   a client error to swallow.

---

## Consequences

**No runtime-plane crate changes at all.** Not one file under
`crates/fabric-tenant-runtime/src`, `crates/fabric-data-api/src` or
`crates/fabric-connector/src`. That is the strongest evidence available that the
consumption shape really was frozen, and it is worth protecting: a future change
that "just adds a field" to `TenantRuntimeBinding` is a wire-contract change and
should be reviewed as one.

**A crate in neither plane is not a new category — five already exist — but it
does need a new invariant.** `fabric-core`, `fabric-git-host`,
`fabric-platform-git`, `fabric-platform-management` and `fabric-registry` are
all in neither `RUNTIME_PLANE` nor `CONTROL_PLANE` today, and `fabric-git-host`
does network I/O — so "neither plane" was never the same claim as "does no
I/O." `check_dependency_direction` already pins every internal edge for every
crate, dev tables included, in its `expected` table; the gap is that
`check_the_planes_do_not_meet` skips a crate in neither plane outright
(`else: continue`), so an edge bridging the planes *through* one would pass
even though `expected` records the direct edges either side of it. `expected`
is hand-maintained, though, so this decision adds two checks that do not depend
on it, computed instead from the plane sets directly:

- for every crate, the set of plane crates it reaches over non-dev edges must
  lie within one plane;
- no crate in `RUNTIME_PLANE` may name `fabric-runtime-publication` in **any**
  table, dev included — a runtime binary can never link a writer of the files
  it reads.

`fabric-runtime-publication` is also deliberately **not** added to
`DOMAIN_CRATES`: domain crates must not know what HTTP is, and this crate's
composed test dev-depends on `axum` and `tower` to drive a real router.

**The composed acceptance test can only live in a crate in neither plane.**
`Graph.direct_dependencies` includes dev tables, so a test in either plane's
crate that dev-depended on the other would fail the plane check. That constraint
is what forces the crate placement, and it is a feature: the crate that owns the
contract is the crate that proves both sides honour it.

**A published catalogue takes effect at the next rollout, not at the next
refresh.** `catalog.json` is read once at process start and never refreshed.
This decision does not change that. A catalogue revision is therefore a
deployment-shaped event, and anyone expecting it to propagate like a tenant
binding will be wrong. One consequence of that worth stating plainly: the
catalogue's document revision has **no consumer at all**. The runtime never
reads `catalog.manifest.json`, so nothing in the running system can answer
"which catalogue revision is this replica serving" except by asking when it was
last rolled out. Making the catalogue a fourth reconciled resource is a
separate decision, and would be the point at which that revision starts
meaning something operationally.

**Removals are enforced, not just bounded.** For a single writer — guaranteed
by the RBAC in "The production owner" — the held-document check in part 3
makes it structurally impossible to publish a data-sources document that drops
a DataSource a live tenant binding still names; the publication is refused
outright rather than left to fail closed later. What remains is the harmless
case: a caller unbinds a tenant and never gets around to retiring the
now-unused DataSource, which sits published and idle. A second writer racing
the same ConfigMaps outside this guarantee is a different problem, addressed
under "What this does not decide" below.

**There is a second revision concept in the system.** Documents carry a
`DocumentRevision`; the resources inside them carry their own `BindingRevision`.
They are not the same type, not the same number, and should never be
conflated: the resource revision drives the runtime's per-resource guard and
its change events; the document revision drives the publisher's refusals. The
manifest keeps them physically apart.

**Two copies of the wire types now exist.** The producer's and the consumer's.
That duplication is not free, and the mitigation — `deny_unknown_fields` plus a
composed test — is what makes it safe rather than what makes it disappear.
A reviewer should treat a change to either copy as a change to both.

**Publication has no production caller yet.** This milestone delivers the seam,
not the deployment. The runtime does not scale up until the Kubernetes adapter
and a scheduled caller exist, and the caller is not useful until the provisioner
output does.

---

## What this does not decide

**Provisioning.** How a database is created, how a discriminator column and
value are chosen and written, and what happens when provisioning half-succeeds.
Named above as the unresolved input, deliberately not designed here.

**The Kubernetes adapter.** Specified above so it can be built without reopening
this decision, and out of scope for the milestone that adopts it. It also needs
its own decision about narrowing the workspace-wide `kube` / `k8s-openapi` ban,
which currently makes the adapter uncompilable by design.

**Making the catalogue refreshable.** It is startup-scoped today and this
decision leaves it there.

**Deletion of a ConfigMap, or of a document.** Deprovisioning is an empty set
inside a document. Nothing deletes an object, and the RBAC does not permit it.

**Multiple writers.** The design answers concurrency with "there is one writer."
A second controller replica publishing at the same revision is outside what this
guarantees; leader election, or a compare-and-set on the ConfigMap's
`resourceVersion`, is the shape of the answer if that ever changes.

**Storage, Events, and the remaining sections of a tenant binding.** `storage`
and `configuration` are carried through the contract because the consumer's type
has them and `deny_unknown_fields` means they cannot be quietly dropped. Nothing
publishes meaningful values into them yet.

**`ResourcePermissions`.** It is process configuration, read from `config.toml`
at startup, not a published resource — nothing in this decision changes that or
gives it a place in any of the three documents.

**Where the document revision comes from in production.** The caller states it.
Which caller, and how it decides, belongs with whatever schedules publication.
