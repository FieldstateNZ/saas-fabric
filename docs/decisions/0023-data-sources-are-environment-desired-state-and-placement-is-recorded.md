# ADR 0023 — Data sources are environment desired state, and placement is recorded rather than inferred

- **Status:** Proposed
- **Date:** 2026-09-18
- **Applies to:** `fabric-platform-management`, `fabric-platform-git`,
  `fabric-control-plane`, `fabric-control-plane-api`, the console, the
  publication controller and its Kubernetes adapter (neither built yet), and
  `saas-fabric-platform`
- **Related:** [ADR 0003](0003-data-sources-are-first-class-resources.md);
  [ADR 0006](0006-a-shared-data-source-can-only-serve-discriminator-isolation.md);
  [ADR 0007](0007-isolation-is-checked-against-an-observed-fact-not-a-label.md);
  [ADR 0008](0008-desired-state-is-the-authority.md);
  [ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md);
  [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md);
  [ADR 0022](0022-running-versions-come-from-deployment-evidence.md);
  the platform specification §4, §5, §6, §7, §15, §16, §17, §18

## Context

The runtime plane is complete, tested against a real connector, and at zero
replicas on LucentRoot. [ADR 0018](0018-runtime-state-is-published-as-three-versioned-documents.md)
built the producer of its three documents and then stopped at the input it
could not honestly derive: a published `DataSource` needs a connector, a
connection, a placement class, residency and capabilities; a published tenant
binding needs a `DataSourceId` and, on a shared source, the discriminator
column and *this tenant's value in it*. None of that is in a client document,
which says only `data.primary: {class, provider, region}` — intent, not
placement. ADR 0018 named the missing input `ProvisionedPlacement`, refused to
invent it from a label, and left it unowned.

Two paths were on the table on 2026-09-18. One was to hand-author the three
documents for LucentRoot as ConfigMaps in the platform repository and raise
the replica count — honest, quick, and exactly what the product exists to make
unnecessary. The product owner refused it: *the whole point is to avoid hand
authored.* The other is this decision.

Three things have changed since ADR 0018 was written, and each removes a
reason it stopped where it did:

- **Fabric already writes platform desired state.** Platform Management holds
  a GitHub App on `saas-fabric-platform`, reads `environments/<environment>/components.yaml`,
  and rewrites it whole in one atomic commit. That file is *machine-managed*
  desired state: Fabric writes it, a human edits it only under break-glass,
  and the platform's own checks validate it. The precedent for a second such
  file is the first one.
- **Fabric already reaches the Kubernetes API without a `kube` crate.**
  [ADR 0022](0022-running-versions-come-from-deployment-evidence.md)'s
  `fabric-deployment-kubernetes` speaks plain HTTPS with the pod's projected
  token and the cluster CA. ADR 0018 said the publication adapter would need
  the workspace-wide `kube` ban narrowed "as a decision of its own". It does
  not: the adapter takes the same shape, and the ban stays whole.
- **The console creates desired state.** Since
  [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md)
  an operator creates a client, a catalogue application and a release through
  the console, and each is a conditional write to Git. Declaring a data source
  the same way is not a new kind of act.

## Decision

The runtime's three documents are produced from four inputs, none of them
written by hand at runtime, and the platform gains one new resource and one
new record to hold the two that did not exist.

```text
saas-fabric-platform                    saas-fabric-clients
  environments/<env>/data-sources.yaml    clients/<client>/client.yaml   spec.data (intent)
  environments/<env>/placements.yaml      fabric-catalogue.yaml          releases[].resources
         │                  │                      │                          │
         │   declared       │   recorded           │   read                   │   derived
         └───────┬──────────┴──────────┬───────────┴──────────────┬───────────┘
                 ▼                     ▼                          ▼
          data-sources.json       tenants.json               catalog.json
                 └──────────────── published (ADR 0018) ─────────────┘
                                          ▼
                     three ConfigMaps in platform-system, mounted by the runtime
```

### 1. A data source is environment desired state, declared in the product

`environments/<environment>/data-sources.yaml` in `saas-fabric-platform` is a
machine-managed document beside `components.yaml`, with the same header
contract, the same `schemaVersion` gate, and the same writer: Fabric, through
the Platform Management application, in one commit per change. `dataSources`
is a list of entries, each carrying its own `id`, rather than a map keyed by
id -- the file is parsed with the wire's own envelope type, and the wire's
own published document is a list. An operator declares a data source on the
console's Environments page; the control plane validates it and writes it.
Editing the file by hand remains the break-glass path and is expected to
keep working.

