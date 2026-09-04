# The control plane

- **Status:** Implemented (identity only)
- **Related:** [ADR 0008](../decisions/0008-desired-state-is-the-authority.md),
  [ADR 0009](../decisions/0009-operator-identity-is-not-tenant-identity.md),
  the platform specification §4–§6 and §30

The platform specification describes two planes. The runtime plane — tenant
identity, the tenant registry, the Data API — is described in
[tenant-runtime-data-api.md](tenant-runtime-data-api.md) and was built first.
This document describes the other one.

## The principle

> **Operators express desired SaaS state through SaaS Fabric. Reconciliation
> makes platform services conform to it. Applications consume the resulting
> runtime state without depending on the control plane.**

And its corollary, which is what most of the design follows from:

> **SaaS Fabric manages platform concepts. Shared platform services implement
> them.**

| SaaS Fabric concept | Implementation |
|---|---|
| Client Identity | Keycloak realm |
| Authorization | OpenFGA |
| Secrets | OpenBao |
| Observability | Grafana |
| Routing / domains | Envoy |

This increment implements **Identity** only. The others are named here so the
shape is visible, not because anything reconciles them yet.

## The flow

```text
Human operator
      │
      ▼
SaaS Fabric UI                    apps/control-plane-ui
      │
      ▼
Control Plane API                 fabric-control-plane
      │
      ▼
Client desired state              fabric-client-model
      │
      ▼
saas-fabric-clients               fabric-client-git
      │
      ▼
Reconciliation                    fabric-reconciliation
      │
      ▼
Keycloak Admin API                fabric-keycloak
      │
      ▼
Client realm
```

The UI never administers Keycloak. Keycloak remains an implementation detail
behind SaaS Fabric.

## Why the two planes are separate processes

They are separate images, separate deployments, and separate dependency graphs
sharing only `fabric-core`.

- **Different networks.** The runtime API is on the product edge, reachable by
  every tenant's application. The control-plane API is on the operator plane and
  must not be reachable from the product edge at all.
- **Different failure modes.** A control plane that cannot reach Git is broken.
  A runtime plane in the same situation has not noticed, because it never reads
  Git (§6). Sharing a process couples their availability, which is exactly what
  §6 forbids.
- **Different identities.** A tenant, and a platform operator. See ADR 0009.

`scripts/check_architecture.py` enforces the separation: no crate in one plane
may depend on a crate in the other.

## Control Plane API

Operator-facing HTTP, in `fabric-control-plane`.

```text
GET    /api/session                      where to sign in       (no operator)
POST   /api/session                      redeem a code          (no operator)
GET    /api/integrations/git             can desired state be read?
POST   /api/integrations/git/connect     describe the app to create
GET    /api/integrations/git/created     host callback          (no operator)
GET    /api/integrations/git/install     where to install it
GET    /api/integrations/git/installed   host callback          (no operator)
GET    /api/integrations/git/repositories  what the install reaches
PUT    /api/integrations/git/repository  choose one
DELETE /api/integrations/git             forget the integration
GET    /api/integrations/platform        has an application been made?
POST   /api/integrations/platform/connect    describe the app to create
GET    /api/integrations/platform/created    host callback      (no operator)
GET    /api/integrations/platform/install    where to install it
GET    /api/integrations/platform/installed  host callback      (no operator)
GET    /api/integrations/platform/repositories  what the install reaches
PUT    /api/integrations/platform/repository    choose one
DELETE /api/integrations/platform        forget the integration
GET /api/clients                       list clients
GET /api/clients/{clientId}            one client's overview
GET /api/clients/{clientId}/identity   its identity, and reconciliation state
PUT /api/clients/{clientId}/identity   replace its identity  (If-Match required)
```

Three things it does not do, each of them a rule rather than a gap:

1. **It does not call a platform service.** There is no code path from a handler
   to Keycloak. Router state holds the domain service and the operator
   authenticator, and the absence of anything else is the design.
2. **It does not expose repository internals.** No path, no branch, no file, no
   YAML (§8). An operator is told "the client changed since you read it", never
   "the blob sha of `clients/acme/client.yaml` moved".
3. **It does not report a write as applied.** A successful `PUT` answers
   `pending`. Writing the document and converging the provider are different
   events that fail independently.

### Desired state is late-bound

`mode = "managed"` is the production mode: **the control plane starts without
knowing where client desired state lives.** It runs, serves, reports itself
unconfigured, and an operator connects a repository through the console.

