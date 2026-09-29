# ADR 0026 — A component describes itself in an artifact attached to its image, and the catalogue selects it from registries an operator registers

- **Status:** Proposed
- **Date:** 2026-09-29
- **Applies to:** a new crate, `fabric-component`; `fabric-client-model` (the
  catalogue and its envelope), `fabric-registry`, `fabric-platform-management`,
  `fabric-platform-git`, `fabric-control-plane`, `fabric-openbao`,
  `fabric-control-plane-api` (and its console workbench),
  `apps/control-plane-ui`, `scripts/check_architecture.py`, this repository's
  `component.yaml`, `README.md`, `.github/workflows/release.yml`, and
  `docs/architecture/{packaging,control-plane,client-desired-state,crate-dependencies}.md`;
  and in `saas-fabric-platform`, `components.yaml` schema 3,
  `environments/README.md`, `scripts/check.py` and the OpenBao policy in
  `environments/lucentroot/config/openbao.yaml`
- **Related:** [ADR 0008](0008-desired-state-is-the-authority.md);
  [ADR 0011](0011-the-platform-creates-its-own-git-application.md);
  [ADR 0017](0017-fabric-decides-which-client-secret-boundary-an-operation-reaches.md);
  [ADR 0021](0021-the-product-catalogue-is-desired-state-and-the-console-creates-clients.md)
  and [ADR 0023](0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md),
  both still Proposed — accepting this accepts the parts of them it builds on;
  [ADR 0022](0022-running-versions-come-from-deployment-evidence.md);
  [ADR 0024](0024-an-instance-is-a-realm-signed-in-at-the-gateway-and-surfaces-are-federated-modules.md);
  [ADR 0025](0025-realm-bootstrap-is-platform-composition.md);
  [Packaging and release](../architecture/packaging.md); the OCI image
  specification 1.1 (artifacts and `subject`) and distribution specification
  1.1 (the referrers API and its tag-schema fallback)

## Context

SaaS Fabric has two models of a component, and only one of them is real.

**Platform Management's component is real.** `components.yaml` in the
platform repository pins a component's images by role and digest; Fabric
discovers newer versions by reading the registry, writes desired state, and
observes what runs (ADR 0022). A version is a *release unit*: every image
present, every image built from one commit.

**The catalogue's component is a free-text record.** An application
component (ADR 0021) is a kind, an "image reference" and a "version or
digest" typed into two boxes, validated for length, and read by nothing but
the entitlement filter and the console. ADR 0021 says so: *nothing deploys,
routes or observes what the catalogue describes.* Everything else an
application is — its configuration fields and the Data API resources it
exposes (ADR 0023) — is typed into the console by an operator too, although
most of it is a fact about the software, known to whoever built it and to
nobody else.

**Registries are not a product concept yet.** The control plane reads one
registry, GHCR, anonymously, named by deployment configuration. The adapter
says what should happen next in its own module documentation: *"When a package
eventually needs authenticating to, that is a registry integration with its
own configuration — not a wider scope on an existing App."*

The product owner, thinking aloud on 2026-09-26: *the component YAML could be
pushed as part of the image; a component would be configured by selecting the
component image; and there could be image registries — Docker Hub, or a
private registry — configured in the product, with images selected from them.*
On 2026-09-29: write the decision, then build it.

Four facts observed while grounding this, each of which shaped it:

- **A new GHCR package starts private.** GitHub's documentation: a package's
  first publication is private, and a linked repository's *permissions* are
  inherited but its *visibility* is not.
- **GHCR does not serve the referrers API.** `GET /v2/<name>/referrers/<digest>`
  answered `404 MANIFEST_UNKNOWN` on 2026-09-29, with and without an
  `artifactType` filter. The distribution specification's fallback for exactly
  this — a tag named `sha256-<hex>` after the subject's digest, holding an index
  the publishing client maintains — is what ORAS uses there. Docker Hub serves
  the API (a `200` with an empty index), so both paths are exercised in practice.
- **Outside GitHub Actions, GHCR accepts only a person's token.** GitHub's
  documentation: *"GitHub Packages only supports authentication using a
  personal access token (classic)"*, and `read:packages` reads every package its
  owner can.
- **Docker Hub authenticates on another host.** Its challenge names
  `auth.docker.io` as the token realm and `registry.docker.io` as the service;
  `registry-1.docker.io/token` does not exist. The adapter's hard-coded GHCR
  token path cannot read Docker Hub at all.

## Decision

### 1. A component descriptor is an OCI artifact attached to the component's primary image

A **component descriptor** — never an OCI descriptor, which this decision
calls by that full name when it means one — is a small JSON document that says
what a component is. It is pushed as an OCI artifact whose `artifactType` is
`application/vnd.saas-fabric.component.v1`, with the empty config
(`application/vnd.oci.empty.v1+json`) and exactly one layer of media type
`application/vnd.saas-fabric.component.v1+json`. Its `subject` is the digest
the version tag of the component's **primary image** resolves to — the image a
person selects the component by; an index, for a multi-platform image. The
document does not name its primary: the `subject` does. Its manifest
annotations carry `org.opencontainers.image.revision` (required: the commit it
was built from), `org.opencontainers.image.version` (equal to the document's
own version) and `org.opencontainers.image.source`.

It is **pushed last**, after every image it names exists.

