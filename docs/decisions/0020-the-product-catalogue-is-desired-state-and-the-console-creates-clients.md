# ADR 0020 — The product catalogue is desired state, and the console creates clients

- **Status:** Proposed
- **Date:** 2026-09-16
- **Applies to:** `fabric-client-model`, `fabric-client-git`,
  `fabric-control-plane`, `fabric-control-plane-api`, `apps/control-plane-ui`,
  and the `saas-fabric-clients` repository that holds what they write
- **Related:** pull request #69;
  [ADR 0008](0008-desired-state-is-the-authority.md);
  [ADR 0010](0010-operators-authenticate-against-the-platform-realm.md);
  [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md);
  [ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md);
  [The control plane](../architecture/control-plane.md);
  [The client desired-state document](../architecture/client-desired-state.md);
  [Fabric Console v0](../product/console-v0.md);
  the platform specification §6, §8, §24

---

## Context

Pull request #69 builds the phase-one operator application: a console that
follows the v2 prototype, and the API and storage underneath it. Most of it is
presentation. Seven parts of it are decisions, and several of them reverse
something this repository already said.

| This repository said | Where | Pull request #69 |
|---|---|---|
| "No **Add client**. Creating a client is a workflow, not a form" | [Fabric Console v0](../product/console-v0.md) | adds `POST /api/clients` and a creation form |
| No `create`, no `delete` | `ClientRepository`'s rustdoc, before this pull request | adds a conditional `create` to the port |
| Client creation is not in this increment | [the control plane](../architecture/control-plane.md), the README | builds it |
| "There is no second posture, and no development shortcut … Local development therefore needs a Keycloak" | the control plane, "Who an operator is" | adds a loopback workbench that authenticates with a test operator |
| Every section under `spec` this model does not own is preserved untouched | [the client document](../architecture/client-desired-state.md) | claims `spec.product`, and refuses any other shape of it |

The code arrived before the decision. That is the wrong order, and this record
does not pretend otherwise. It states what the code decides, why each part is
defensible, and what each costs, so that each can be accepted, amended or
reversed deliberately rather than discovered in a diff. Until it is accepted,
the architecture documents describe the pull request's behaviour and point here.

The frame does not change. [ADR 0008](0008-desired-state-is-the-authority.md)
holds in full: a mutation writes desired state and calls no platform service, a
write answers `pending`, reconciliation adds and never deletes, and a concurrent
edit is refused. [ADR 0012](0012-the-platform-acts-on-keycloak-as-the-operator.md)
holds: anything that reaches Keycloak does so with the operator's own bearer.
[ADR 0019](0019-the-edge-proves-the-token-and-the-issuer-names-the-tenant.md)'s
public-client contract is what §4 below writes. Nothing here adds a path from a
handler to a platform service.

## Decision

### 1. A product catalogue is desired state

The platform's product definition is one document, `fabric-catalogue.yaml`, at
the root of the client configuration repository — beside the client documents,
not under their path prefix.

```yaml
apiVersion: fabric.fieldstate.nz/v1
kind: Catalogue
spec:
  applications: []
  clientFields: []
  settings:
    platformName: SaaS Fabric
    defaultRegion: New Zealand
    timezone: Pacific/Auckland
  environments: []
  activity: []
  definitionVersion: 0
```

`apiVersion` and `kind` are checked before anything else is parsed, as the
client document's are, so a file that is not a catalogue is refused as the
wrong document rather than as a catalogue missing every field. **The envelope is
storage only.** `GET` and `POST /api/catalogue` send the body and a revision, and
their JSON did not change when the envelope was added.

| Section | Holds |
|---|---|
| `applications[]` | an `id`, an editable `draft`, and `releases[]` — immutable, numbered, oldest first |
| a definition | a name, a description, a hostname template containing `{client}`, and the five lists below |
| `components[]` | `kind` — `container`, `helm` or `capability` — a reference, a version, whether every plan includes it, and an `automatic` or `manual` policy |
| `features[]` | the components that implement each feature |
| `plans[]` | the features a plan grants, and non-secret plan configuration |
| `fields[]`, `clientFields[]` | typed, non-secret configuration: `text`, `number`, `boolean`, `choice`, `hostname`, `identifier` or `timezone` |
| `navigation[]` | a label, a same-origin route, an optional feature gating it, and a permission name the application enforces |
| `settings` | the platform's display name, and defaults for new clients |
| `environments[]` | links to other operator consoles |
| `activity[]` | operator-authored catalogue changes (§6) |
| `definitionVersion` | the shared client contract's version, moved by every `clientFields` save |

