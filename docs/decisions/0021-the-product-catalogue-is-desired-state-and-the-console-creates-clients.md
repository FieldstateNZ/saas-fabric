# ADR 0021 — The product catalogue is desired state, and the console creates clients

- **Status:** Proposed. Owed decisions 2 and 3 are disposed and built by
  the [2026-10-11 amendment](#amendment-2026-10-11-a-clients-lifecycle-d01-14-d01-19a);
  owed decision 1 is disposed by the
  [2026-10-12 amendment](#amendment-2026-10-12-the-loopback-workbench-is-retired-d01-9);
  acceptance follows ADR 0023 through its own reviewed change (D01-19d).
- **Date:** 2026-09-16; amended 2026-10-11 and 2026-10-12
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

**An application id is an identity client id, chosen once.** `createApplication`
refuses an id that is not a valid OIDC client id — a Fabric identifier may start
with a digit, and an OIDC client id may not — an id Keycloak already holds in every
realm (`account`, `account-console`, `admin-cli`, `broker`, `realm-management`,
`security-admin-console`), and this deployment's own client ids: the console's,
and the Keycloak adapter's when one is configured. The application id becomes a
client id in every realm it is assigned to (§4), and Keycloak accepts a second
client under a name it already holds, beside the real one. The deployment's own
ids are computed where the deployment is composed. These checks run when an application is created, not when the
catalogue is read, so a catalogue already holding such an id stays readable.

**Some catalogue rules hold on every save and every read.** The settings'
timezone, and a `timezone` field's default, is `UTC` or an `Area/Location` name
with no empty, `.` or `..` segment. A plan's configuration keys follow the
field-key rule. `options` are refused on any field that is not a `choice`. A
client's own timezone is held to the same rule when the client is saved.

**A caller sends commands, never the document.** `POST /api/catalogue` takes one
of `createApplication`, `saveApplication`, `publishApplication`,
`saveDefinition`, `saveSettings` or `saveEnvironment`. Release numbers,
publication times and activity entries are assigned by the server, and no
command can carry one. A release a caller could write would not be immutable; it
would be a draft with a number on it.

**Publishing validates, then freezes.** `publishApplication` refuses a draft with
no plan, a container or chart component with no pinned version, a hostname
template that fails the publication rule in §4, or a draft identical to the
latest release, and otherwise appends the draft as release
`n + 1`. Editing the draft afterwards changes no release.

**The first write says it is the first.** Every write carries `If-Match` with
the revision it read or — only when no catalogue exists yet —
`If-None-Match: *`. A write carrying neither is refused with `428`, and a stale
revision with `409 revision_conflict`, whose message names the catalogue rather
than a client — including when the race is lost between the read and the write.
This is
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

1. refuses the realm the id would become when it is reserved, or when another
   stored client document already declares it — `409 realm_unavailable`, saying
   which of the two it is;
2. reads the catalogue and resolves every requested application against a
   **published** version and a plan inside it, and every configuration value
   against its declared field — an undeclared key is refused, a required one
   enforced, a default filled in, each value checked against its type;
3. builds a `v2` client document: `metadata.name` and `spec.identity.realm` both
   the client id, both required roles, the requested display name and hosts,
   and `spec.product` (§3) carrying a `Client created` activity entry;
4. projects the assigned applications into `spec.identity.clients` (§4);
5. refuses a document that renders past the 900 KiB growth limit —
   `422 document_too_large` (see Consequences);
6. writes it **only if no document has that id**. In Git that is a contents
   write with no expected blob, and the host answers two refusals the adapter
   cannot read on their own: a `409`, ordinarily a lost race on the branch, and
   a `422`, which GitHub returns both for a file that already exists and for a
   genuine validation failure. So the outcome is settled by reading the id back.
   A document there means the id is taken — `409 client_exists`, whichever
   refusal reported it, its own code because a taken id is fixed by choosing
   another. Nothing there after a `409` means the write lost a race and can be
   sent again — `409 revision_conflict`. Nothing there after a `422` means the
   request really was invalid, and it stays a rejection rather than becoming a
   conflict something would retry;
7. marks the client `pending`, answers `201` with an `ETag`, and — where an
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

**Why some realms are refused.** Reconciliation treats every realm a document
names as that client's, and this platform's Keycloak adapter treats a realm that
already exists as a successful create. A client named `master` would therefore
have the next pass write roles and application clients into Keycloak's own realm,
with the operator's own bearer. So creation refuses `master`; the realm operators
sign in against, read from the operator issuer; the Keycloak adapter's admin
realm, when one is configured; and any realm another stored client document
already declares. The answer is `409 realm_unavailable`, naming the realm and
which of the two reasons applies. An earlier version withheld the reason, on the
grounds that "already taken" confirms another client's realm exists — but
`GET /api/clients` already shows every client's realm to every operator, so the
omission protected nothing and only made the message less useful. The reserved
names are compared as case-folded strings and never parsed as realm identifiers:
a deployment whose operator issuer or admin realm is `Fabric` or `saas_fabric` —
names Keycloak accepts and this model's own realm type would refuse — must not be
unable to start because of this check. Its limit is what it can see: a realm made in
Keycloak by hand, which no client document declares, is not refused.

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

**A copy, not a reference, and the copy is kept.** While an assignment's
application and version are unchanged, a save keeps the client's stored release
and resolves the plan and configuration against it; only a new application or a
changed version reads the catalogue's release as it is now. So a client keeps
exactly the definition it was assigned: a hand edit to that release in the
catalogue does not reach it, and removing the release from the catalogue does not
stop the client being saved. That is what "an assignment keeps its exact version"
has to mean if it is to survive the catalogue. Every save still reads the
catalogue, for the client fields and any new assignment, so a catalogue that
cannot be read still refuses one. The cost is duplication: every assigned client carries a whole
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

**A client document has two size ceilings.** A creation or a product save is
refused when the document it would write passes 900 KiB; an identity edit is
allowed up to 960 KiB, so a document already at the growth ceiling can still
have a compromised callback or role removed. Both sit under what the contents API
will read a file back at. See Consequences.

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

**The template is checked when it is published, not when it is assigned.** A
draft's template may hold at most one `{client}`. A release that has a template
must hold exactly one, and publishing substitutes a worst-case client id and
refuses the template unless the callback it produces is one `claimedHttps`
admits. That id is sized to the DNS label `{client}` actually shares: 63
characters — the longest a client id can be — for a bare
`{client}.example.com`, and 56 for `{client}-portal.example.com`, where the
affix takes the rest of the label. So an affixed template publishes, instead of
being refused on behalf of every client. A template under `.internal` or
`.example.test` is refused once, at publication, rather than published and then
refused by every assignment. Two things are left to assignment: a client id too
long for its template's label, and the client's own hosts, which remain the limit
under Consequences.

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
*Amended by [ADR 0028](0028-the-activity-feed-is-a-bounded-view-and-the-audit-trail-is-a-separate-durable-source.md)
(2026-10-12, D01-19c): each event is also appended to a hash-chained,
append-only audit trail behind a port, disabled until storage is activated,
and `GET /api/activity` returns a bounded view of the newest 200 actions.*

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

> **Amendment (2026-10-12, D01-9).** The loopback workbench below is retired;
> see [the amendment](#amendment-2026-10-12-the-loopback-workbench-is-retired-d01-9).
> `LocalClientRepository` and `local_directory` are unchanged.

**`local_directory` persists.** It now opens `LocalClientRepository`, in
`fabric-control-plane-api`, instead of holding documents in memory:

- opened with no snapshot, it imports the directory's top-level `*.yaml` files,
  and no subdirectories;
- the first write creates `.fabric-state.json` in that directory, and from then
  on the snapshot is the authority and the YAML is not read again;
- every open parses every stored client and the catalogue, and refuses to start
  on one that does not parse;
- a snapshot whose catalogue carries neither `apiVersion` nor `kind` is refused
  at open as predating the envelope, naming the file; one carrying a pair this
  build does not recognise is refused as an invalid catalogue instead, saying
  what it found. There is no legacy read path: the file is removed, or its
  catalogue wrapped in the envelope by hand;
- an open failure is typed — a held lock, an I/O error, an invalid snapshot, a
  pre-envelope or unreadable catalogue, an unreadable client — and a stored
  document that will not parse is reported as invalid, not unavailable;
- a write serialises the whole snapshot to `.fabric-state.next`, `fsync`s it and
  renames it over the snapshot, in a task of its own, so it finishes — on disk
  and in memory — even if the request that started it goes away;
- `.fabric-state.lock` holds an OS lock, and a second process opening the same
  directory is refused;
- it applies the same conditional create, update and catalogue compare-and-swap
  as the other repositories, with revisions `local-<n>`.

The in-memory repository lost everything on restart, which made a catalogue with
published releases impossible to work with for longer than one session.
`InMemoryClientRepository` remains for tests, and renders and parses every client
document and the catalogue on every write and read as the durable stores do, so a
test cannot pass against desired state no real store could hold. No deployment
mode selects it.

**The workbench.** `cargo run -p fabric-control-plane-api --example
console_workbench` starts the real router over a `LocalClientRepository` in
`.local/workbench`, or in `FABRIC_WORKBENCH_DATA`, and `npm run preview:ui`
serves the console against it.

| Guard rail | Where it is |
|---|---|
| The API binds `127.0.0.1:8082`, and nothing else | a literal in the example |
| A request whose `Host` is not `127.0.0.1:8082` or `localhost:8082` is refused with `421`. A loopback bind keeps out the network, not a page in a local browser that points a name it controls at `127.0.0.1` | the example |
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
than a feature gate. The workbench also reserves no names: `master` and the
platform's own client ids are refused only where a deployment is composed, so it
refuses just a realm another stored client declares and Keycloak's six built-in
ids.

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

**Activity and releases are bounded by refusal, not by trimming.** Nothing
trims either list. Every catalogue command rewrites the whole of
`fabric-catalogue.yaml` — every release snapshot and every activity entry — and
every product save or identity edit rewrites the client's document with its
release copies and its activity. A write is refused with
`422 document_too_large` — `422` and not `413`, because the request body is small
and it is the document the write would produce that is too big — once that
document passes its limit. There are two, both in `document_size`:
**`MAX_DOCUMENT_BYTES`, 900 KiB, for a write that grows a document** — a
creation, a product save or a catalogue command — and
**`MAX_REMEDIATION_DOCUMENT_BYTES`, 960 KiB, for an identity edit**. Both sit
under what the contents API will read a file back at, so a document either check
accepts is one this platform can still read.

The two limits exist so that growth stops first. An identity edit is how a
compromised redirect URI or role is removed, and that edit *shrinks* the document
— checking it against the ceiling growth has already reached would block
remediation on exactly the document that needs it. So a client document at the
growth limit still takes identity edits, with 100 KiB of margin to make them in;
once even that margin is gone, the document has to be trimmed in Git before
anything can be written to it at all.

**An identity edit can spend that margin, and that is accepted.** An identity
edit is checked only against `MAX_REMEDIATION_DOCUMENT_BYTES`, and it grows the
document as well: an activity entry every time, and a body of up to 64 KiB
(`MAX_BODY_BYTES`). So a document already at the growth limit can be walked up
towards the remediation ceiling by repeated identity edits — one large one, or
a few hundred small ones. It can never pass that ceiling, so the
document stays readable; what runs out is the room to write it at all, and then
it has to be trimmed in Git before any path can write it again. The alternative
is checking an identity edit against the limit growth has already reached, which
would refuse the shrinking edit this margin exists for.

Every catalogue command appends activity and none removes anything, so **once the
catalogue reaches its limit, every catalogue command is refused** — publishing,
saving a draft, changing settings — until someone trims the file by hand in Git.
The only things there are to trim are activity entries and releases, and releases
were meant to be immutable. A client already assigned a trimmed release keeps its
own copy (§3); a new assignment of that version can no longer be made. A client's
own document reaches the growth limit the same way: enough assignments, each a
whole release, and enough activity.

In Git each of these writes is a commit whose `Requested-by:` trailer already
names the operator, so the activity lists are a second copy of attribution kept
for the console's convenience. They are also desired state that anyone with write access to the
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
assignment for that client. (An application whose template resolves under
`.internal` is now refused when it is published, §4.) LucentRoot's production
hosts are `.internal`, so its clients cannot take an application today.

**Tightening a validation rule later can make the catalogue unreadable.** Every
read parses and re-validates every stored release with the publication rules.
Remove a capability name, or narrow the route or hostname-template rule, and a
release published under the old rule makes the whole document unreadable — the
catalogue page, client creation, every product save and the activity listing
all read it — answered `500 desired_state_invalid` until the file is corrected
by hand, which means editing a release that was meant to be immutable. Client
documents' copies are not re-validated, so existing clients stay readable.

This pull request has already done it once. A release template with no
`{client}` or one resolving under `.internal` or `.example.test`, a draft template
with two, `options` on a field that is not a `choice`, a plan configuration key
outside the field-key rule, and a settings timezone that is not `UTC` or an
`Area/Location` name were all accepted by earlier revisions of it and are refused
on read now. A catalogue written by those revisions can be unreadable, and a local
store holding one refuses to open.

**Creation reads every client first.** The realm check lists every stored client
on each creation, so one client document that will not parse refuses every
creation with `500 desired_state_invalid`, as it already fails the client
listing.

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
*Designed in [ADR 0028](0028-the-activity-feed-is-a-bounded-view-and-the-audit-trail-is-a-separate-durable-source.md)
(D01-19c): the port, the hash chain, the entry cap and a local development
adapter are built; storage activation and retention are still not decided.*

**Replicas.** Every write here is a conditional write the Git host checks
atomically, so none of it rests on process-local coordination. The local
repository is single-process by its lock, and is a development mode only.

## Decisions owed to the product owner

1. **Keep or remove the loopback workbench.** *Disposed 2026-10-12 (D01-9):
   removed; see the amendment below.* Keep it, and the control-plane
   architecture's "no development shortcut" is amended to name it and its guard
   rails. Remove it, and working on the console locally needs a Keycloak again,
   as that document says. Either answer should be recorded rather than left to
   whichever document was edited last.
2. **What a created client may mean.** *Disposed 2026-10-11 (D01-19a): an
   explicit incomplete state; see the amendment below.* Whether a document with a realm and
   product configuration is an acceptable "client exists" to show operators
   before routing, data placement and a secret boundary are designed — or
   whether creation should wait for them.
3. **Deprovisioning.** *Disposed 2026-10-11 for a whole client (D01-14):
   tombstone first, retained state, confirmed deletion as a separate
   operation; see the amendment below. Removing one application from a client
   is still refused and still owed.* What removing an application from a client must undo —
   the realm client, a deployment, data — what confirmation it needs, and
   whether a mistaken assignment should have an exit before that exists.
   Changing an assignment's plan, or moving it to an older version, is allowed
   while removal is refused — yet a plan granting fewer features, or a release
   with fewer components, drops components from what the client is entitled to.
   That is the same deprovisioning question, asked more quietly.
4. **Private-network clients.** Whether a projected client should follow the
   kind of the client's hosts — `privateNetwork` for `.internal` — which is what
   LucentRoot needs, or whether applications stay public-host only.
5. **Who owns a projected identity client.** Whether a product save may keep
   overwriting hand edits, or a projected client should be marked as the
   product's and refused in identity edits.
6. **Activity, and what the size limits leave.** *Disposed 2026-10-11
   (D01-19c): activity is a bounded view and a separate durable audit source
   is the trail; see [ADR 0028](0028-the-activity-feed-is-a-bounded-view-and-the-audit-trail-is-a-separate-durable-source.md).
   Trimming the stored feed waits on storage activation; what an operator
   does when releases fill the catalogue is still owed.* Activity is bounded today only
   by `422 document_too_large`, and by the margin an identity edit keeps above
   it. Keep it in desired state with a trimming rule, move it to a durable store
   beside §24's audit events, or drop it in favour of Git history — and decide
   what an operator does when a catalogue full of releases reaches the limit.
7. **Upgrading clients.** Whether clients ever move to a newer release without
   an operator saving each one, and whether a component's `automatic` policy is
   meant to mean anything for application components.

**Extended by [ADR 0026](0026-a-component-describes-itself-in-an-artifact-attached-to-its-image.md) (2026-09-29).** An application component gains
a fourth kind, `described`, selected from a registry and resolved by the
server, and the fields and resources its component descriptor declares are in
effect beside authored ones. A catalogue holding one is
`fabric.fieldstate.nz/v2`. The configuration-field and application-resource
rules may from now on only relax, which answers, for those shapes, part of
"How catalogue validation rules may change". Saving a draft takes a request
shape of its own that cannot carry what the server resolved.

## Amendment 2026-10-11: a client's lifecycle (D01-14, D01-19a)

Recorded from the D01-14 and D01-19a dispositions in
[#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415):

> **D01-14:** Tombstone first, explicit retained state, confirmed deletion as
> a separate operation. No destructive action or retention period is
> approved by this choice.
>
> **D01-19a:** Explicit incomplete state until provisioning/identity/publication
> readiness is met; distinguish saved intent from usable client.

The follow-up answers in
[#108 comment 6113749198](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6113749198)
bound it further: retain state with no automatic deletion; record intent in
separate revision-fenced writes; no live identity, session or credential
change; and no claim that a tenant stopped being served without withdrawal
evidence.

This amends owed decisions 2 and 3 for a whole client. It does not accept
this ADR.

### The states

```text
 create ──► incomplete ◄──evidence──► ready
                │                       │
                └──── tombstone ────────┘
                          ▼
                     tombstoned            retirement recorded; state retained; still published
                          │ confirm deletion, a separate write
                          ▼
                  deletionConfirmed        intent only; nothing is deleted
```

- **`incomplete`** is saved intent: the document exists and the client is not
  usable. **`ready`** means all three readiness conditions are met. Both are
  derived on every read, never stored. An operator does not declare a client
  usable; the platform shows it.
  - *Provisioning:* every logical data source in `spec.data` has a placement
    record, and there is at least one.
  - *Identity:* reconciliation is `applied` (or `drifted`, which was
    corrected) at the client's current revision.
  - *Publication:* the tenant is in the tenants document the last successful
    publication pass left, at the revision its placements add up to
    (ADR 0023 part 4, D1).

  Missing evidence never counts as met. A deployment that manages no
  platform, an unreadable placements document, and a process that has not
  completed a publication pass since it started all leave the condition
  unmet and say why. A ready client whose evidence regresses goes back to
  `incomplete`.
- **`tombstoned`** and **`deletionConfirmed`** are recorded in the client
  document under `spec.lifecycle`, because an operator decided them
  (ADR 0008). A recorded tombstone wins over any evidence. Both are intent:
  the console labels a tombstone "Retirement recorded", never "withdrawn".

### The rules

| Transition or use | Rule | Refusal (status, code) |
|---|---|---|
| Tombstone | Only from `incomplete` or `ready`, at the current revision, with a reason | `409 client_already_tombstoned`, `409 deletion_already_confirmed`, `422 invalid_tombstone_reason`, `409 revision_conflict` |
| Confirm deletion | Only from `tombstoned`, as its own write at the revision the tombstone produced, naming the client again in the body | `409 deletion_requires_tombstone`, `409 deletion_already_confirmed`, `422 deletion_confirmation_mismatch`, `409 revision_conflict` |
| Leave `tombstoned` or `deletionConfirmed` | No transition exists | — |
| Edit product configuration | Refused when retired | `409 client_retired` |
| Remove a secret | Refused when retired: it would not be retained | `409 client_retired` |
| Place a data source | Refused when retired | `409 client_retired` |
| Publish the tenant | Not governed by the lifecycle. A retired tenant stays in every pass exactly as it was, until a withdrawal exists (below) | — |
| Route the client | Refused unless `ready` | `client_not_ready` (domain only; see below) |

Saved intent may be edited, placed and published, because each is a step
towards readiness, and publication is itself a readiness condition.

### Withdrawal is deferred

Recording a tombstone does not withdraw the tenant from the runtime
documents. The publisher composes a retired tenant's placements exactly as
before: its binding, revision and resource history are unchanged, so every
pass offers the same tenants document and the publication history guards
(ADR 0018; the removed-binding and held-tenant guards in #138) have nothing
to refuse. Other tenants' publication proceeds, and the retained-last-good
and emptying guards stay as they are.

An earlier draft of this change filtered retired tenants out before
composition. That removed a published binding without bumping anything,
which #138's guards correctly refuse, so a multi-tenant environment would
have stalled with the retired binding still published, and a sole tenant
was refused by the emptying guard. Withdrawal needs its own reviewed
operation: a revision-bumping, explicit retirement those guards can admit,
with deterministic two-tenant and final-tenant proofs, restart and
held-record tests, and reader-applied evidence before anything claims a
tenant stopped being served.

Until then every retired client's response carries `lifecycle.withdrawal`,
derived only from what this process observed of publication:

| `withdrawal.state` | Evidence |
|---|---|
| `pending` | The tenants document the publication target last accepted still carries the tenant |
| `refused` | The last pass was refused or failed, so the runtime keeps the documents it last accepted |
| `unobserved` | No publication target, or no pass has published since the control plane started |
| `notPublished` | The accepted tenants document does not carry the tenant (it was never placed or published). No reader is observed applying it |

There is no `withdrawn` or `applied` state. The control plane holds
publisher-side evidence only, and Riley's answer requires reader-applied
evidence before withdrawal is called complete.

Two edits stay open on a retired client on purpose: an **identity edit** and
a **secret write**. A tombstone does not revoke the client's identity, so
removing a compromised redirect URI or role, or rotating a leaked secret,
must still be possible.

### What a tombstone and a confirmed deletion do not do

Nothing is removed. The document, its `spec.data`, every placement record,
the realm, its projected clients and the secrets all stay. Identity
reconciliation continues to add and correct, exactly as for any client.
Confirming deletion writes `spec.lifecycle.deletionConfirmed` and an
activity entry. It applies no retention period and starts no deletion.

### Verified against the code (at the base of this amendment)

| Claim | Where |
|---|---|
| The state machine and every refusal | `crates/fabric-client-model/src/lifecycle.rs`, `lifecycle/record.rs`; `lifecycle/lifecycle_tests.rs` |
| `spec.lifecycle` round-trips, and a hand edit confirming deletion without a tombstone, before it, or under a misspelt key is refused on read | `document/lifecycle.rs`; `document/document_lifecycle_tests.rs` |
| Readiness evidence, and missing evidence is never readiness | `crates/fabric-control-plane/src/lifecycle/evidence.rs`; `lifecycle/evidence_tests.rs` |
| The API shows the state on every client response, and each write and refusal through the router | `handlers/tombstone_client.rs`, `handlers/confirm_client_deletion.rs`; `tests/client_lifecycle.rs` |
| Retirement leaves every tenant published, the tenants document byte-identical and placements retained, so publication of other tenants proceeds | `client_lifecycle.rs::a_client_becomes_ready_once_placed_applied_and_published_and_retirement_leaves_every_tenant_published` |
| Retiring the only published tenant neither empties nor stalls publication (`unchanged`) | `client_lifecycle.rs::retiring_the_only_published_tenant_neither_empties_nor_stalls_publication` |
| Withdrawal is reported `pending`, `refused`, `unobserved` or `notPublished` only from publication evidence | `crates/fabric-control-plane/src/lifecycle/withdrawal.rs`; `lifecycle/withdrawal_tests.rs`; `client_lifecycle.rs::a_refused_pass_is_reported_as_refused_and_the_last_good_tenant_stays`, `::without_publication_retirement_reports_withdrawal_as_unobserved` |
| The console tells saved intent from a usable client, labels a tombstone "Retirement recorded", and never claims a tenant stopped being served | `apps/control-plane-ui/src/console/LifecycleStatus.tsx`; `LifecycleStatus.test.tsx`, `console.test.tsx` |

### Known limits

- **A retired client is still served.** Its tenant stays published, and a
  tombstone revokes no identity, session or credential. Nothing in this
  change limits runtime access; see "Withdrawal is deferred".
- **Identity and publication evidence is per process.** Both come from
  in-memory state. After a restart a client shows `incomplete` until a
  reconciliation pass and a publication pass have run. Nothing gates
  publication on identity for the same reason: withdrawing a tenant after
  every restart would be an outage. Withdrawal evidence is per process for
  the same reason: after a restart it is `unobserved` until a pass
  publishes.
- **Routing has no producer in Fabric.** The rule refusing to route a client
  that is not `ready` is in the domain (`ClientLifecycle::permits`). The
  issuer-to-tenant registry and edge generator (D01-4) must consult it when
  built.
- **An older Fabric ignores `spec.lifecycle`.** A control plane from before
  this amendment reads a tombstoned client as active and would let it be
  edited and placed. Publication is unaffected, because this change does not
  withhold a retired tenant either. The release rollback floor (D01-21) should sit at or
  above the release that carries this. It does: `compatibility.yaml` sets it
  at `0.3.0-preview.16`, the first release that can carry this amendment,
  and Platform Management refuses an in-product rollback below it. A
  break-glass edit of the platform repository is not refused.

### Deferred, and how each is bounded

Answered for engineering in
[#108 comment 6113749198](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6113749198);
the user-only parts stay open.

- **Retention:** no timer and no automatic deletion. State is retained until
  a separately approved deletion. No duration, expiry or export obligation
  is set. *Open, user-only:* the legal and customer retention duration and
  export or evidence obligations.
- **Eventual deletion:** confirmation stays recorded intent. Before any
  destructive executor exists, a reviewable inventory, order, backup,
  export and recovery plan must name exactly what would be deleted and who
  executes it. No realm, user, secret, database or volume deletion job
  exists. *Open, user-only:* deletion scope and its execution.
- **Restoring a tombstone:** to be a separate, bounded, reviewed slice: an
  explicit operator write with current-revision CAS, audit and
  retained-placement checks, never lowering document or resource history
  revisions and never regenerating retained secrets. No automatic restore.
  A deletion-confirmed client needs an explicit disposition before it can be
  restored. Not built here: there is no route back.
- **Identity at tombstone:** a tombstone disables no sign-in, revokes no
  token or session, removes no projected client and rotates no credential.
  The remediation paths that stay open (identity edits, secret writes) are
  as designed and are no permission for a live change. *Open, user-only:*
  any live identity or session change.
- **Who confirms deletion:** two-step, client-named, revision-fenced and
  audited, under the existing operator authorization. No second-operator
  role, grant or waiting period.
- **Withdrawal:** a separately reviewed publisher and reader retirement
  slice, as described under "Withdrawal is deferred". No activation is
  authorized.
- **Removing one application from a client** (owed decision 3's first half)
  is still refused.

## Amendment (2026-10-12): the loopback workbench is retired (D01-9)

**Disposition.** [#108](https://github.com/FieldstateNZ/saas-fabric/issues/108#issuecomment-6109518415),
D01-9: retire the workbench and `/api/session` only after the disposable
dogfood test and runbook have moved to the gateway flow or been explicitly
parked, keeping local automated tests possible.

**What was removed.** `fabric-control-plane-api/examples/console_workbench.rs`;
the console's `preview/` entry, `preview.html` and `npm run preview:ui`. §7's
guard-rail table described code that no longer exists. The control-plane
architecture's "there is no development shortcut … local development therefore
needs a Keycloak" stands again without an exception, and this record's
contradiction table row for it is resolved in that document's favour.

**What stays.** `LocalClientRepository` (`local_directory`) and
`LocalAuditTrail`, which a deployment's own configuration selects and which
have their own integration tests. `testing::AcceptingOperator`, which every
control-plane integration test drives; with the workbench gone, only tests
populate it.

**Local automated tests.** Unchanged in reach: `cargo test --workspace` drives
the real router with `AcceptingOperator`, and `npm test` drives the console
with the network stubbed. Interactive work against a live API needs the
gateway in front of the console (ADR 0024 slice 1's local half), which is not
built yet.

**The disposable dogfood profile** was built around `/api/session`. It is
parked rather than moved; why, and what that keeps running, is in
`examples/disposable-dogfood/README.md`, "Parked (D01-9)".

