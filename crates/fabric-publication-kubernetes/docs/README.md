# fabric-publication-kubernetes

The Kubernetes adapter for runtime publication: the production owner
[ADR 0018](../../../docs/decisions/0018-runtime-state-is-published-as-three-versioned-documents.md)
specified and [ADR 0023](../../../docs/decisions/0023-data-sources-are-environment-desired-state-and-placement-is-recorded.md)
part 4 builds. It implements `fabric-runtime-publication`'s
`RuntimePublication` port by writing the runtime's three documents as three
`ConfigMap`s in one namespace, over plain HTTPS, with the pod's own service
account.

It sits in the control plane (`scripts/check_architecture.py`'s
`CONTROL_PLANE` set). Its only internal edges are `fabric-core` and
`fabric-runtime-publication`; it reaches neither plane and no Git adapter.

## What it decides: nothing

Every rule about a publication — stale or divergent revisions, a tenant
naming a data source the snapshot lacks, a data source retired while a
tenant still names it, an unintended emptying, an empty catalogue, a held
payload gone missing — is decided by `fabric_runtime_publication::plan_publication`,
the same function the filesystem adapter calls. This crate reads what the
cluster holds into a `HeldDocuments`, asks for the plan, and writes each
document the plan marks `Written`, in the order the plan declares them:
data sources, then the catalogue, then tenants (ADR 0018 part 3). A second
adapter re-implementing those rules is how two adapters drift; this one
cannot.

## Why no `kube` crate

The workspace bans Kubernetes client crates everywhere
(`docs/architecture/crate-dependencies.md`). `fabric-deployment-kubernetes`
showed the shape that needs none, and this crate takes it exactly:
`reqwest` against `https://kubernetes.default.svc`, the projected
service-account token re-read from its file on every request so rotation is
honoured, the cluster CA from the mounted root, no redirects, a four-second
request timeout and a two-mebibyte response cap. ADR 0018 said the adapter
would need the ban narrowed; it does not, and the ban stays whole.

## The objects

| ConfigMap | `data` keys |
|---|---|
| `fabric-runtime-tenants` | `tenants.json`, `tenants.manifest.json` |
| `fabric-runtime-data-sources` | `data-sources.json`, `data-sources.manifest.json` |
| `fabric-runtime-catalog` | `catalog.json`, `catalog.manifest.json` |

The names are ADR 0018's and are constants here: the platform repository
mounts them by name into the runtime, and a name that could vary is a name
the two could disagree on. Only the namespace is configuration
(`PublicationTarget`), validated as a DNS label before it can become a path
segment. Nothing from a document ever reaches a path. Every object carries
the label `app.kubernetes.io/managed-by: saas-fabric`.

## How a publication goes

1. `GET` each of the three objects, remembering its `resourceVersion`. A
   `404` is "not held". A manifest key that will not parse, or one
   describing a different document, is `Unreadable`.
2. Build `HeldDocuments` and call `plan_publication`. A refusal here means
   nothing is written.
3. Build every object the plan would write, before writing any, so a
   document that would take its object past the API server's one-mebibyte
   cap refuses the whole publication (`Unwritable`) rather than leaving the
   cluster half-updated. Nothing is ever split across objects.
4. For each `Written` document, in order: `PUT` the whole object with both
   keys and the remembered `resourceVersion` when it existed, or `POST` to
   create it when it did not. The API server refuses a replace over a
   version this adapter did not read with `409`, which is reported as
   `Unwritable` naming the document; the next pass re-reads.

`current()` is step 1 alone, reported as `PublishedRevisions`.

## What an error says

An error names the document kind and what the cluster did, in this crate's
own words. It never carries the cluster's response body, a URL, or the
bearer: the response may name things an operator's console must not see,
and the request carried a credential.

## Tests

`src/publish_tests.rs` drives the adapter against a fake API server on a
local socket (`src/testing.rs`, mirroring `fabric-deployment-kubernetes`'s
client tests, extended to read request bodies): create-when-absent with the
label and both keys; replace at the remembered `resourceVersion` with an
unchanged document left alone; a cluster `409` as `Unwritable` that leaks
nothing; a stale snapshot and an oversized document both refused after the
reads and before any write; `current()` on a held manifest whose payload is
gone; and byte equality between what this adapter sends and what
`FilesystemRuntimePublication` writes for one snapshot.

## What this crate does not do

It has no schedule and no inputs of its own: the controller that composes a
`RuntimeSnapshot` from data sources, placements and the derived catalogue,
and calls this port on an interval, is the control plane's (ADR 0023 part
4). It never deletes an object (the Role it needs has no `delete`;
deprovisioning is an empty set inside a document). It does not tell the
runtime anything happened: the kubelet refreshes whole-volume ConfigMap
mounts on its own cadence, and the runtime re-reads on its refresh interval.