That is a change of posture, not a relaxation. The alternative was a deployment
holding a Git host's identifiers and a credential that a human had to create by
hand *before the platform could start at all*, which made the platform's own
onboarding somebody else's problem — and left the console, the one tool for
fixing it, unable to start for exactly the reason it was needed.

"Not configured" is a `ClientRepository` implementation rather than an
`Option` threaded through the service, the loop and every handler. Every caller
has one code path, and the state arrives as an ordinary `RepositoryError` that
is impossible to forget to handle.

| Mode | Where desired state comes from | Bad configuration |
|---|---|---|
| `managed` | connected by an operator, in the product | n/a — there is none to be wrong |
| `git` | stated by the deployment | **fatal at startup** |
| `local_directory` | a directory, held in memory | fatal at startup; development only |

A deployment that *states* a repository has opted out of the managed path, so
stating it wrongly still fails at startup. Silently starting unconfigured would
hide the mistake behind a screen inviting somebody to connect a repository the
deployment had already named.

### Connecting the integration

An operator establishes it from inside the product; nothing about a Git host is
in a deployment. The flow is two approvals on the host, because that is what
the host requires, and the console shows whichever is outstanding rather than
pretending it is one action that sometimes half-works.

```text
operator names the organisation
        ↓
browser POSTs a manifest to the host        ← a real form; a manifest is a POST
        ↓
host returns with a one-time code
        ↓
platform redeems it → private key → secret partition, then the record
        ↓
operator installs the application
        ↓
host returns with an installation id
        ↓
platform mints a token → only then records the installation
        ↓
one repository → adopted;  several → the operator chooses
        ↓
desired state is bound, live, with no restart
```

**Nothing is recorded before it is proven.** The private key is stored *before*
the record, because a record without its key describes an application the
platform can never authenticate as and the key arrives exactly once. An
installation is recorded only after a token has been minted for it, so
"recorded" means "working".

**The two callbacks take no operator.** The host redirects a *browser*, which
carries no bearer token. What they require instead is a correlation token this
platform issued to an authenticated operator moments earlier — random,
server-side, single-use, ten minutes. A captured callback URL is not
replayable, which a signed stateless blob would be for as long as it is valid.
An in-flight flow does not survive a restart; that means "start again", takes
seconds, and cannot produce a wrong outcome.

**Disconnecting does not uninstall.** It removes the record, the key and the
binding. Deleting an organisation's application from a console button would be
doing more than the button said.

### Where the platform keeps its own state

Two ports, one backing service, and the separation is in the types rather than
the location:

| Port | Holds | May be shown to an operator |
|---|---|---|
| `SecretStore` | the application's private key | never |
| `IntegrationStore` | application id, slug, installation, repository | yes |

`fabric-openbao` implements both and is the only crate that knows OpenBao
exists. It authenticates with the pod's own Kubernetes identity, so there is
still no credential for a human to create or transport — which is the whole
point, since secrets projected *into* a pod are a one-way path and the platform
now generates credential material of its own.

### Two integrations, two applications

The same connection flow serves two unrelated purposes: client configuration,
and the platform's own composition. They are **separately installable,
configurable and removable** — two GitHub Apps, two installations, two records,
two sets of routes. Connecting one does not connect the other, and disconnecting
either leaves the other exactly as it was.

One application holding both permissions would be smaller, and would mean an
operator who wanted to manage clients had to grant write access to the platform
repository as well.

Which integration a request acts on is decided by **the route it was sent to**,
never by anything in the request. There is no `/api/integrations/{kind}` and no
integration name in a body: a caller who could name an integration is one
refactor away from naming a third that does not exist, which is the shape §15
forbids.

The handlers are written once and mounted twice, over a `Flow` type that
supplies the service and the console redirect key. The two differ in exactly
three places — the application's name on the host, its callback path, and where
its record and private key are stored.

`GET /api/integrations/platform` reports the *application's* lifecycle: created,
installed, repository chosen. It deliberately does not report whether the
platform repository can be read — that is `GET /api/platform`'s answer, from the
binding this integration connects, and two routes reporting one fact is one
route away from them disagreeing.

### Stopping an environment, and letting it go again

Automatic promotion without an in-product pause is an incomplete operator
experience. Git remains the break-glass path and is expected to keep working;
the console is the normal one.