```yaml
schemaVersion: 1
environment: lucentroot
dataSources:
- id: shared-postgres-nz-01
  revision: 3                       # bumped by Fabric on every change
  connector: postgres-nz            # the connector process id the runtime is configured with
  connection:
    kind: named                     # or { kind: secret, reference: <path> }; never a value or the connector's own default
    name: shared
  placement: shared                 # shared | dedicated | high_availability | regulated | development | ephemeral
  residency:
    region: nz
    jurisdiction: NZ
  pool:
    max_connections: 20
    idle_timeout_seconds: 300
    acquire_timeout_seconds: 5
  capabilities:
    writable: true
    accepts_new_tenants: true
  discriminator:                    # required when placement is shared; refused otherwise (ADR 0006)
    column: tenant_key
  labels:
    owner: platform
```

Every field but `discriminator` is the wire contract's own field
(ADR 0018, `data-sources.json`), **spelled exactly as the wire spells it** —
`snake_case`, unlike `components.yaml` beside it — because each entry is
parsed with the wire's own `DataSourceDocument` type rather than a third
declaration of the same shape. A file that is the published document in YAML
cannot disagree with what is published from it. `revision` is the
`BindingRevision` the runtime sees, and the control plane bumps it on every
change so a correction to an endpoint moves one number and no tenant's
([ADR 0003](0003-data-sources-are-first-class-resources.md)).

`discriminator.column` is the one field that is not on the wire's
`DataSource`. It belongs here rather than on the tenant because it is a fact
about the database — the column every collection on it carries — and
[ADR 0006](0006-a-shared-data-source-can-only-serve-discriminator-isolation.md)
makes it the *only* isolation a shared source may serve. The wire contract is
unchanged: the column is copied into each tenant binding when the tenant is
placed, exactly where the runtime reads it today.

**What a human states here is that a database exists.** The database, the
connector process that reaches it and the named connection that connector
holds are the environment's composition — declared in the platform repository
the way Keycloak's database is — and no product flow can invent them. Stating
them once, in a form, is not the runtime state this decision exists to stop
anyone writing.

**Validation is the control plane's and refuses the combinations the runtime
would refuse later.** A `shared` placement without a discriminator column, any
other placement with one, an id that is not a `DataSourceId`, a connection
that carries a value rather than a name or a reference, and a change to a data
source whose held revision the request did not name are all refused before a
byte reaches Git. A data source that a placement record still references
cannot be removed — the publisher already refuses that publication (ADR 0018
part 4), and refusing the edit is the earlier, clearer place to say so.

### 2. Placement is a Fabric write, and the record is the fact

A client's `spec.data.<logical>` is intent. Placing that intent on a data
source is an act of the control plane: it selects a data source in this
environment whose placement class, residency and `acceptsNewTenants` admit the
intent, allocates the tenant's isolation on it, and **records the outcome** in
`environments/<environment>/placements.yaml` — one commit, beside the data
sources it refers to.

```yaml
schemaVersion: 1
environment: lucentroot
placements:
  - tenant: acme
    logical: primary
    data_source: shared-postgres-nz-01
    isolation:
      kind: discriminator
      column: tenant_key
      value: acme
    placed_at: 2026-09-18T02:14:00Z
```

`placements` is a list, each entry carrying its own `tenant` and `logical`
rather than a map keyed by either — the same reason `data-sources.yaml` is a
list keyed by nothing: the file is parsed with the wire's own
`PlacementRecord` shape, sorted by (`tenant`, `logical`) so an unrelated
edit produces no diff, and a second declaration of the same shape as a map
is a second place for the two to disagree.

Publication reads this record and copies it into the tenant binding. It never
recomputes it. That is how this decision meets
[ADR 0007](0007-isolation-is-checked-against-an-observed-fact-not-a-label.md)
and ADR 0018's refusal to derive placement from a label: the published value
is the one the record holds, the record was written once by the act that
allocated it, and a break-glass edit to the record is honoured as written
rather than overruled by a rule that runs again. The fact is the record.

**On a shared data source, the allocated discriminator value is the tenant
id.** ADR 0018's objection to `format!("tenant-{client}")` was that a value no
provisioner wrote could collide, and that a value nobody recorded is a label.
Both are answered by the record, not by the formula: the tenant id is a
validated identifier unique within the platform by construction, the
placements document refuses two tenants with one value on one data source,
and the value published is read from the record. For a tenant with no rows
yet, a predicate that matches nothing is the correct answer.