It is **attached, not tagged**. It lives in the primary image's repository, so
it shares that repository's visibility and credential, needs no package of its
own, and is found from the image a person already chose: through the referrers
API where a registry serves it, and through the referrers tag schema where it
does not. It is bound to a digest rather than to a second, independently
movable tag, by the OCI 1.1 referrers mechanism that Notation, and cosign in
its referrers mode, also use.

**Exactly one component descriptor may be attached to an image digest.**
Counting is over distinct manifest digests among referrers of the family
`application/vnd.saas-fabric.component.v<N>`. More than one is refused, never
chosen between; a publisher's remedy is the next version. Every other referrer
— a signature, an SBOM, a build attestation — is ignored.

*"Pushed as part of the image"* is honoured in the way OCI allows. Inside the
image's filesystem a reader must pull layers to see it; as a label it cannot
hold structure; and a component of several images has no single image to put
it in.

### 2. What a component descriptor says, and what it never says

```json
{
  "apiVersion": "fabric.fieldstate.nz/v1",
  "kind": "Component",
  "spec": {
    "name": "reports",
    "title": "Reports",
    "description": "Scheduled reporting over a client's own data.",
    "version": "1.4.0",
    "images": {
      "api": { "repository": "registry.example.com/acme/reports",     "digest": "sha256:…" },
      "web": { "repository": "registry.example.com/acme/reports-web", "digest": "sha256:…" }
    },
    "capabilities": ["Identity", "Database"],
    "fields": [ { "key": "team", "label": "Team", "kind": "text", "required": true,
                  "default": null, "options": [], "description": "" } ],
    "resources": [ { "name": "reports", "dataSource": "primary", "collection": "reports" } ]
  }
}
```

| Section | Holds | Rule |
|---|---|---|
| `name`, `title`, `description` | the component's identity and wording | `name` is a DNS label |
| `version` | the artifact version | SemVer without build metadata or a `v` prefix, at most 128 characters, equal byte for byte to the tag it was found by and to the `version` annotation |
| `images` | every image by role, at most eight: a canonical repository and a `sha256` digest | a role is an identifier; every image is on the primary image's registry; the primary is one of them, at exactly the digest the component descriptor is attached to |
| `capabilities` | what the software needs from the platform | ADR 0021's closed list of seven, now a type |
| `fields` | at most 64 typed, non-secret configuration fields a client supplies | the catalogue's configuration field, unchanged |
| `resources` | at most 64 Data API resources it exposes | ADR 0023's application resource, unchanged |

A repository is written in full: its registry host in lower case, a port only
when it is not 443, a path in the distribution specification's grammar, and
`docker.io/library/…` in full for Docker Hub's official images — nothing is
implied. **One digest has one version**: a version tag on a digest whose
component descriptor names another version is not a release unit of that
version.

What a component descriptor **never** carries is what
[packaging](../architecture/packaging.md) keeps out of every artifact this
repository publishes — *"no manifests, no namespaces, no hostnames, no replica
counts, no secrets"*, a registry's host inside an image reference being
identity, not placement — and for the same reason no plans, features, update
policy, DataSource identifiers, connector endpoints or URLs. A resource's
`dataSource` is a *logical* name the software uses, not a placement; logical
names and collection names are what the software is.

**A component descriptor is small and parsed strictly.** At most 16 KiB, with
the counts above. Every string is bounded by the catalogue rule it reuses. The
component descriptor's parser — not the shared field and resource validators,
which also check authored content and frozen copies on every catalogue read —
additionally refuses Unicode format characters in declared text, so a title
cannot spoof one written in another direction. A duplicated JSON key is refused
rather than resolved by the last one winning.

**The published form is JSON; the authored form is YAML.** A component's
repository holds `component.yaml` — the same document without `version` and
without digests — and its publisher renders the component descriptor from it
with Fabric's renderer, into canonical bytes under a fixed file name. A release
and the server that later reads it use one set of types and validators, each at
the version it was built from. JSON is what travels because the server reads it
from a registry, and YAML aliases can amplify a small document into a large one.

**Versions are strict in both directions.** Within v1 a rule may only relax —
a longer limit, a wider character set. A new field, even an optional one, a new
enumeration value such as a capability, or a stricter rule is a new version,
stated by both the `artifactType` and the document's `apiVersion`, which must
agree. A reader that finds a component descriptor of a version it does not read
answers *invalid*, naming the version it found — never *undescribed*. Fabric's
own release publishes a new version only once a release that reads it is what
runs everywhere Fabric manages itself: reader first, as for `components.yaml`.

This also answers, for the shapes a component descriptor shares, part of what
ADR 0021 left open about changing validation rules: the catalogue's own
configuration-field, application-resource and capability rules may from now on
only relax, and a tightening is a new component descriptor version with a new
catalogue `apiVersion` whose rules apply only to documents written at it. The
reason is stronger than the catalogue's own: a published component descriptor
cannot be edited by hand, and Platform Management re-reads it on every
discovery pass and every rollback.

**The shapes it shares move to a neutral crate.** `fabric-component` — in
neither plane, with no transport and exactly the edges `fabric-core` and
`fabric-runtime-publication`, checked as a domain crate — owns the component
descriptor, its renderer, the capability type, and the configuration-field and
application-resource shapes with their validators. The hostname and identifier
value rules are re-declared there over `fabric_core::naming`, as
`fabric-runtime-publication` re-declares its identifiers. `fabric-client-model`
re-exports the moved types where they are today, byte for byte — including the
type of the catalogue's `clientFields`, which is not about components at all —
and Platform Management depends on the same crate to read a component
descriptor. One declaration of each shape, not two.