A capability component names one of a closed list — `Identity`, `Database`,
`Secrets`, `Authorization`, `Routing`, `Object storage`, `Messaging` — and
anything else is refused. A capability name is a promise the platform makes, and
a free-text one is a promise nobody checked.

**A caller sends commands, never the document.** `POST /api/catalogue` takes one
of `createApplication`, `saveApplication`, `publishApplication`,
`saveDefinition`, `saveSettings` or `saveEnvironment`. Release numbers,
publication times and activity entries are assigned by the server, and no
command can carry one. A release a caller could write would not be immutable; it
would be a draft with a number on it.

**Publishing validates, then freezes.** `publishApplication` refuses a draft with
no plan, a container or chart component with no pinned version, or a draft
identical to the latest release, and otherwise appends the draft as release
`n + 1`. Editing the draft afterwards changes no release.

**The first write says it is the first.** Every write carries `If-Match` with
the revision it read or — only when no catalogue exists yet —
`If-None-Match: *`. A write carrying neither is refused with `428`, and a stale revision with `409`. This is
ADR 0008's fourth rule applied to a second document, including a case ADR 0008
did not have: a document that does not exist yet, which two operators could
otherwise both create.

**Environment registrations are links.** Each environment runs its own control
plane against its own operator realm. A registration stores a name, a
description and an HTTPS console URL, and nothing that could authenticate to
that console. A registration that could *act* on another environment would need
that environment's credential or a trust between two realms, and neither exists
or has been designed.

**Why desired state rather than a store of its own.** The catalogue inherits
what ADR 0008 already paid for — one authority, conditional writes, a commit per
change carrying a `Requested-by:` trailer — and the control plane stays
stateless. The cost is that the whole catalogue is one file, rewritten on every
change and grown by every release and every activity entry; see Consequences.

### 2. The console creates clients

This reverses Fabric Console v0 and the port's former "no `create`".
`POST /api/clients` takes an id and a configuration, and:

1. reads the catalogue and resolves every requested application against a
   **published** version and a plan inside it, and every configuration value
   against its declared field — an undeclared key is refused, a required one
   enforced, a default filled in, each value checked against its type;
2. builds a `v2` client document: `metadata.name` and `spec.identity.realm` both
   the client id, both required roles, the requested display name and hosts,
   and `spec.product` (§3) carrying a `Client created` activity entry;
3. projects the assigned applications into `spec.identity.clients` (§4);
4. writes it **only if no document has that id**, and answers
   `409 client_exists` otherwise — its own code beside `revision_conflict`,
   because a taken id is fixed by choosing another rather than by re-reading. In
   Git it is a contents write with no expected blob, which the host refuses for
   a file that exists;
5. marks the client `pending`, answers `201` with an `ETag`, and — where an
   identity provider is configured — starts a background convergence as the
   creating operator.

**Why the reversal is defensible.** Console v0 was right that creating a client
is a workflow: routing, data placement, secrets, a database. It still is, and
none of that is built. What this builds is the one step of it that is a
desired-state write — a document — and under ADR 0008 that is the step that has
to come first, because every other step reconciles from it. Nothing is
provisioned and no platform service is called.

**What it costs.** "A client exists" now means "a document exists". A created
client has, once converged, a realm with two roles and a public client per
assigned application — and no route, no DataSource, no secret boundary and no
DNS name. The console must not imply otherwise, and it states each of those as
not observed. The rest of the workflow still owes its own design.

**Why the realm is the client id.** A realm cannot be renamed, so whatever
creation chooses is permanent. The client document already describes client
`acme` as tenant `acme` as realm `acme`; creation enforces that rather than
offering a choice nobody could correct afterwards. The cost is that there is no
way to create a client onto an existing realm of another name.

### 3. A client's product configuration lives in its own document, as full snapshots

Each client document gains `spec.product`:

| Field | Holds |
|---|---|
| `legalName`, `region`, `timezone` | the client's commercial details |
| `definitionVersion` | the catalogue's `definitionVersion` when this client was last saved |
| `configuration` | values for the catalogue's `clientFields` |
| `applications[]` | `applicationId`, `planId`, per-client `configuration`, and `release` — a **complete copy** of the published release |
| `activity[]` | operator-authored changes to this client (§6) |

`PUT /api/clients/{clientId}/product` replaces it and requires `If-Match`; it and
`GET` both answer with an `ETag`. A request names an application, a **version**
and a plan, and the server copies the release in. A snapshot supplied by the
caller would be a forged release, so no request shape carries one.

**In the client's document, not the catalogue.** One write, one revision and one
conditional check cover a client's product configuration and the identity
clients projected from it, so the two cannot be committed apart.

