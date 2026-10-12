# ADR 0023 — Data sources are environment desired state, and placement is recorded rather than inferred

- **Status:** Accepted as amended (2026-10-11, under the D01-2 and D01-27
  dispositions in [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415)).
  Acceptance covers the decision as amended below. It does not verify the
  items listed under [Owed](#owed-not-yet-built), which remain open under
  [#112](https://github.com/FieldstateNZ/saas-fabric/issues/112).
- **Acceptance order (D01-19d):** this ADR, then
  [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md),
  then [ADR 0026](0026-a-component-describes-itself-in-an-artifact-attached-to-its-image.md).
  Each is accepted through its own reviewed amendment.
- **Date:** 2026-09-18; amended 2026-10-11
- **Applies to:** `fabric-platform-management`, `fabric-platform-git`,
  `fabric-control-plane`, `fabric-control-plane-api`, the console, the
  publication controller and its Kubernetes adapter, and `saas-fabric-platform`
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
keep working, under the same rule the control plane follows: a hand edit
that changes a data source bumps its `revision`, and nothing restores an
earlier entry by reverting the file (see
[Amendment 2026-10-11](#amendment-2026-10-11-roll-forward-only-d01-27)).

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
    revision: 1                     # this record's own revision; bumped by a break-glass edit
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

**Each record carries its own revision, and a tenant's published revision is
the sum of its records'.** `PlacementRecord.revision` is the same kind of
number `data-sources.yaml` already carries per entry: nothing in this slice
edits or removes a record, so nothing bumps it yet, but a future break-glass
edit that changes what a record says — a different data source, a different
isolation — must bump it, for the same reason correcting a data source's
endpoint must. Publication sums a tenant's records into the `revision` on
that tenant's published binding, rather than taking their maximum or
tracking a revision of its own: a sum moves forward on every record that is
added or bumped, and never moves backward, which is the one property the
runtime's monotonic-revision guard needs — the tenant's binding revision is
not a fact about any one record, it is a fact about how many times *any* of
the tenant's data has changed. A second record placed for a tenant, or a
correction to the one it already has, always advances the sum; nothing here
removes a record, so nothing here needs the sum to fall. **A sum is only
safe while nothing removes a record**: deleting one while bumping a
survivor by less than the deleted record's own revision can leave the sum
exactly where it was, and the runtime, seeing an unmoved revision, keeps
serving the binding the removed record was part of -- so a hand removal
must bump a surviving record by at least what it deletes, and until
deprovisioning is designed (see "What this does not decide") a record must
not be removed by hand at all.

**On a shared data source, the allocated discriminator value is the tenant
id.** ADR 0018's objection to `format!("tenant-{client}")` was that a value no
provisioner wrote could collide, and that a value nobody recorded is a label.
Both are answered by the record, not by the formula: the tenant id is a
validated identifier unique within the platform by construction, the
placements document refuses two tenants with one value on one data source,
and the value published is read from the record. For a tenant with no rows
yet, a predicate that matches nothing is the correct answer.

**A non-shared placement takes a dedicated source that is already declared.
It never provisions one.** *(Amended 2026-10-11, D01-2. The text this
replaces said dedicated placement was refused outright, which contradicted
the next paragraph and the selector as built.)* Two acts are distinct here,
and this decision makes only the first:

- **Taking an already-declared, compatible dedicated source.** An operator
  has declared a `dedicated`, `high_availability`, `regulated`, `development`
  or `ephemeral` data source in `data-sources.yaml` (part 1), which states
  that its database, connector process and named connection already exist in
  the environment's composition. When it matches the intent's class and
  region, accepts new tenants and is writable, and holds no placement, the
  control plane may place one tenant on it, isolated as a whole database
  (`{"kind": "database"}`). This is recording a fact about a database a human
  declared, the same act as placing on a shared source.
- **Provisioning a new database or schema.** Creating a database, a schema,
  a role or a connector process because a client asked for one. **Nothing in
  Fabric or the platform does this, and Fabric makes no claim that it
  happens automatically.** Per D01-3, it will be platform composition per
  client. Until then, a dedicated intent with no free declared source is
  refused with the reason. The client's Data tab shows the refusal
  (`PlacementRow`) rather than binding a tenant to a database that was never
  made.

`schema` isolation is never produced, because nothing creates a schema
(`placements/select.rs`, rule 4).

**Every non-shared class holds at most one tenant, and that is the whole
rule, not a separate check.** The selector considers a non-shared data source
a candidate only while it holds no placement at all, and the lowest id among
free candidates is taken (`placements/select.rs`, rule 4). A second tenant is
refused, and the message says which of two things is true:

- nothing declared admits the class or region at all. The operator's next
  step is to declare a source (`NoDataSourceAdmits`);
- something does, and every matching data source already has a tenant. The
  operator's next step is to have a database provisioned through the
  environment's composition and then declare it (`AllMatchingSourcesOccupied`).
  This decision does not build that provisioning.

Collapsing the two into one message would send an operator to declare a data
source they already declared.

**Placement can be asked for.** A client creation, a change to `spec.data`,
or a new data source that accepts tenants is what makes placement possible.
An operator asks for it explicitly through
`POST /api/clients/{clientId}/placements/{logical}` and is told why when it
is refused. `GET /api/clients/{clientId}/placements` previews the outcome
without writing. The original text also had the part 4 controller attempt
placement on each pass. **That is not built:** the publication pass reads
placement records and never creates them. It is listed under
[Owed](#owed-not-yet-built).

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

**The ownership split stands as amended (D01-2).** The platform owns the
wiring. The control plane owns the data: the published documents, and the
`data-sources.yaml` and `placements.yaml` desired state they are composed
from. This amendment changes no Role, RoleBinding, mount or credential. ADR 0018's
"as built" RBAC amendment applies: `get`, `create` and `update` only; `get`
and `update` are scoped by `resourceNames`, and `create` cannot be.

> **As built.** The controller is `fabric-platform-management`'s
> `publication` module (`compose`, `RuntimePublisher`, the offer-and-advance
> retry in `protocol.rs`) plus `fabric-control-plane`'s
> `RuntimeCatalogueSource` implementation and `POST /api/platform/publication`,
> plus `fabric-control-plane-api`'s schedule
> (`startup/platform/publishing.rs`) and `[platform_management.publication]`
> configuration. One structural change from the paragraphs above: the
> `RuntimePublisher` itself is built once, in `build_control_plane`, from
> `ControlPlaneDeps.publication` — not by `fabric-control-plane-api`'s
> `startup::platform::establish`, which only builds the *sink* (D5's
> `PublicationSink`, wrapping `KubernetesRuntimePublication`) and hands it
> in, because building the `RuntimeCatalogueSource` needs the *client*
> desired-state binding, which only the composition root's `build` function
> has in scope alongside the platform binding.

### 5. The runtime is raised when the publication is complete, and not before

The runtime Deployment stays at zero replicas until the controller reports
all three documents published at a revision, the environment's connector
configuration names the connectors the data sources reference, and the
issuer-to-tenant registry covers the placed tenants. Raising the replica
count is then a one-line platform change, as its README already says. It is
not a step this decision automates, because a runtime that starts with a
document missing fails closed at startup, and the platform should be told
before that happens rather than discover it in a crash loop.

**Activation stays explicit and readiness-gated (D01-2, D01-18).** Fabric
reports the readiness gate. A deliberate operator action changes the replica
count, and only once every input and its evidence is ready. `replicas: 0` is
intentional, not a fault. Scaling up cannot make up for absent documents,
issuer registration or connectors. Holding authority over a disposable
environment does not excuse an unmet condition.

As built, Fabric reports only the first of the three conditions: the
publication row on the console's Environments page (`PublicationPanel`, from
`GET /api/platform` and `POST /api/platform/publication`). It shows what the
target holds and what the last pass did. It does not claim the runtime has
reloaded. Fabric does not yet report whether `[[connectors]]` lists every
connector the declared data sources reference, or whether the issuer registry
covers every placed tenant. A gate that reports all three is listed under
[Owed](#owed-not-yet-built).

> **Amendment (2026-10-11, D01-4, D01-18).** Fabric now reports all three
> gates, named as the platform contract names them: `documents`, `identity`
> and `connectors` (`PublicationState::readiness`, the `readiness` field of
> the publication row). This is publisher evidence only.
>
> - `identity` is met when an issuer registry generation is published
>   naming at least one issuer.
> - With client lifecycles (D01-19a), "covers the placed tenants" means
>   every placed tenant whose client permits routing. That holds by
>   construction, because the registry is generated from exactly those
>   clients. Placed tenants that are not ready or are retired are reported
>   as excluded from the generated registry. That is not route withdrawal:
>   no edge or runtime application of the registry is observed.
> - `connectors` is never met by Fabric. Platform composition deploys the
>   connectors, and Fabric observes neither them nor the runtime's
>   `[[connectors]]`. The gate lists the connectors the published data
>   sources name.
>
> Raising replicas stays a deliberate operator action.

> **Amendment (2026-10-12, G7).** Reader evidence now sits beside the three
> gates, per the G7 disposition on
> [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6118073758).
>
> - **The documents gate is unchanged.** It is still publisher evidence, so
>   a runtime at zero replicas is raised against it with no bootstrap
>   cycle.
> - **The readers row.** It holds a sample of every replica that could
>   serve, from every replica set of the runtime Deployment, against the
>   record the last accepted pass stamped. The answer is `applied`,
>   `pending`, `unobserved` or `noReaders`.
> - **`updateComplete`.** It is true only when the documents gate is met
>   and the readers are applied. It is never true at zero replicas.
>
> With no reader target configured, which is every environment today, the
> readers are unobserved. Live use of the replicas' reader-generation
> listener is not approved. Scaling stays a deliberate operator action, and
> nothing scales, repairs or rolls back on this evidence.

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
- **A non-shared tenant needs a declared, free source of its class.** A
  dedicated tenant is placed only when an operator has declared a matching
  dedicated source that the environment's composition already provides. With
  none free, the tenant is refused until one is provisioned outside Fabric
  and declared. Automatic provisioning does not exist (D01-3).
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

**Provisioning a database or a schema.** Creating a database, a schema or a
data source because a client asked for one is provisioning, and this decision
refuses rather than pretends. Placing one tenant on a dedicated source an
operator already declared is not provisioning (part 2). D01-3 assigns
provisioning to platform composition per client.

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
owes an answer to. D01-14 gives its direction: tombstone first, retained
state, then confirmed deletion as a separate operation. Until that exists, no
placement is removed, whether by hand or by a revert (see the amendment
below).

**A resource on a logical data source a client never asked for.** A
resource exists for every tenant, but a tenant whose `spec.data` declares no
binding for the resource's logical data source fails closed at the runtime
(ADR 0018, `UnboundDataSource`). Nothing cross-checks that at assignment;
surfacing it on the client's Data tab is owed.

**Multiple environments.** A deployment manages one environment and its files.
A client placed in two environments is two placement records in two
repositories, and nothing here reconciles them.

**Extended by [ADR 0026](0026-a-component-describes-itself-in-an-artifact-attached-to-its-image.md) (2026-09-29).** A published release's resources
are its *effective* resources: the ones an operator authored and the ones its
described components declare. The runtime catalogue and the publish-time
cross-application check both read the effective set.

## Amendment 2026-10-11: roll forward only (D01-27)

Recorded from the D01-2 and D01-27 dispositions in
[#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415).
ADR 0018's amendment of the same date covers the runtime documents. This
section covers the two platform files this decision adds.

- **Placements roll forward only.** A record is added by `Placements::place`
  and nothing else. Fabric has no operation that edits, reverts or removes a
  record. A break-glass edit that changes what a record says bumps that
  record's `revision`, so the tenant's summed revision moves forward.
- **No blind placement revert or removal.** Reverting `placements.yaml` in
  Git, or deleting a record by hand, is not a supported rollback. Either can
  drop a tenant from `tenants.json`, which the runtime treats as
  deprovisioning, or lower the tenant's summed revision, which a running
  runtime ignores. Removing a placement waits on deprovisioning (D01-14).
- **A referenced data source changes only through a write that bumps its
  revision.** The console's declaration (`DataSources::declare`) recomputes
  the revision as held + 1 whenever any field differs, and ignores the
  revision the request carries. A hand edit must do the same. Removing a data
  source a placement still names is refused (`DataSources::remove`,
  `DataSourceInUse`). Reverting `data-sources.yaml` is not a supported
  rollback either. To restore an earlier connection, write it again as a new
  revision.
- **Any break-glass Git effect is characterized in tests before it is
  claimed as an operational rollback.** None is claimed here.

### Verified against the code (at the base of this amendment)

| Claim | Where |
|---|---|
| A non-shared source is a candidate only while it holds no placement. The lowest id wins, and isolation is `database` | `crates/fabric-platform-management/src/placements/select.rs` (rule 4), `select/pick.rs::pick_exclusive`; `select_tests.rs::rule_4_a_dedicated_source_with_any_held_placement_is_not_a_candidate`, `::rule_4_a_free_dedicated_source_is_isolated_as_a_whole_database` |
| Occupied and inadmissible are separate refusals | `placements/refusal.rs` (`AllMatchingSourcesOccupied`, `NoDataSourceAdmits`) |
| Nothing provisions a database, schema or connector | No such code exists in `crates/`. Placement writes only `placements.yaml` |
| Placement happens only on an explicit request | `crates/fabric-control-plane/src/handlers/placements/post.rs` is the only production caller of `Placements::place` |
| Placement records are only ever added. Each starts at revision 1 | `placements/service.rs` (`place`, `for_client` only); `placements/record.rs` (`FIRST_REVISION`) |
| A tenant's published revision is the sum of its records' revisions | `publication/snapshot.rs::compose_tenants`; `snapshot_tests.rs::a_tenants_revision_is_the_sum_of_its_records_own_revisions` |
| A data source correction moves its revision to held + 1, and the incoming revision is ignored | `data_sources/plan.rs`; `plan_tests.rs::a_changed_field_moves_the_revision_forward_by_one`, `::an_incoming_revision_is_never_trusted` |
| A data source a placement names cannot be removed | `data_sources/service/remove.rs` (`DataSourceInUse`) |
| The console shows the publication row, with **Publish now** | `apps/control-plane-ui/src/components/PublicationPanel.tsx` (#81) |

### Owed (not yet built)

Accepting this decision does not make these true. Each is tracked against
[#112](https://github.com/FieldstateNZ/saas-fabric/issues/112) for a bounded
PR. The IDs match ADR 0018's list and `docs/roadmap/m0-contract-decisions.md`.

O1 (the publisher guards resource revisions) and O2 (a lost manifest is held
at the revision the publication record names) are listed in ADR 0018. Both
are implemented in [#136](https://github.com/FieldstateNZ/saas-fabric/pull/136).
O3 and O4 are implemented, and O5 is partly characterized, as described
below.
#136 also narrows O6 on the reader side. Within one process, a request is
resolved against one generation's tenants and data sources, held together
across the binding, the exclusivity check and the execution target. What
remains of O6 is listed in ADR 0018. None of this is verified live.

- **O3. Implemented, not verified live.** A data source that is removed and
  declared again no longer restarts at revision 1. `data_sources/plan.rs`
  writes a declaration above the higher of its held revision and the
  highest revision the id was ever published at. That revision is read from
  the publication record's `resources` history and the held documents
  (`RuntimePublication::published_resources`). The publisher refuses a
  resource offered at or below it (ADR 0018, O3). History starts with the
  first record that carries it.
- **O4. Implemented, not verified live.** `compose` holds the placement
  records to the tenants document last published. A published tenant or
  logical binding with no record left is refused (`PlacementRemoved`).
  Records that no longer reach the tenant's published revision, or that
  reach it with a changed binding, are refused (`PlacementLowered`). A Git
  revert of `placements.yaml` therefore cannot deprovision a tenant or move
  it back. The way forward is a revision-bumping write. The publisher also
  refuses a dropped tenant (`TenantRemoved`) unless the whole document is
  emptied with intent. No withdrawal path exists yet: retiring a client
  records intent only and leaves its placements in composition, so
  retirement cannot stall publication on these guards.
- **O5. Partly characterized.** Reverts of `placements.yaml` and
  `data-sources.yaml` are driven through the router, the controller's pass
  and a real filesystem target
  (`fabric-control-plane/tests/platform_publication_revert.rs`). Each is
  refused and leaves the published files unchanged. The runtime is not
  driven in those tests; it keeps the generation the files hold. A revert
  of the catalogue or of client documents is still not characterized.
- **O7. The controller does not attempt placement on each pass.** Placement
  happens only on an explicit request (part 2).
- **O8. The readiness gate reports three gates, from publisher evidence
  only (part 5, D01-18).** It does not observe connectors or what an edge
  applied. Those remain the platform's check. What the runtime's replicas
  serve is reported beside the gates since G7. That report is built in
  source, but unobserved until a reader target is approved and configured.
  Not verified live.