**Dedicated and schema placement are not placeable yet, and say so.** A
`dedicated` data source is one tenant's database; placing a second tenant on
it, or placing a tenant on a data source that does not exist yet, requires
provisioning a database or a schema, which nothing in the platform does. The
control plane refuses the placement with the reason, and the console shows
the client as *not placed* with that reason, rather than binding a tenant to a
database that was never made. Provisioning is the next decision, not this one.

**Every non-shared class holds at most one tenant, and that is the whole
rule -- not a separate check.** The selector considers a `dedicated`,
`high_availability`, `regulated`, `development` or `ephemeral` data source a
candidate only while it holds no placement at all; the first tenant to ask
for that class takes it. A second tenant is refused, and the message says
which of two things is true: nothing declared admits the class or region at
all (the operator's next step is to declare one), or something does and
every matching data source already has a tenant (the operator's next step
is provisioning, which this decision does not build). Collapsing the two
into one message would send an operator to declare a data source they
already declared; keeping them apart costs one more refusal variant, named
for what it is rather than folded into "nothing admits this".

**Placement runs when something changes and can be asked for.** A client
creation, a change to `spec.data`, or a new data source that accepts tenants
is what makes placement possible; the controller in part 4 attempts it on
each pass and records only successes. An operator can also ask for it
explicitly, and is told why when it is refused.

### 3. The runtime catalogue is derived from applications

`catalog.json` maps logical resource names to collections and is the same for
every tenant (ADR 0018). Nothing declares those resources today. This decision
puts them where the application is defined: a published application release
in the product catalogue (ADR 0021) carries `resources`, one entry per logical
resource with the wire's own fields — logical data source, collection, key
field, operations, queryable fields. The runtime catalogue is one document
for the environment, and releases of one application coexist because
clients are pinned to versions, so for each resource name the derived
definition is the one from the **newest published release** of the
application that declares it: a newer release supersedes an older definition
of the same resource, and a client on an older release sees the newer shape.
Two **different** applications declaring one resource name is a conflict.
Publication is what takes a name: the second application's release is
refused when it is published, naming both applications; a draft never owns a
name, because an unpublished draft blocking the rightful owner's next release
would be the wrong way round. A conflict that a hand edit produces is
reported by the derivation, never resolved by guessing a winner.

A resource exists for every tenant whether or not the tenant has the
application, because a catalogue entry is a name-to-collection mapping and not
an entitlement; what a caller may do with it remains the Data API's
permission model and the application's own. Whether an application's
resources should instead be scoped to the tenants it is assigned to is a
question for the Experience API, not for the wire contract.

### 4. Publication is a controller in the control plane, writing ConfigMaps over plain HTTPS

The caller ADR 0018 named lives in the control plane, on a schedule, with its
own service account — no borrowed operator authority, so a poll is safe
(ADR 0018, "The production owner"). On each pass it reads the four inputs,
composes a complete `RuntimeSnapshot`, offers each document at the revision
the adapter currently holds, and advances a document's revision only when the
adapter reports its bytes diverge. A publication that changes nothing writes
nothing.

The Kubernetes adapter is its own crate, implements `RuntimePublication` and
nothing else, and reaches the API server the way `fabric-deployment-kubernetes`
does: `reqwest`, the projected service-account token read per request, the
cluster CA from the mounted root, no redirects, bounded response sizes. It
writes the three ConfigMaps ADR 0018 names, in `platform-system`, with a Role
scoped by `resourceNames` and without `delete`. **The workspace-wide ban on
Kubernetes client crates stays exactly as it is.** ADR 0018's open question is
closed by the precedent rather than by an exception.

The platform repository's part is wiring, never data: the Role, the
RoleBinding for the control plane's account into `platform-system`, and the
runtime Deployment's three whole-volume mounts referencing the ConfigMaps by
name. The ConfigMaps themselves are declared nowhere, so no Argo sync can
revert a publication.

### 5. The runtime is raised when the publication is complete, and not before

The runtime Deployment stays at zero replicas until the controller reports
all three documents published at a revision, the environment's connector
configuration names the connectors the data sources reference, and the
issuer-to-tenant registry covers the placed tenants. Raising the replica
count is then a one-line platform change, as its README already says. It is
not a step this decision automates, because a runtime that starts with a
document missing fails closed at startup, and the platform should be told
before that happens rather than discover it in a crash loop.

## What is built first

This decision is delivered in slices, each ending in something an operator
can open, in this order:

1. **Data sources** (part 1): the model, the platform Git read and write, the
   API, and the Environments page. Proposed together with this decision.
2. **Placement** (part 2): the record, the selector, the client page's data
   row, and the explicit request.
3. **Resources on a release** (part 3): the catalogue model, the derived
   runtime catalogue, and its view.
4. **The publisher** (part 4): the adapter crate, the controller, the platform
   panel's publication row, and the platform repository's wiring.

Slice 4 is specified in full by ADR 0018 and could be built in parallel; it is
sequenced last so that it lands with inputs to publish rather than dark.

## Consequences

### Good

- **Nothing at runtime is written by hand**, and nothing published is derived
  from a label. The three documents come from declared infrastructure, a
  recorded placement, and declared application resources.
- **Break-glass keeps working**, for the same reason it does for
  `components.yaml`: a machine-managed file a human can edit, in the
  repository that already explains why an environment is the way it is.
- **The `kube` ban is untouched.** ADR 0022 already showed the shape.
- **Every refusal moves earlier.** A data source the runtime would refuse is
  refused at declaration; a placement the publisher would refuse is refused at
  placement; a catalogue conflict is refused at release publication.

### Bad, and accepted

- **Two more machine-managed files in the platform repository**, and two more
  schemas for `scripts/check.py` there to validate. The alternative — a store
  Fabric owns — loses the property that the repository explains the
  environment when Fabric is the thing that is broken.
- **The discriminator value is the tenant id.** Readable, and therefore a
  fact an operator could infer. Nothing reads it except the runtime, the
  column never appears in a response, and the record is still the authority.
  If a deployment ever needs an opaque value, the record can hold one without
  a contract change.
- **Placement on anything but a shared source is refused until provisioning
  exists.** A dedicated tenant on LucentRoot waits for a decision this one
  deliberately does not make.
- **The runtime's issuer registry is still configuration.** ADR 0019 G4a
  stands: `[identity].trusted_issuers` is not one of the three documents, and
  a placed tenant whose issuer the runtime does not list is refused at the
  edge. See "What this does not decide".

## Alternatives rejected

| Alternative | Why rejected |
|---|---|
| Hand-author the three documents for LucentRoot | The product exists to make this unnecessary, and the product owner refused it. It would also have made the first live tenant a state nothing could reproduce. |
| Derive placement from `spec.data` on every pass | ADR 0007 and ADR 0018 both forbid it. A rule that runs again can produce a different answer from the one a tenant's rows were written under. |
| Declare data sources in `saas-fabric-clients` | A data source is in one environment and is not a client's. The clients repository is per client and environment-agnostic. |
| Record placements in Fabric's own store (OpenBao) | The rollback hold went to platform Git in 2026-08 for the reason that applies here: if Fabric is dead or bypassed, the repository must still explain why a tenant is where it is. |
| Put the discriminator column on each tenant binding only | It is on the tenant binding on the wire, and stays there. Declaring it once per data source is what lets the control plane refuse a shared source with no column before any tenant is placed. |
| A fourth document for the issuer registry, now | Nothing has designed how the runtime's edge and the runtime's registry are generated from one tenant list. Doing it as a side effect here would be the accident ADR 0019 G4a warns against. |

## What this does not decide

**Provisioning a database or a schema.** Placing on a `dedicated` source, or
creating a data source because a client asked for one, is provisioning, and
this decision refuses rather than pretends.

**The connector deployment.** A data source names a connector id; that the
runtime's `[[connectors]]` lists it, and that an `ndc-postgres` process exists
on the environment configured with the named connection, is platform
composition. The publisher can report a data source whose connector the
runtime does not list; it cannot deploy one.

**The issuer-to-tenant registry** (ADR 0019 G4a), and the identity edge
(ADR 0019 §G). Both stand between a published tenant and a served request.

**Schema migration.** A collection a resource names must exist in every data
source a tenant may be placed on. Nothing here creates tables.

**Deprovisioning.** Removing a placement, and what an empty tenants document
means for a tenant's rows, is the deprovisioning question ADR 0021 already
owes an answer to.

**A resource on a logical data source a client never asked for.** A
resource exists for every tenant, but a tenant whose `spec.data` declares no
binding for the resource's logical data source fails closed at the runtime
(ADR 0018, `UnboundDataSource`). Nothing cross-checks that at assignment;
surfacing it on the client's Data tab is owed.

**Multiple environments.** A deployment manages one environment and its files.
A client placed in two environments is two placement records in two
repositories, and nothing here reconciles them.