**A copy, not a reference.** A client keeps exactly the definition it was
assigned until an operator changes it — whatever the catalogue later says,
including a catalogue that is later edited by hand or cannot be read. That is
what "an assignment keeps its exact version" has to mean if it is to survive
the catalogue. The cost is duplication: every assigned client carries a whole
release, a correction to a published definition reaches no existing client, and
there is no bulk migration. A client moves to a newer release, or a newer client
definition, only when an operator saves it.

**`spec.product` is owned, and refused unless it is this shape.** Every level is
`deny_unknown_fields`, and a present section must carry all seven keys. It joins
`spec.identity` on the owned side of the document's asymmetry; the rest of
`spec` is still preserved untouched. An absent section reads as empty. On read
only the shape is checked; the rules in step 1 of §2 apply when the control
plane writes.

A product save also migrates a `v1` document to `v2`, because it merges through
the same identity edit that does.

### 4. Assigned applications are projected into identity as public clients

On creation and on every product save, each assignment writes — or replaces, by
id — an entry in `spec.identity.clients`:

| Field | Value |
|---|---|
| `id` | the application id |
| `type` | `oidc` |
| `pkce` | `s256` |
| `redirect.strategy` | `claimedHttps` |
| `redirect.uris` | `https://<hostname template, with {client} replaced by the client id>/callback`, when the release has a template; and `https://<host>/<application id>/callback` for **every** host the client declares |

An assignment with neither a template nor a client host is refused. So is the
first assignment of an application whose id already names an identity client
declared by hand: the projection never claims a client it did not create.

**Why project, rather than reconcile products.** An application a client is
entitled to has to be able to sign that client's users in, and ADR 0019 already
defines what a public client is and how it is converged. Writing into the
existing identity contract means reconciliation needs no change and knows
nothing about products. The costs — a product save reverting hand edits, and
private-network clients being unable to take an application — are under
Consequences.

**What is not projected.** A confidential client, which still needs ADR 0008's
undesigned secret delivery; and any redirect strategy but `claimedHttps`.

### 5. Removing an assigned application is refused until deprovisioning exists

A product save that drops an application the client is currently assigned is
refused as an invalid request, naming deprovisioning. Nothing in the catalogue
can be deleted either; there is no command for it.

Removing the line would leave the projected client in the realm — reconciliation
adds and never deletes — and leave whatever the application did with the
client's data wherever it did it. A document claiming the application was gone
while all of that remained is the "operator removed a line from a YAML file"
ADR 0008 refuses to treat as evidence.

The cost is that an assignment made in error has no exit in the product. The
only one is a hand edit in Git, and that leaves the realm client behind anyway.

### 6. Activity is recorded inside desired state, and only for operator writes

| Write | Recorded in | Action |
|---|---|---|
| a catalogue command | the catalogue's `activity` | for example, `Application definition published` |
| client creation | the client's `spec.product.activity` | `Client created` |
| a product save | the client's `spec.product.activity` | `Client configuration updated` |
| an identity edit | the client's `spec.product.activity` | `Identity updated` |

Each entry is `{at, operator, action, resource}`, written **in the same write as
the change**. `GET /api/activity` merges the catalogue's entries with every
client's, newest first.

In the same write, because a history written separately can disagree with the
change it describes, and one commit is both or neither. Inside desired state,
because the control plane has nowhere else durable to put it, and ADR 0008 keeps
it stateless.

**Activity is a view, not the audit trail.** Each of these writes also emits a
structured audit event to the log pipeline, carrying the operator, the operation
and the resulting revision: `control_plane.audit.client_created`,
`control_plane.audit.product_updated`, `control_plane.audit.identity_updated`
and `control_plane.audit.catalogue_changed`. The catalogue event names no
client; it reads its operation and entry back from the activity entry the
command appended, so the audit record and the console cannot disagree about
what happened. Where those events are durably retained is not built here.

**Reconciliation passes are not recorded.** An earlier revision of this pull
request appended a "pass completed" entry to the catalogue after every sweep. It
was removed for three reasons, any one of which was enough:

- it wrote an **observed runtime event into desired state**, which §6 and
  ADR 0008 keep apart;
- it cost a **commit per pass**, filling history with the platform talking to
  itself;
- it **moved the catalogue's revision under an operator mid-edit**, so their next
  save failed with `409` for a change nobody made.

What a pass finds is each client's reconciliation status, held in memory as
ADR 0008 already describes.

### 7. A persistent local repository, and a loopback workbench

**`local_directory` persists.** It now opens `LocalClientRepository`, in
`fabric-control-plane-api`, instead of holding documents in memory:

- opened with no snapshot, it imports the directory's top-level `*.yaml` files,
  and no subdirectories;
- the first write creates `.fabric-state.json` in that directory, and from then
  on the snapshot is the authority and the YAML is not read again;
- every open parses every stored client and the catalogue, and refuses to start
  on one that does not parse;
- a write serialises the whole snapshot to `.fabric-state.next`, `fsync`s it and
  renames it over the snapshot;
- `.fabric-state.lock` holds an OS lock, and a second process opening the same
  directory is refused;
- it applies the same conditional create, update and catalogue compare-and-swap
  as the other repositories, with revisions `local-<n>`.

The in-memory repository lost everything on restart, which made a catalogue with
published releases impossible to work with for longer than one session.
`InMemoryClientRepository` remains for tests; no deployment mode selects it.

**The workbench.** `cargo run -p fabric-control-plane-api --example
console_workbench` starts the real router over a `LocalClientRepository` in
`.local/workbench`, or in `FABRIC_WORKBENCH_DATA`, and `npm run preview:ui`
serves the console against it.

| Guard rail | Where it is |
|---|---|
| The API binds `127.0.0.1:8082`, and nothing else | a literal in the example |
| The console's server binds `127.0.0.1:5174`, and proxies `/api` to the API, adding `X-Test-Operator` | `apps/control-plane-ui/preview/server.mjs` |
| The operator is `testing::AcceptingOperator`: any request carrying `X-Test-Operator` is `local-workbench`, holding a fixture bearer | the example |
| No identity provider, sign-in, secret store, Git integration or platform management — every one `None` | the example |
| In no image: the Rust image builds three named binaries and no examples, and the console image builds `index.html`, not `preview.html` | `Dockerfile`, `apps/control-plane-ui/Dockerfile` |

Because nothing is connected, **nothing it accepts can be authorised or
converged.** `POST /api/reconciliation` answers that convergence is unavailable,
a write starts no background pass, and a written client stays `pending`.

**This contradicts the control-plane architecture, and is recorded as a
proposal.** That document says there is no development shortcut, and that local
development therefore needs a Keycloak. Deployments still have one posture:
`mode = "oidc"` is the only one configuration accepts, and the workbench is code
in an example binary rather than a setting a deployment could state. But it is a
development shortcut, and the reason given there for refusing one is exactly
true of it — an operator established without a token can authorise nothing, so
everything that reaches a platform service does not work under it.

Its limits, plainly. Loopback is not authentication: **any process on the
machine that can reach either port is an operator.** And `AcceptingOperator`
lives in `fabric-control-plane`'s `pub mod testing`, compiled unconditionally,
so keeping it out of a deployment is a convention of the composition root rather
than a feature gate.

## Consequences

**A document holding a `spec.product` of another shape becomes unreadable to
every path that reads the product.** Before this pull request an unknown
`spec.product` was preserved untouched. Now `GET` and
`PUT /api/clients/{clientId}/product` answer `500 desired_state_invalid` for that
client — the code an unreadable client document already gets, and not retryable.
So does an **identity edit**, which appends activity (§6) and so reads the
product first. So does `GET /api/activity`, which fails **whole** rather than
leaving that client's entries out, as `GET /api/clients` already fails whole on
one unreadable document: a partial feed would read as a quiet day. Listing
clients, a client's overview, reading identity and reconciliation still work,
because parsing a client does not read its product. A survey of
`saas-fabric-clients` for `spec.product` before deployment costs minutes.

**An identity edit writes a `product` section into a document that had none.**
The activity entry needs somewhere to go, so a role change on a client created
before this pull request adds `spec.product` with empty `legalName`, `region`
and `timezone`, `definitionVersion: 0`, no configuration or applications, and
one activity entry. A reviewer reading that commit sees a new section appear
beside a role change.

**Activity grows without bound, and duplicates what Git already records.**
Nothing trims either list. Every catalogue command rewrites the whole of
`fabric-catalogue.yaml` — every release snapshot and every activity entry — and
every product save rewrites the client's document with its release copies. In
Git each of those is a commit whose `Requested-by:` trailer already names the
operator, so the lists are a second copy of attribution kept for the console's
convenience. They are also desired state that anyone with write access to the
repository can edit: a view, not evidence. The structured audit record §24 asks
for is the separate event each of these writes emits (§6).

**A product save reverts a hand edit to a projected client.** The projection
replaces the entry by id, whole: callbacks, strategy and PKCE. An edit to it
through `PUT /api/clients/{clientId}/identity` — which the console does not
offer, because applications are read-only there — or by hand in Git lasts until
the next product save for that client, and the realm is then converged back.
Removing the projected client through the identity API is undone the same way.