### 3. One rule decides whether a version is a release unit

Platform Management's discovery and the catalogue's selection apply the same
rule, written once, evaluated in order, the first failure deciding the answer.
For a component selected by its primary image's repository and a version:

1. the primary repository's tag for the version resolves to a digest;
2. exactly one component descriptor is attached to that digest;
3. the component descriptor is valid and of a version Fabric reads, names that
   version, and names the primary image at that digest;
4. every other image it names is on the primary's registry, exists at its
   digest, and its repository's tag for the version resolves to that same
   digest — so every image still carries the version, as a release unit's
   images always have;
5. every image's revision, and the component descriptor's own, is one commit.
   An image's revision is the set of `org.opencontainers.image.revision` values
   in its manifest's config labels and annotations — for an index, the index's
   annotations and every deployable child's labels and annotations, a child
   that declares no concrete platform not counting — and it must hold exactly
   one value.

The answer is one of four, and each has its own wording in the console:

| Answer | Means |
|---|---|
| complete | a release unit |
| **undescribed** | the tag exists and no component descriptor is attached to the digest it resolves to. Fabric cannot tell a publication in progress from one that will never attach one, and says only what it saw |
| **incoherent** | the images and the component descriptor name different commits, or another image's version tag is missing or now points at bytes the component descriptor does not name: one version built twice |
| **invalid** | a component descriptor is present and cannot be used, for a reason from a closed list the console shows: unreadable, a version Fabric does not read, the wrong version, several attached, an image that does not exist, an image with no revision or more than one, an image on another registry, a repository not registered (catalogue), or roles and repositories other than the ones the environment pins (Platform Management) |

In Platform Management's discovery nothing about an answer is remembered; every
pass asks again. A registry that cannot be asked — a timeout, a `429` or `5xx`,
or a `401` or `403` once a token was issued — is an error, never an answer: a
rate limit must not read as *undescribed*, and an image that cannot be read is
not an image that does not exist.

**Every digest Fabric records is one it computed.** A manifest or blob fetched
by digest is hashed, and a mismatch is refused. A tag is resolved to the digest
its registry names, and that digest is then fetched and hashed like any other,
unless bytes Fabric already verified for it are held — so a
`Docker-Content-Digest` header is a pointer Fabric checks, never a fact it
records. Listings, referrers responses and token responses are bounded, not
content-addressed. Only `sha256` is accepted. A browser's digest is never
accepted at all. This changes how every kind of component is resolved, the
`oci` kind included.

Verified bytes are content, not an answer: Fabric may hold a bounded cache of
manifests and blobs it has hashed, keyed by digest, because what a digest names
cannot change. That is what keeps a sweep within a registry's pull quota, since
resolving a tag is then a `HEAD` wherever the registry answers one.

### 4. How Fabric reads a component descriptor

| Read | Answer |
|---|---|
| `GET /v2/<name>/referrers/<digest>` — `200` with an OCI index | the list; its `Link` pages followed on the same origin, bounded, running past the bound an error |
| the same — a `200` of any other content type, or a `404` whose error is not `NAME_UNKNOWN` | the registry does not serve the API; read the tag schema |
| the same — `404 NAME_UNKNOWN` | the repository does not exist: an error |
| `GET /v2/<name>/manifests/sha256-<hex>` — `404` | nothing is attached through the tag schema |
| the same — `200` with an OCI index | the list |
| the same — `200` with anything else | *invalid*: the referrers tag does not hold an index |
| any read — a timeout, `429`, `5xx`, or `401`/`403` after a token was issued | an error |

When the referrers API answers, the tag schema is read as well and the two
lists are merged by digest, so component descriptors attached before a registry
started serving the API are still found. Entries are kept by `artifactType`
family whatever a registry says it filtered; Fabric never relies on a
registry's filter.

**Every candidate is fetched by digest and checked, on both paths.** It must be
an OCI image manifest of the family's `artifactType`, with the empty config,
exactly one layer of the matching media type no larger than 16 KiB, and a
`subject` equal to the primary's digest. An entry whose manifest answers `404`
is not attached. An entry whose manifest names another subject is *invalid*.
The tag-schema index is maintained by whoever publishes, so Fabric trusts none
of it until each manifest is checked.

**Every body is bounded before it is parsed**: a manifest or index at 4 MiB, a
referrers list at 1 MiB, a token response at 16 KiB, a tag page at 1 MiB.

**Redirects.** A blob read follows redirects, at most ten, to any HTTPS origin,
because every hosted registry serves blobs from a CDN; credentials never follow
a redirect to another origin (reqwest removes `Authorization` when the host or
port changes, and a test with two loopback servers proves it). A manifest, tag,
referrers or token request does not follow a redirect to another origin. No
request carries a `Referer`, and no redirect target appears in a diagnostic —
its signed query is a credential.

### 5. Registries are integrations an operator registers

A **registry** is a product concept with a closed set of kinds:

| Kind | Endpoint | Token realm | Names repositories as |
|---|---|---|---|
| `ghcr` | `https://ghcr.io` | `https://ghcr.io/token`, service `ghcr.io` | `ghcr.io/…` |
| `dockerHub` | `https://registry-1.docker.io` | `https://auth.docker.io/token`, service `registry.docker.io` | `docker.io/…` |
| `distribution` | `https://host[:port]`, given by the operator | the origin its challenge names when it is registered, recorded and shown | its host, with the port if any |

**A registry is named by its host**, because that is how every image reference
already names it, and there is at most one per host. The host is a lookup key
into records Fabric already holds, never a location a request builds from.
`ghcr.io` may be registered only as `ghcr` and `docker.io` only as
`dockerHub`. A registry's kind, host and endpoint are fixed when it is
registered: changing one is removing the registry and registering another, so a
stored credential never moves to a different endpoint. A `distribution`
endpoint is an origin — no path, no user information, no query, no fragment,
no IP literal.

**Every connection Fabric makes for a registry an operator registered goes to a
public address** — to its endpoint, its token realm, a redirect target or a
pagination link — checked on every connection after name resolution, and for an
IP-literal redirect target before it is followed, with IPv4-mapped IPv6
addresses normalised first. Loopback, link-local (which covers cloud metadata
endpoints), private, shared, unique-local, multicast and unspecified addresses
are refused, and a redirect to one fails as any refused address does. Its
client uses no ambient proxy, sends no `Referer`, and speaks only HTTPS; plain
HTTP to loopback exists only in a constructor tests use.

**The deployment's registry is configuration, and keeps its own rules.**
`[platform_management.registry]` names where Fabric reads its host — its
endpoint, a mirror or not, on any network the deployment chooses. It now follows
its own challenge, as a `distribution` registry does, with the realm origin it
names recorded when the control plane starts, rather than assuming GHCR's token
path. An operator's registry for that host contributes a credential and
repositories only: its endpoint must equal the deployment's, and its credential
is presented only to that endpoint. Removing it restores the anonymous default.
Registries and the picker exist whether or not Platform Management is
configured; their timeouts are a deployment's, in a `[registries]` section of
their own.

**Authentication follows each kind's rule, and nothing else.** `ghcr` and
`dockerHub` use the realms above; a challenge naming anything else is an error.
A `distribution` registry's realm origin is read from its challenge when it is
registered; a later challenge naming another origin is an error that names
both, and nothing is sent to the new one. A `Basic` challenge is honoured only
by a `distribution` registry, and only toward its own origin. A registry that
never challenges is read with no credential. A token request asks for no scope
when a credential proves the `/v2/` endpoint, and otherwise only for
`repository:<path>:pull`, whatever the challenge offered. A proof with no
credential ends at the challenge: a `Bearer` challenge naming a realm the
kind's rule allows is the proof, and no token is asked for — GHCR's realm
refuses an anonymous request for no scope while granting every public
repository.