```text
PUT    /api/platform/components/{component}/hold    stop it advancing
DELETE /api/platform/components/{component}/hold    let it advance again
```

**Pause and rollback are different acts** even though both write a hold. Pause
keeps the desired version and adds a hold; rollback changes the version *and*
adds a hold, in one commit. Keeping them apart is what lets an operator stop
advancement before testing a preview without also moving what runs.

Neither is a policy change. `update` stays `automatic`, and the effective state
reads `Automatic — Paused`: the operator said "not for now", not "not ever",
and the manifest must not record the second.

Resuming lifts the hold and **does not advance**. What happens next is the next
sweep's to decide from what it observes then, so nothing here reports a version
it has not moved to.

#### Two artifact kinds, and what rollback means for each

A component is published either as **container images** or as a **Helm chart**,
and the two are discovered differently and guarantee different things. They are
not one shape with fields left empty:

| | images | chart |
|---|---|---|
| discovered from | a registry — tags, manifests, config blobs | a chart repository's `index.yaml` |
| eligibility | every image carries the version and agrees on its source commit | the version is published |
| what deploys | an immutable digest | a version |
| rollback | offered — restores the version *and* the exact bytes | offered — restores the version, not provably the bytes |

**Rollback means restoring an older published version of the component.** That
is the definition, and it is offered for every artifact kind. It is not a
history: the platform keeps no record of what an environment selected or ran,
and offers what the registry or chart repository publishes *now* below the
desired version, in its channel and — for a prerelease — its line. For images
the restored version comes with the exact bytes, because a release unit carries
every digest. For a chart it is the chart *version*: a classic chart repository
pins a version rather than a digest, so the bytes behind `7.3.0` may have been
republished since, and what comes back is provably the version and not provably
the bytes it once represented.

**That difference is stated, not enforced.** The console says it in one line
beside the candidates — "Restores the chart version. A chart repository can
republish the bytes behind a version, so this is not the byte-for-byte return
an image rollback is." — and this document says it in the table above. Neither
half of the platform refuses on the strength of it.

**The alternative definition was considered and not taken.** It is: rollback
requires immutable artifact identity, so a component without one has no
rollback — `roll_back` takes a `ReleaseUnit`, the API answers
`501 rollback_unsupported`, and the console omits the button. That is coherent,
and it is what this platform did until this decision. What settled it against
that reading is the operator: someone whose chart upgrade broke login wants the
version they were on back, and telling them the platform will not do it because
of a guarantee they were never promised leaves them hand-editing the platform
repository — which is the break-glass path, not an operator experience. A
weaker guarantee an operator is told about beats a capability they do not have.

If chart lifecycle is later modelled — an OCI chart registry, or a digest
recorded at the moment of deployment — a chart rollback gains the byte
guarantee and the caveat line goes away. The operation does not change.

The shape is in the signatures rather than in a check: `advance` and
`roll_back` both take a `Release`, which is either kind, and both go through
the same identity check, which refuses a release shaped for the *other* kind
before any file is read. What neither can express is moving one image or
supplying a digest.

Pause and resume are offered for both, and always were. Stopping an environment
advancing needs no artifact guarantee at all.

#### A chart repository is read over HTTPS, end to end

The index a chart repository serves names the version that gets pinned into
what Argo deploys, so a byte rewritten on the way from the repository is a byte
that steers a rollout. The chart reader refuses a repository URL that is not
`https://`, and refuses to follow a redirect to anything else. An HTTP client's
default policy follows a `30x` wherever it points, including back down to plain
HTTP, which would make the first hop's TLS a formality; here every hop is HTTPS
or the read is refused. A refused redirect is reported as a refusal rather than
an outage, because retrying it changes nothing.

The reader also trusts only the chart it was asked about. A repository serves
every chart it holds in one document, and an unrelated chart's malformed entry
must not make this component undiscoverable — nor may an unrelated entry's YAML
aliases turn a bounded download into an unbounded allocation. So the requested
chart's entries are the only ones read into a shape; everything else in the
index is skipped without being materialised.

#### A decision is applied to the state it was taken against

Every write presents the revision its decision was read at, and desired state
that moved in between is a conflict rather than an overwrite.

Without it, a sweep reads, decides, and writes — and an operator who adds a
hold between the read and the write watches it be ignored, because the write
re-reads and applies a decision taken about something else. The decision was
right when it was taken and wrong by the time it landed.