**A client with a `.internal` or loopback host cannot be assigned any
application.** Every host a client declares contributes a callback, whether or
not the application has a hostname template, and `claimedHttps` admits public
`https://` hosts only (ADR 0019 §3). One such host therefore refuses every
assignment for that client, and an application whose template resolves under
`.internal` cannot be assigned to anyone. LucentRoot's production hosts are
`.internal`, so its clients cannot take an application today.

**Tightening a validation rule later can make the catalogue unreadable.** Every
read parses and re-validates every stored release with the publication rules.
Remove a capability name, or narrow the route or hostname-template rule, and a
release published under the old rule makes the whole document unreadable — the
catalogue page, client creation, every product save and the activity listing
all read it — answered `500 desired_state_invalid` until the file is corrected by hand, which means editing a release
that was meant to be immutable. Client documents' copies are not re-validated,
so existing clients stay readable.

**`local_directory` has two authorities over time.** Until the first write, the
top-level YAML is what loads; after it, `.fabric-state.json` is, and later edits
to the YAML are ignored without a warning. The shipped
`examples/control-plane.toml` points at `examples/clients`, so running it and
saving once writes the snapshot there, where Git ignores it. A write is atomic —
temp file, `fsync`, rename — but the directory is not `fsync`ed, so on some
filesystems a crash just after the rename comes back with the previous snapshot.
The lock refuses a second process on the same machine.

**Nothing deploys, routes or observes what the catalogue describes.** No
controller deploys a `container` or `helm` component, no DNS name or certificate
is issued for a template or a host, and no runtime health is observed. A
component's `automatic` policy is recorded intent that nothing acts on, and an
application component is not a Platform Management component: it cannot be
paused or rolled back. The console reports deployment, routing and health as not
observed, because Fabric has observed none of them.

**A created client converges with the creating operator's authority, or not at
all.** The background pass borrows their bearer (ADR 0012). If that bearer
cannot create a realm, the client stays unconverged until an operator whose
bearer can runs a pass.

**One revision covers the whole catalogue.** Two operators editing different
applications conflict: the second is refused and re-reads. That is the price of
one document, and it is why §6 could not let anything but an operator's own
change move that revision.

## What this does not decide

**Deprovisioning.** Removing an application from a client, and deleting a
client, an application or an environment registration. §5 refuses the first;
nothing expresses the rest.

**Deploying application components**, issuing their DNS names and certificates,
and observing their runtime health.

**How catalogue validation rules may change** without making stored releases
unreadable — whether a rule change ships as a new `apiVersion`, or a release is
validated against the rules it was published under.

**Confidential application clients**, and any projected strategy other than
`claimedHttps`.

**Permission enforcement.** Navigation is an entitlement preview; the
application enforces its own permissions.

**Acting across environments.** A registration is a link.

**A durable audit store.** Client creation, a product save, an identity edit and
a catalogue command each emit a structured audit event, but to the log pipeline;
where those events are retained, and for how long, is not decided here. Activity
is not that record and does not replace one.

**Replicas.** Every write here is a conditional write the Git host checks
atomically, so none of it rests on process-local coordination. The local
repository is single-process by its lock, and is a development mode only.

## Decisions owed to the product owner

1. **Keep or remove the loopback workbench.** Keep it, and the control-plane
   architecture's "no development shortcut" is amended to name it and its guard
   rails. Remove it, and working on the console locally needs a Keycloak again,
   as that document says. Either answer should be recorded rather than left to
   whichever document was edited last.
2. **What a created client may mean.** Whether a document with a realm and
   product configuration is an acceptable "client exists" to show operators
   before routing, data placement and a secret boundary are designed — or
   whether creation should wait for them.
3. **Deprovisioning.** What removing an application from a client must undo —
   the realm client, a deployment, data — what confirmation it needs, and
   whether a mistaken assignment should have an exit before that exists.
4. **Private-network clients.** Whether a projected client should follow the
   kind of the client's hosts — `privateNetwork` for `.internal` — which is what
   LucentRoot needs, or whether applications stay public-host only.
5. **Who owns a projected identity client.** Whether a product save may keep
   overwriting hand edits, or a projected client should be marked as the
   product's and refused in identity edits.
6. **Activity.** Keep it in desired state with a bound, move it to a durable
   store beside §24's audit events, or drop it in favour of Git history.
7. **Upgrading clients.** Whether clients ever move to a newer release without
   an operator saving each one, and whether a component's `automatic` policy is
   meant to mean anything for application components.