**A credential is optional.** Reading needs none. When one is given it is a
username and a long-lived token. It is stored in the control plane's secret
partition beside the Git applications' private keys and treated as they are —
never returned by any route, never logged — and every change to it is audited,
as a client secret's write is under ADR 0017; unlike a client secret it has no
reveal. The console shows the username and which operator set the token, and
when. Registries whose credential is a short-lived exchange (ECR's authorization
token, Artifact Registry's access token) are not supported. Neither GHCR nor
Docker Hub offers a flow in which the platform creates its own credential, as a
GitHub App manifest does for ADR 0011, so a person types this one once.

**A credential is presented only for the repositories registered under its
registry.** Every other repository on that host is read anonymously. Setting,
replacing or removing a credential discards every token Fabric obtained with the
previous one before the change is reported as done. **A credential its realm
refuses is marked refused** — held in memory, shown in the console, and not
presented again until an operator replaces it or proves it again, so a sweep
does not retry it every minute into a locked account; a restart presents it once
more. A refused credential is its own error, `502`, not a retryable `503`.

Registering a repository that Platform Management pins, under a registry that
holds a credential, makes its discovery use that credential; if it lapses,
discovery of that component fails with the registry's error, naming it, and
removing the repository restores anonymous reading. The registries section says
which registry each managed component is read through.

**A registry's repositories are registered, not browsed.** Neither GHCR nor
Docker Hub serves the distribution catalog endpoint to an anonymous client, so
the picker offers the repositories an operator registered. A repository is
proven before it is recorded: its tag listing must answer successfully. A
`401`, `403` or `404` is refused with one message — *not readable through this
registry* — so Fabric adds no distinction the registry did not make; a
credential its realm refuses while proving is the refused-credential error. A
Docker Hub repository of one path segment is refused by its grammar, with a
message naming `docker.io/library/<name>`. A registry is proven the same way
before it is recorded: its `/v2/` endpoint answered, through the challenge and
with the credential when one is given.

**What the console shows is what was proven, never "connected".** Each
registry shows its kind, host, endpoint, the realm origin it recorded, whether a
credential is held and whether it was last refused, and its repositories with
when each was proven. The deployment's registry is listed as the deployment's.

**Where it is kept.** Registries are operator-managed integration state in the
instance's secret partition, as the Git integrations' records are. That is an
exception to ADR 0008's "OpenBao … never sources of truth", stated here
because ADR 0011 made the same one without naming it: a credential cannot live
in desired state, and a registry that depended on the client-configuration
repository could not be registered before it. The exception passes ADR 0023's
test — *if Fabric is dead or bypassed, the repository must still explain* —
because the catalogue and `components.yaml` hold full repository names, from
which a registry's host is derived, and never a registry record. A registry may
therefore be removed while a catalogue holds components resolved from it; what
they recorded stays, and selecting another version needs the registry back.

Every operator may manage registries; the control plane has no operator tiers.
The ordering of registry changes is one process's, as every integration's is.
Registering, replacing and removing a registry, its credential and its
repositories — including a proof that was refused — are audit events naming the
operator, the host, the operation and the outcome, never a username, a token or
a realm's response.

### 6. Fabric's read credential is not written anywhere a cluster reads

A registry token stays in the control plane. No route, log, audit event,
diagnostic or document carries it out, and the control plane writes no pull
secret. The platform's External Secrets policy, `platform-secrets`, can today
read every path under `secret/data/platform/`, Fabric's instance partition
included; nothing references it, but nothing prevents it. `saas-fabric-platform`
denies that policy `platform/saas-fabric/instances/*`, and its `check.py`
refuses a policy or `ExternalSecret` that reaches the prefix; that lands before
any environment stores a registry credential. An OpenBao initialised before the
denial keeps its old policy until it is next rebuilt, because the policy is
applied once, at first start; until then the check is what holds, since nothing
the platform repository declares can read the partition, and no person runs a
policy command to close it sooner. How a cluster will pull a private
component's images is not decided here: GHCR offers only a person's token, and
the platform cannot generate one the way ADR 0025's credentials are generated.

### 7. The catalogue selects a described component, and the server resolves it

An application component gains a fourth kind, **`described`**: a component
selected by its primary image's repository and a version tag. The server
applies §3 and records a **resolution**: the repository, the version, the
primary image's digest, the component descriptor's digest, the commit, when it
was resolved, and a frozen copy of the whole component descriptor — `apiVersion`
and `kind` included, so each copy is validated on every catalogue read with the
rules of the version it records. The component's `reference` is the canonical
primary repository and its `version` the version tag, both derived from the
resolution and checked equal to it on every catalogue read; a hand edit that
makes them disagree makes the catalogue unreadable, which is the point.

**In the catalogue, every repository a component descriptor names must be
registered.** A repository that is not is *invalid* before any request is made
to it, so a component descriptor cannot make Fabric present a registry's
credential to a repository an operator did not choose, and a private component
of several repositories is resolved by registering each of them.

**Selecting is a catalogue command of its own**: an application, a component
id, a repository and a version, in a body that refuses unknown fields.

| The component id names | Selecting |
|---|---|
| nothing | creates a described component, named by the component descriptor's `title`, not included in every plan, with a manual update policy |
| a described component | re-resolves it, from any registered repository, keeping its name, whether every plan includes it, and its policy |
| a `container` or `helm` component | converts it in place under its id, keeping its name, whether every plan includes it, and its policy, so the features that name it and the plans that grant them are kept |
| a `capability` component | is refused |

It is refused, with the answer named, for a repository not registered —
before any request — and for anything §3 does not call complete; and when the
version resolves to the component descriptor the component already records, so a
timestamp never becomes a change. A registry that cannot be asked is `503` with
`Retry-After`; a refused credential is `502`. A selection is an audit event
naming the operator, the outcome, the repository, the version, the primary
image's digest and the component descriptor's digest.

**Resolution is bounded, and runs outside `ClientService`.** The handler reads
the catalogue first — its revision, the application, the component's kind, and
whether the repository is registered — then a service of its own, handed the
registry port and §3, resolves against a deadline that cancels registry reads
still in flight (reads are safe to abandon; writes are not), checking the images
other than the primary concurrently, and passes the resolution to the
catalogue's pure `apply` for the write. Startup refuses a configuration where a
Git read, the resolution budget and a Git write do not fit under
`request_timeout_seconds`. This is the second operator request, after rollback,
whose write depends on a registry read, and the control-plane architecture's
"does not call a platform service" gains that exception by name.

**Saving a draft cannot carry a resolution.** `saveApplication` takes a request
shape of its own, not the stored definition, tagged by `kind`: a `container`,
`helm` or `capability` component is what it is today; a `described` one is its
id, name, whether every plan includes it, and its policy — and a body that
carries its `reference`, `version`, resolution, a digest or declared content is
refused, not ignored. The server keeps the stored resolution for each described
component the save names, drops a component the save omits, and refuses a
described component it holds no resolution for, and any component whose stored
kind differs from the kind the save gives it — in either direction, even when
the same save omits the old one. Only selecting creates or re-resolves a
described component; turning one back into free text takes two saves. The
console keeps resolutions in its local draft, so its comparisons with the last
release compare like with like, and strips them only when it sends.

**Publishing freezes by copying the draft** and does not call a registry — a
copy, not a reference, as ADR 0021 rules for releases. The draft must be saved
before a version is selected, because selecting writes it; the picker is
disabled while the draft has unsaved changes, and the answer replaces the local
draft.

**The picker lists what is registered.** Registries, their registered
repositories, and a repository's *version tags* — tags that parse as a version,
newest first, with how many tags were not versions. **The list is navigation;
the selection is validation.**

**A catalogue is written at the lowest `apiVersion` that expresses it**,
computed on every render: `fabric.fieldstate.nz/v1` for a catalogue with no
described component in any draft or release, and `fabric.fieldstate.nz/v2` for
one with any. This build reads both, re-renders a `v2` catalogue without one as
`v1`, and refuses a described component under `v1`, so an older build refuses a
catalogue that holds one by naming the version it found. A client document whose
`spec.product` copies a release holding a described component is refused by an
older build as a malformed product, not by version; the reach of that is stated
under Consequences.

`container`, `helm` and `capability` components still read, save and publish,
and stay authorable. A new component is described unless the operator chooses
otherwise.

### 8. The component descriptor declares what the software is; the operator composes what is sold

- **Declared fields and resources are in effect** beside the ones an operator
  authored. One accessor on the definition gives the effective fields and
  resources, and every reader uses it: definition validation, the
  cross-application resource check on both sides, the runtime catalogue, and the
  check of a client's configuration. The console computes the same union with
  one helper, tested against the Rust fixtures. A key or resource name declared
  twice — by two components, or by a component and an operator — is refused, and
  a declared resource that collides with another application's published one
  refuses the publish, naming both, exactly as ADR 0023 rules for authored ones.
- **Declared fields apply to every client of the application**, whatever its
  plan grants — as authored application fields do today.
- **Capabilities are shown as needs, never as provided.** Nothing provisions a
  capability yet (ADR 0021). `capability` components stay operator-authored.
- **Navigation stays the operator's.** A v1 component descriptor declares no
  modules. Module declarations are a later component descriptor version, written
  when ADR 0024's shell fixes a module entry's shape; placing a module remains
  instance configuration and a trust decision, never a consequence of selecting
  or publishing a component.
- **The operator still owns** the application's name, plans, features, which
  components a plan grants, update policy, the hostname template, and any
  fields, resources and navigation they author.

The console shows declared content read-only, labelled as declared by the
component descriptor, with its digest and when it was resolved — what Fabric
observed then, not a claim that the artifact is still there — and every declared
resource's operations and every declared default in full.

### 9. Platform Management reads a component through its component descriptor

`components.yaml` schema 3 adds a third artifact type:

```yaml
artifact:
  type: described
  primary: runtime                    # the role whose image carries the component descriptor
  sourceRevision: 5320432…
  images:
    console:      { repository: ghcr.io/fieldstatenz/saas-fabric-control-plane-ui, digest: sha256:… }
    controlPlane: { repository: ghcr.io/fieldstatenz/saas-fabric-control-plane,    digest: sha256:… }
    runtime:      { repository: ghcr.io/fieldstatenz/saas-fabric,                  digest: sha256:… }
```

The images stay: the overlays are rewritten from them, and the platform's check
compares every rendered reference with them. **The component descriptor's digest
is not recorded in the file.** A digest typed into a platform pull request is a
fact nothing proved, and every break-glass edit of the version would need one
looked up by hand. Instead every advance and rollback re-applies §3, and its
commit message names the component descriptor's digest, as it already names the
commit an image was built from.

Discovery lists the primary repository's versions and applies §3, with the
environment's pinned roles and repositories; they are read as §5 says, so a
repository not registered is read anonymously. Advancing and rolling back write
the images and the commit together, in one commit, as for `oci`. The console is
told the artifact kind `oci` for a described component, because a rollback
restores the same exact bytes. *undescribed*, *incoherent* and *invalid* each
have their own wording, *invalid* with its reason, and none falls through to
another.

A file may be schema 2 or 3, and Fabric writes back the version it read. Schema
3 may hold `oci` and `helm` components as well; `type: described` requires it,
and `primary` must be one of the component's images. A build that predates this
refuses a schema 3 file by naming its version rather than failing on an unknown
field. The five-version bound on a rollback listing, set by measuring GHCR, is
measured again on this path against the first release that carries a component
descriptor, before the rollout's switch.

### 10. Fabric's own release publishes its component descriptor

`component.yaml` at the root of this repository describes `saas-fabric`: its
three images by the roles the platform already pins — `runtime`, `controlPlane`,
`console`. It declares no capabilities, fields or resources: Fabric *is* the
platform that provides them. The release workflow's matrix gives each image its
`role` and marks `runtime` as primary.

- **On every pull request**, a workspace test renders `component.yaml` with
  synthetic digests, reads the result back through `fabric-component`'s parser,
  and checks its roles and repositories, and the primary, against the release
  workflow's matrix. Changing a role or a repository is a change to the contract
  with `saas-fabric-platform`.
- **On a tag**, the version is computed once, by a leading job every other job
  uses; each build job records the digest it pushed, and refuses to move a
  version tag that already exists — reusing a version is not supported, so a
  build job that pushed cannot be re-run, and a new preview is the fix; and a
  final job, which fails rather than skips when the tagged commit is not on
  `main`, and runs only when every earlier job succeeded:
  1. checks each tag still resolves to the digest its build recorded;
  2. renders the component descriptor from those digests with the renderer this
     repository builds;
  3. reads the component descriptors already attached to the primary digest:
     none, and it attaches; one whose layer and `artifactType` equal what it
     would push, and it has nothing to do; anything else, and it fails without
     pushing;
  4. attaches by digest, with ORAS pinned by digest and the tagged commit's time
     as `org.opencontainers.image.created`, so a re-run produces the same bytes;
  5. evaluates the version with Fabric's own reader, anonymously, and requires
     complete; and records the component descriptor's digest in the run's
     summary.
- **A component descriptor is never attached to an image published before the
  release that carries this job**, because that would make a build that refuses
  these formats a rollback target. Fabric cannot enforce it — anyone who can push
  can attach one — so the job attaches only to digests it has just pushed.
- **No clean-up of untagged versions may run on the primary package.** On GHCR a
  component descriptor is an untagged package version, and deleting it makes
  every release unit it described *undescribed*.

### 11. What a component's publisher does

For anyone publishing a component Fabric can select:

- tag every image of a version with that version, in bare SemVer — no `v`, no
  build metadata — and give one digest one version;
- label or annotate every image with `org.opencontainers.image.revision`;
- keep every image on one registry;
- push the images, then attach one component descriptor to the primary image's
  digest — the index, for a multi-platform image — with a fixed creation time,
  once, using a referrer-aware tool such as `oras attach`; a version with more
  than one attached is never a release unit, and the remedy is the next version;
- copy a component to another registry only with a tool that carries referrers,
  such as `oras cp -r`; and
- render the component descriptor with the renderer in this repository. A
  distributable renderer and a published JSON Schema are later work; until then a
  hand-written component descriptor is validated when an operator selects it.

## What is built first

**No commit on `main` attaches a component descriptor until every reader this
decision adds is on `main`.** A preview can be cut from any commit, so where the
attach step lands is the rule, not which release ships what. In this order:

1. **The contract.** `fabric-component`, with the shared shapes moved into it
   and tests that an existing catalogue and client document re-render
   byte-identically; `component.yaml`; the renderer as a command the release can
   run; the matrix's `role` and primary, inert; the pull-request test. Nothing
   is published. Ends in a pull request whose checks render Fabric's own
   component descriptor.
2. **The readers.** The adapter reads component descriptors through both paths,
   hashing every digest for every kind of component, bounding every body and
   following blob redirects without their credentials; §3, once, with an
   evaluator the release can run; Platform Management's `type: described` with
   schema 2 and 3; the console's diagnostics. In `saas-fabric-platform`,
   `check.py` and the contract README learn schema 3 — inert until used.
   Observable only after the rollout's switch; until then, proven by tests.
3. **Registries.** The adapter's authentication and address rules and its
   per-host routing; registries in the control plane, their store, routes and
   audit; an *Image registries* section on the Integrations page; the
   workbench's registries, in memory. In `saas-fabric-platform`, the External
   Secrets denial. Ends in an operator registering Docker Hub and GHCR
   repositories in the workbench and seeing what was proven.
4. **The picker.** The catalogue's `v2` envelope and the `described` kind; the
   resolution service and its budget; the selection command; the save request;
   the effective fields and resources; the console's picker and declared views.
   Ends — once slice 5's first release exists — in an operator registering
   `ghcr.io/fieldstatenz/saas-fabric` and its two sibling repositories and
   selecting a version. Declared fields and resources are proven by tests until a
   component that declares them is published. The registries section's
   account of which registry each managed component is read through (section
   5) lands here, where the platform view first carries each component's
   repositories; slice 3 has nothing to join it against.
5. **Publishing.** The release workflow's final job in §10, and the packaging
   documentation's fourth artifact.

Then the rollout, in this order: merge the External Secrets denial; tag the
release carrying all five; let LucentRoot advance to it through the `oci` path it
uses today; confirm it runs (ADR 0022); measure the rollback listing's bound on
its component descriptor; hold `saas-fabric` in the console; switch it to
`type: described`, schema 3, in a platform pull request; resume; and let the
next preview advance through its component descriptor.

## Consequences

### Good

- An operator composes an application from what the software says it is,
  instead of retyping it.
- Deployment and the catalogue agree on what a version *is*: one rule, one set
  of types and validators, digests Fabric computed.
- A version is complete only when its component descriptor exists; a version
  without one is shown as exactly that.
- Docker Hub and private registries become readable without widening any
  existing credential, and a credential reaches only the repositories it was
  registered for.

### Bad, and accepted

- **Older builds cannot read what newer ones write.** A `v2` catalogue and a
  schema 3 `components.yaml` are refused by name; a client document copying a
  release with a described component is refused as a malformed product, which
  stops that client's product and identity routes, and the activity listing,
  which fails whole. An unreadable catalogue also stalls runtime publication and
  stops the workbench opening. Every release that carries a component descriptor
  reads every format this decision introduces, because the attach step lands
  last, and in-product rollback of a described component offers only versions
  with a component descriptor — so it cannot reach a build that refuses them. It
  can reach one older than a *later* format change; a floor for rolling back
  Fabric's own component is owed. Break-glass below this decision, in one commit
  per repository: `type: oci` and schema 2 in `components.yaml`; and every
  described component removed from the catalogue's drafts and releases, its id
  removed from every feature that names it, the catalogue's `apiVersion` set back
  to `fabric.fieldstate.nz/v1`, and the same components removed from every
  client's `spec.product` — records that are otherwise never edited.
- **Previews cut from a commit not on `main`** get no component descriptor, so
  once `saas-fabric` is described they are *undescribed* and are never advanced
  to or offered for rollback.
- **Two lookup paths, and a fragile one.** The referrers API and its tag-schema
  fallback are both read, because the registry Fabric's own images live on
  serves only the second. The tag-schema index is rewritten by every publisher
  that attaches anything; one that drops an entry makes a version *undescribed*.
- **Anyone who can push to a component's repository can make its versions
  invalid**, by attaching a second component descriptor. Digests prove content,
  not publisher; a signature policy is later work.
- **A pasted token.** The first platform credential a person types in. On GHCR
  it is a classic personal access token belonging to a person, reading every
  package that person can read and lapsing when they leave. Fabric limits where
  it presents the token; it cannot limit what the token can do if it leaks. Any
  operator can register another repository under a registry that holds one, and
  so extend where it is presented to any repository it can read, without knowing
  the token; each registration is proven and audited.
- **One registry per host.** Two Docker Hub accounts cannot both be read.
- **Operators register public registries only.** A registry reachable only on a
  private network — an in-cluster Harbor, say — can be read only as the
  deployment's registry, named by configuration; and with no ambient proxy, an
  operator's registry cannot be read where egress needs one.
- **Selecting and publishing a described component is a grant.** Its author's
  declared resources become Data API resource definitions over every tenant's
  logical data source of that name, as an authored one would (ADR 0023), its
  declared defaults become every client's configuration unless overridden, and a
  newer version that widens a resource's operations reaches every tenant when the
  operator publishes it. The console shows every declared operation and default
  for that reason.
- **Size.** Every release and every client assignment carries its frozen
  component descriptors. At the 16 KiB bound, a 900 KiB catalogue holds about
  fifty-five frozen copies across all drafts and releases before every catalogue
  command is refused, and ADR 0021 owes the trimming. Component descriptors of the
  size fields and resources take in practice are a few kilobytes.
- **A registry read on the catalogue's write path**, bounded and budgeted, and a
  Docker Hub anonymous pull quota shared with the cluster's own pulls; a
  credential for Docker Hub is recommended.
- **Tools.** A publisher needs a referrer-aware tool, and a mirror must carry
  referrers. Untagged-version clean-up on a primary package deletes component
  descriptors.

## Alternatives rejected

- **The component descriptor in the image, or in its labels.** A layer has to be
  pulled to be read; a label cannot hold structure; a component of several
  images has no one image to hold it.
- **A repository of its own, tagged by version.** Simple to read, but a second
  thing to select for every component, and on GHCR a new package starts private.
- **A tag of its own in the image's repository** (`1.4.0.fabric`, say). One
  lookup path, copied by every mirror tool, safe from untagged clean-up and from
  a second attachment — but a second tag per version that moves independently of
  the image's, a naming convention Fabric would have invented, and one a version
  parser can mistake for a prerelease. The referrer is bound to the digest by the
  standard mechanism.
- **YAML on the wire.** Aliases make a bounded body an unbounded parse.
- **Free-text references with a "check" button.** The browser would still author
  what gets recorded.
- **Registry records in desired-state Git.** A credential cannot live there, and
  registries would be unavailable until the client-configuration integration was
  connected.
- **A generic `/api/integrations/{kind}`.** Registries get their own route family,
  as each Git integration has its own.
- **Recording the component descriptor's digest in `components.yaml`.** A digest
  nothing proved, copied by hand, and a second value every break-glass edit would
  have to find.

## What this does not decide

**Deploying application components**, the pull credentials that needs, and
their routing — still ADR 0021's to owe. **Modules**: their declaration waits
for ADR 0024's shell, and which modules an instance may load is not decided
anywhere yet. **Whether `automatic` means anything** for an application
component (ADR 0021, decisions owed to the product owner, item 7): selecting a
newer version stays an operator's act. **Publisher identity**, and a signature
policy. **Charts published to OCI registries**, and **mirrors** — a component
copied under another registry's host names its original registry. **Registries
with short-lived credentials**, and **registries on private networks** an operator
registers. **A floor for rolling back** Fabric's own component below a later
format change. **More than one replica** of the control plane, which every
integration's ordering assumes away. **Connector kinds**: a `Database` capability
cannot yet say which connector it needs, and saying so is a new component
descriptor version.

## Decisions owed to the product owner

1. **Images without a component descriptor.** Whether a plain image — resolved to
   a digest by the server, declaring nothing, with fields authored by the
   operator — can be selected, or only images that describe themselves. Until
   then, `container` and `helm` stay authorable, and a Docker Hub image that
   carries no component descriptor can be recorded only as free text.
2. **Browsing.** Registering repositories by name works on every registry with
   one protocol, and is what this builds. Browsing — Docker Hub's namespace
   listing is anonymous; GHCR's needs the same token through GitHub's packages
   API — is what was asked for, at the cost of one vendor API per kind. Typing a
   repository name departs from the console's rule that repositories are picked,
   not typed.
3. **Declared fields and plans.** As built, a declared field applies to every
   client of the application. The alternative is that it applies only where the
   client's plan grants the component that declares it.
4. **Adopting declared resources.** As built, selecting a described component
   adopts everything it declares, and publishing puts it in effect. The
   alternative is adoption per resource, with each re-resolution showing what
   changed and holding changes until they are adopted.
5. **Declared defaults.** Whether an operator may override a declared field's
   default, label or requiredness for an application. As built, declared fields
   are taken as the component descriptor states them; an override would be a later
   record beside the resolution.
6. **The free-text kinds.** Whether `container` and `helm` components stay
   authorable once plain images and charts can be selected, or become read-only
   history.