The selector's own docs already claimed this. They were describing the
intended design, not the implemented one: the precondition was the revision the
*adapter* had just read for itself, which proves only that nothing changed
during the write.

The state a decision is taken against includes **which repository was bound**,
so a disconnect or a rebind between the read and the write is a conflict too —
and a disconnect completes only once the operations already in flight against
the old repository have finished, so nothing this platform reports as done was
done to a repository an operator had already stopped targeting.

#### The series only means something for a prerelease

An automatic policy walks forward within the desired version's own line, and a
line is a `major.minor.patch` core. That is right for a prerelease —
`0.3.0-preview.9` and `preview.10` are the same line, `0.4.0-preview.1` is a
different one — and wrong for anything else: **every stable advance changes the
core**, so applying the rule to a stable component meant it could never advance
and would report "nothing newer" however much its repository published.

What should bound a stable advance instead is **not settled**, so the
combination fails closed: a stable component on `automatic` advances nothing
and reports `UndefinedStablePolicy`. It still shows what is newer, so the
decision that is owed stays visible rather than looking like an idle component.

Patch and minor upgrades are ordinary; a major is not something to take on a
sweep. Until that is chosen, a stable component that should move is `manual`,
where a person chooses.

#### Rolling back

```text
GET  /api/platform/components/{component}/versions    what it could go back to
POST /api/platform/components/{component}/rollback    put it back on one
```

An operator names **a version and nothing else**. For an image component every
candidate the listing offers is one Fabric resolved from the registry to a
complete, coherent release unit — three images that exist and agree about the
commit they were built from. A version that never was one is not offered and is
refused if asked for (`422 version_not_rollable`), because rolling back to it
would deploy a composition nobody ever ran.

**A chart's candidates come from the index**, and carry no source revision. A
chart repository lists versions and no provenance, so there is no commit to
name: the API omits `source_revision` for those rows rather than sending an
empty one, and the console lays the row out without the line rather than
rendering "built from" about something nothing observed. A version the
repository no longer lists is refused the same way an image version that was
never a release unit is — `422 version_not_rollable`, decided against the index
on this request.

What gets written is resolved at the moment of the write, **with the hold**, in
one commit: for images the version, its source commit and its three digests;
for a chart the version, together with the repository and chart name it was
discovered under, so a number that is plausible against the wrong chart is
still refused. There is no request shape carrying a digest, so "roll back to
whatever Git used to say" is not expressible.

The hold is not optional. An environment moved backwards under a live automatic
policy would be advanced forward again by the next sweep, and the operator
would watch their rollback disappear. It records `reason: rollback` rather than
`paused` so a later reader can tell which act stopped the environment, and the
policy stays `automatic` — this is "put me here and stay until I say
otherwise", not a decision to stop advancing forever.

The candidate search is bounded at five, and **the bound is reported**:
`more: true` says older versions exist that were not examined. A list that
stopped quietly would read as "this is everything there is".

The bound is about latency, not taste. Proving a version was a whole release
means a manifest and a config blob per image, fetched sequentially — around
three seconds a version against GHCR — and the listing has to fit inside one
operator request. Raising it needs concurrency first, not a bigger number.

A chart listing is bounded by the same five and reports it the same way, and
there the reason is not latency — reading an index costs almost nothing. It is
so an operator meets one shape whichever kind of component they are looking at,
rather than a long list for one and a short one for the other.

The picker is navigation; the rollback operation is validation. They must not
share a hidden "only the first N are legal" rule, which is why the two are
bounded differently and deliberately.

**The version is re-resolved on the rollback request**, not carried over from
the listing the console fetched moments ago — so a version withdrawn between
the two is refused rather than deployed from a stale candidate object. The
request body is `{version, note}` under `deny_unknown_fields`: a body carrying
a digest is **refused, not ignored**. That temptation will look like a
performance fix — the browser already has those values — and a digest a caller
sends is the thing that would actually be deployed.

Rolling back resolves **only the version asked for**, not the whole listing
again. Membership in the offered list was never the property that mattered:
what matters is that the version is in this component's channel and series,
sits strictly below what is desired, and resolves *now* — to a complete
coherent release unit for images, or to an entry the chart repository still
lists. One consequence is deliberate — a version older than the bound is still
rollable if a caller names it, because the bound limits what is *offered* and
it would be a strange safety rule that made a real release unrollable because
five newer ones existed.

**The series only bounds a preview here too**, and for the reason stated above.
Rolling back used to pass the desired version as the series unconditionally,
which is the same latent defect advancement had: every stable release changes
the `major.minor.patch` core, so `7.3.0` counted as a different line from
`7.3.1` and a stable component was offered nowhere to go, whatever its registry
or chart repository held. Both directions now read the rule from one place.

#### The component may be named; the environment still may not

The environment reaches the platform repository as a path segment, which is why
`GET /api/platform` takes no name (§31.7). A **component** name does not:

> A caller may select a component identifier that already exists in the
> environment manifest. Fabric SHALL NOT use that value as a repository path, a
> file path, a registry location, or any other desired-state locator.

The rule is enforced by the read that precedes every write — the name is a key
looked up in a manifest already trusted from Git, and one it does not carry
selects nothing. A component that does not advance on its own cannot be paused
at all: a hold on it would stop nothing and show `Paused` about something that
was never moving.

`advance` remains structurally unable to express a hold. That is what
guarantees an automatic pass cannot clear one in order to succeed, and it is
why these are separate operations rather than an argument to it.

### What the platform panel reports

The specification's model is **Desired / Available / Running**. Two of those
are honest today and one was not, so the name changed rather than the meaning
being stretched to fit it.

| Row | What it is | What it is not |
|---|---|---|
| `Desired` | what the environment is asked to run | — |
| `Newer version` | the newest eligible version **newer than desired**, i.e. what Fabric would advance to | not "the available version" |
| `Running` | what is actually serving; `Unknown` until there is a reconciliation integration to ask | not inferred from Git having changed |

`newer` is `None` whenever nothing sorts after `desired`. Under the label
*Available* that rendered as `—` for an environment running the newest preview
there was, which reads as "nothing is available" about a version that plainly
is. **Nothing in discovery observes whether the desired version is still
published**, so the broader word was a claim the platform could not support.

The tempting fix — "desired exists, so it must still be available" — was
rejected. It is sound right up until an artifact is deleted or a registry is
unreachable, and then the console is confidently wrong about the one thing an
operator is looking at it to learn.

A `Latest available` worth the name arrives with a versions view, where Fabric
enumerates what a registry holds. Widening discovery to compute it now would be
a broader registry scan on every sweep for no new operator capability:
candidates are deliberately only examined *above* the floor, which is what
makes automatic selection unable to move an environment backwards.

`Newer version` and `Desired state` overlap in the steady state, and that is
accepted: one answers "what would Fabric advance to", the other "does desired
state need advancing".

### Integration status

`GET /api/integrations/git` answers whether the platform can read desired
state. It is **derived, never advanced** — there is no stored "connected" flag
for something to forget to clear:

| Reported | Derived from | What an operator does |
|---|---|---|
| `not_configured` | nothing is bound | connect a repository |
| `connected` | bound, and the last sweep read it | nothing |
| `invalid` | bound, and the credential was refused | reconnect |
| `error` | bound, and reads are failing otherwise | look at the platform |

`invalid` is separated from `error` because only one of them is fixed by
reconnecting. A revoked or removed installation lands in `invalid`, and that is
the case this platform cannot be *told* about — the operator plane is a tailnet
with no inbound path from a Git host, so there is no webhook and the platform
has to notice for itself on the next sweep.

The response carries status, a human-readable connection description, and when
desired state was last read successfully. **No credential, no reference to one,
and no path** — section 15, checked by `scripts/check_architecture.py`.

It requires an operator like every other client-facing handler: whether this
platform is connected, and to what, is reconnaissance an unauthenticated caller
should not get for free.

### Who an operator is

Two postures, and a deployment states which one it runs.

`mode = "oidc"` is the only posture. The control plane verifies a token
the platform's own realm issued: the signature against the realm's published
keys, the issuer matched exactly, the token issued to the console's client, and
a **realm role** that confers operator authority. Authority therefore lives in
the identity provider, where joiners and leavers are already handled, rather
than in a list of names in a deployment.

```toml
[control_plane.operator]
mode = "oidc"
issuer = "https://auth.example.test/realms/master"
client_id = "saas-fabric-console"
required_role = "fabric-operator"
redirect_uri = "https://fabric.example.test/"

# Only when the address the browser uses is not one this pod can resolve.
# reachable_at = "http://keycloak-http.identity.svc.cluster.local/realms/master"
```

**`issuer` and `reachable_at` are two different questions**, and on a cluster
they usually have two different answers. The issuer is what appears in a token
and where a *browser* is sent; `reachable_at` is where this process fetches the
signing keys and redeems the code. Collapsing them fails in a way that reads as
something else — every operator refused, and a log saying either that no key
set arrived or that the token is not from this realm.

**There is no second posture, and no development shortcut.** A trusted-header
one used to sit beside this. It was safe only because of *where the service
sat*, and it asserted a name while lending nothing — which matters now that the
platform acts on Keycloak with an operator's own bearer
([ADR 0012](../decisions/0012-the-platform-acts-on-keycloak-as-the-operator.md)).
An operator established by a proxy header could not authorise a realm, so half
the control plane would not work under it.

Local development therefore needs a Keycloak. The shipped example says so
rather than faking it.

**The realm needs two things before the OIDC posture works**, and neither is
created by this application yet:

| What | Why |
|---|---|
| A **public** client `saas-fabric-console`, PKCE required, redirect URI set to the console's origin | the console holds no secret, so PKCE is what replaces one |
| A realm role `fabric-operator`, granted to each operator | this is what the control plane checks to let them in |
| Each operator holding master-realm **`admin`** | this is what *Keycloak* checks when the platform creates a realm as them |

The last row is easy to get wrong and expensive to discover. `create-realm`
alone is not enough: creating a realm grants the creator that realm's
administrative roles into tokens minted *afterwards*, and a borrowed token
cannot be re-minted — so an operator with only `create-realm` creates a realm
and is then refused on the first role inside it. See
[ADR 0012](../decisions/0012-the-platform-acts-on-keycloak-as-the-operator.md).

Automating both in the reconciler is the obvious next step. It is deliberately
not in this change: it needs a broader grant on the master realm than the
platform's service account holds, and that grant deserves a decision of its own.

### How the console signs in

The console never talks to the identity provider. It cannot — its policy is
`default-src 'self'`, so it may *navigate* to another origin but not `fetch`
one.

```text
console  → GET /api/session          where do I sign in?
browser  → provider                  top-level navigation, with a PKCE challenge
provider → console                   redirect back with code + state
console  → POST /api/session         code + verifier
API      → provider                  redeems server-side
API      → console                   the access token
```

The browser generates the PKCE verifier and keeps it; the state is compared
against what the tab stored, so a callback the tab did not start is refused
before anything is redeemed. The token is held in memory for the life of the
tab — not `localStorage`, not a cookie, and no refresh token.

**`redirect_uri` must be the console's origin root**, not a path beneath it.
The console is served by nginx with `try_files $uri $uri/ =404` and
deliberately has no history fallback, so a redirect to `/callback` would be
answered with a 404 rather than the application. The provider returns to `/`
with the code in the query string, which nginx serves as `index.html` and the
console reads on load.

### Errors

Ten codes, because an operator needs to tell the cases apart (§23):
`unauthenticated`, `unknown_client`, `invalid_request`, `desired_state_invalid`,
`revision_required`, `revision_conflict`, `realm_immutable`,
`repository_unavailable`, `repository_denied`, `repository_rejected`.

Two things no error says: anything an upstream system said verbatim, and
anything about the repository's internals.

"Reconciliation pending" is deliberately **not** an error. §23 asks that it be
distinguishable, and it is — as a status on a successful response. Reporting a
good write as a failure because a downstream convergence has not happened yet
would make the normal path look broken.

## Desired State Repository

`ClientRepository` is the port; `GitClientRepository` is the implementation.

The domain asks for a client and writes a document at a revision. Whether that
lands as a commit on `main` in `saas-fabric-clients` or as an entry in a map is
the implementation's business — and there is a second implementation,
`InMemoryClientRepository`, which implements the same concurrency rule rather
than a shortcut past it.

**Optimistic concurrency.** A revision is the stored file's blob hash. A write
carries the hash the caller believed it was editing, and the hosting API applies
it only if that hash is still current. The check is atomic on the server, so a
second control-plane replica cannot interleave with it. There is no
last-writer-wins path.

**No Git library.** The adapter speaks the hosting provider's contents API over
HTTPS. `git2`, `gix` and `gitoxide` are banned workspace-wide, which keeps "Git
is never in the request path" a structural fact about every binary this
workspace builds.

## Reconciliation

`fabric-reconciliation` owns comparison and convergence; an adapter owns one
provider's protocol. The seam is `IdentityProvider`, written in the platform's
words.

**Idempotent.** A second pass over unchanged desired state produces an empty
plan and makes no changing calls. Asserted, not assumed.

**Additive only.** Nothing deletes a realm, a role, or an application client. A
role the document does not mention is left alone: a role that exists is a role
something may already be granted, and removing a line from a YAML file is not
enough evidence to revoke it.

**Four statuses**, and the fourth is the one worth having:

| Status | Means |
|---|---|
| `Pending` | Desired state has changed and has not been reconciled since |
| `Applied` | The provider matches the desired state |
| `Failed` | The last pass could not converge it |
| `Drifted` | The provider had stopped matching a desired state already converged |

Without `Drifted`, an out-of-band change that reconciliation quietly corrects
looks exactly like an ordinary pass, and nobody learns that something outside
SaaS Fabric is editing the realms the platform owns.

A report is only meaningful for the revision it was made against. If an operator
has written a newer one, the honest answer is `pending` — even though a report
exists saying `applied`.

**On a schedule, and on demand.** A sweep runs every `interval_seconds`, and an
accepted write asks for one immediately. The interval is the one that makes the
design correct: triggers get lost, and without a poll a lost one would strand a
client forever. The interval also bounds how long drift goes unnoticed, which is
what actually sets the value.

## Platform Service Adapters

Two so far, holding the same boundary:

| Adapter | Owns | Nothing above it may name |
|---|---|---|
| `fabric-keycloak` | Keycloak admin REST | `RealmRepresentation`, `publicClient`, the admin token |
| `fabric-client-git` | the hosting contents API | `ContentsEntry`, `PutContents`, a blob, a commit |

Both are checked by `scripts/check_architecture.py`, the same way ADR 0001
contains the NDC protocol in the runtime plane. A representation that escapes
its adapter turns the platform's own model into a thin wrapper over somebody
else's.

They no longer get their authority the same way, and the difference is the
point.

`fabric-client-git` is handed a **credential the platform owns** — a GitHub
application it created for itself, whose private key lives in its own secret
partition (ADR 0011). It can act at any time because the authority is the
platform's.

`fabric-keycloak` is handed **an operator's bearer**, per request. There is no
credential in its configuration, nothing for a deployment to deliver, and
nothing to rotate. Permission to create a realm belongs to a person in the
master realm, and the platform borrows it (ADR 0012).

That asymmetry is deliberate rather than an inconsistency. Git holds *desired
state*, which the platform must be able to read to know what it should be
doing. Keycloak is *changed*, and changing an organisation's identity provider
is an act that ought to trace to somebody who chose it.

Every credential either of them handles is a redacting newtype with no
`Display` and a fixed `Debug`.

## Runtime publication boundary

This is the seam this increment deliberately **documents rather than builds**.

```text
Git desired state
      ↓
reconciliation
      ├── Keycloak            ← implemented
      ├── Envoy               ← not built
      ├── OpenBao             ← not built
      ├── OpenFGA             ← not built
      └── runtime bindings    ← not built
```

The runtime plane reads tenant bindings and DataSources from files a controller
writes, and resolves them in memory with no control-plane dependency (§6, §7).
That must not change: publishing a binding is another reconciliation target, on
the same footing as Keycloak, and **not** a control-plane mutation reaching into
a runtime registry.

Concretely, when it is built:

- it belongs in `fabric-reconciliation` or a sibling, behind a port of its own;
- it writes what `fabric_tenant_runtime::ResourceSource` reads;
- it does **not** give `fabric-control-plane` a dependency on
  `fabric-tenant-runtime`, and the architecture check will refuse one.

## Auditability

Every control-plane mutation is attributable (§24). `fabric-control-plane`
emits a structured audit event carrying who requested it, which client, the
domain operation, and the resulting revision; the log pipeline supplies the
time.

Git history is a **second** copy: the commit message carries a `Requested-by:`
trailer, because every commit is authored by the platform's machine identity and
would otherwise record only that SaaS Fabric changed something. It is not
sufficient on its own — a refused write leaves no commit and is still worth
knowing about, and a future repository may not be Git at all.

No audit record carries a secret, a token, or an administrative credential.
Nothing in the audit module is handed a value that could contain one.

## What this increment does not include

Client creation, deletion of anything, OpenFGA/OpenBao/Grafana/Envoy
reconciliation, database provisioning, runtime-binding publication, a workflow
engine, and provisioning the realm's own console client and operator role.
