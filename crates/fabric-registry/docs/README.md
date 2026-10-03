# fabric-registry

Reads published artifacts — container images from an OCI registry, the
component descriptors attached to them (ADR 0026), chart versions from a
classic Helm chart repository — for `fabric-platform-management`. Anonymous,
read-only, HTTPS end to end, and nowhere near the platform's Git credential.

Sits in neither plane (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on `fabric-platform-management` (whose `Registry`/`ChartIndex` ports
it implements), on `fabric-component` for the component descriptor's artifact
type family and size bound (never to parse one), and on `ring` for SHA-256;
does not declare a dependency on `fabric-core` even though
`check_architecture.py`'s `expected` table permits one (the dependency-graph
check is a subset check, not an exact one).

## Why this crate exists

`fabric-platform-management` defines four ports, and this crate implements
the two that read published artifacts — `Registry` (image discovery) and
`ChartIndex` (chart discovery). The rules crate is handed
implementations, deliberately, so the registry integration and the platform
repository's GitHub App credential can never be conflated. **The SaaS Fabric
packages are public**, so this crate holds no credential of its own at all:
it exchanges an anonymous pull token per repository and reads. One fewer
secret on the path between a published preview and a running environment,
and a boundary that is clean by construction rather than by discipline — the
GitHub App that writes platform desired state is not, and must never become,
the registry credential (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
When a package eventually needs authenticating to, that is a new registry
integration with its own configuration, never a wider scope on the existing
App.

## Key concepts

- **`OciRegistry`** — implements `Registry`. Talks the standard OCI
  Distribution API (`/v2/<name>/tags/list`, `/v2/<name>/manifests/<ref>`,
  `/v2/<name>/blobs/<digest>`, `/v2/<name>/referrers/<digest>`) plus the
  anonymous token endpoint
  (`/token?service=...&scope=repository:<name>:pull`). Holds one pull token
  per repository, cached with **no expiry tracking** — an aged-out token
  simply comes back as `401`, which the client (`client/token.rs`) retries
  once with a fresh token. Cheaper to notice than to predict, and it is the
  same path a token revoked early would take anyway.
- **Listing tags follows pagination, bounded.** `client/tags.rs::list_tags`
  follows the registry's `Link: <...>; rel="next"` header (100 tags
  requested per page) for up to 50 pages, resolving a relative `Link` target
  against the URL that answered and refusing one on another origin rather
  than following it wherever it points. Paging past the bound is a hard
  error, and so is a later page answering `404`, not a silently truncated
  list — a truncated tag list would look exactly like a component whose
  newer versions do not exist, and discovery would quietly stop advancing.
- **Every digest is one it computed.** Every manifest and blob is hashed
  (SHA-256, via `ring`) and compared with the digest it was asked for or
  named by; only `sha256` is accepted. A tag resolves by `HEAD` — which does
  not count against a registry's pull quota — and then the bytes of the
  digest the registry names; where `HEAD` is not answered (`405`) or names
  no digest, the tag's manifest is read and hashed, and a
  `Docker-Content-Digest` header must agree with the hash. A header is a
  pointer this checks, never a fact it records.
- **Nothing is remembered about what was *found*.** This is a correctness
  property, not a missing optimisation: a component's images are published
  by parallel CI jobs, so a version present in two of three repositories and
  not the third is an ordinary minutes-long window. An adapter that cached
  "not there" would still believe it an hour later. What *is* held is a
  bounded cache of bytes already verified by digest (256 entries, 8 MiB,
  oldest first) — content, which a digest names forever, and never an
  answer: it is consulted only once a fresh `HEAD` or a just-read manifest
  has named the digest, so a `404`, a tag's current digest or a referrers
  list is always asked again.
- **HTTPS, one origin, and bounded bodies.** `OciRegistry::new` accepts only
  an `https://` base URL (a `#[doc(hidden)]` `plain_http_to_loopback` exists
  for tests). Two clients: manifests, tags, referrers and tokens follow a
  redirect only to the same origin, and a `Link` only on the registry's own
  origin (another origin is `Refused`, never a short list); blobs follow up
  to ten redirects to any HTTPS origin, because hosted registries serve them
  from a CDN. Blob redirects are followed by hand, and the pull token is
  attached only to a hop on the registry's own origin: `reqwest` on its own
  rebuilds every hop from the first request's headers and strips
  `Authorization` only when a hop changes host or port from the hop before,
  so a second hop on the same CDN would carry it. Neither sends a `Referer` or uses an ambient proxy. Every body is
  bounded: manifest 4 MiB, referrers page 1 MiB, token 16 KiB, tag page 1
  MiB, config 1 MiB, component descriptor 16 KiB.
- **Component descriptors are found here, read elsewhere.**
  `component_descriptor(repository, subject)` asks the referrers API and
  always also reads the referrers tag schema (`sha256-<hex>`, which ORAS
  maintains on registries without the API, such as GHCR), merges them by
  digest, keeps the `application/vnd.saas-fabric.component.v<N>` family,
  fetches and shape-checks every candidate by digest, and returns
  `Attached::{Nothing, One, Several, Unusable}`. The document comes back as
  bytes; `fabric-component` parses it, in the domain.
- **`provenance_of`** — for a plain manifest, the source-commit label on its
  config blob and the annotation on its manifest, which must name one commit
  between them. For a multi-architecture *index*, every **deployable** child
  — its labels, its annotations and the index's annotations — must agree: a child is skipped if it names no concrete platform, or
  names `unknown/unknown` (which is exactly the shape of the attestation
  manifests `docker buildx` writes alongside a real image — they carry no
  revision and would otherwise make every multi-arch image look
  unprovenanced). Zero deployable children is `Provenance::Absent`, not
  `Agreed` — nothing agreed, because there was nothing to agree.
- **`Provenance`** — `Agreed(commit)` | `Absent` | `Disagreed`, three states
  because absence and disagreement need different responses: an artifact
  with no revision label may simply still be publishing (wait); one whose
  parts *disagree* about their source commit is one version built twice, and
  no amount of waiting fixes that.
- **`HelmCharts`** — implements `ChartIndex` over a classic chart
  repository's `index.yaml`. **HTTPS end to end, including every redirect
  hop** (`charts/transport.rs`): the index this reads names a version that
  gets pinned into what Argo deploys, so `reqwest`'s default policy of
  following a redirect anywhere — including back down to plain HTTP — would
  make the first hop's TLS a formality. `HelmCharts::new` is the only
  production constructor and always enforces this; a second, `#[doc(hidden)]`
  constructor (`plain_http_to_loopback`) exists solely for this crate's own
  tests, which serve a fixture from a real socket with no certificate.
- **The index reader trusts only the requested chart.** A chart repository
  serves *every* chart it holds in one document — `charts/index/seed.rs`
  reads only the requested chart's own raw YAML entries without
  materialising the rest of the index into a value at all, so a malformed
  entry under some *other* chart's name (or a hostile YAML alias elsewhere
  in the document) cannot make this chart undiscoverable or turn a bounded
  read into an unbounded allocation.
- **Duplicate precedence is refused, not chosen between.** `ChartIndex::versions`
  refuses two entries whose `SemVer` precedence is equal — the same version
  twice, or two spellings differing only in build metadata — because a
  caller choosing "the newest" has no principled way to break the tie.
- **The index body is size-bounded** (`charts/read.rs`, `MOST = 8 MiB`),
  checked as the response streams in, not after it is fully buffered.

## How the pieces fit

```text
fabric-platform-management::Registry / ChartIndex     the ports
        |                                    |
   OciRegistry                          HelmCharts
        |                                    |
   /v2/.../tags/list, manifests,         GET {repository}/index.yaml
   blobs, referrers                      anonymous, HTTPS end to end
   anonymous pull token per repository
   HTTPS; every digest computed
```

## Getting started

```rust,ignore
use fabric_registry::{OciRegistry, HelmCharts};

let images = OciRegistry::new("https://ghcr.io", "ghcr.io", 30)?;
let charts = HelmCharts::new(30)?;

// both implement the ports fabric-platform-management consumes:
let service = fabric_platform_management::PlatformManagement::new(
    Arc::new(images), Arc::new(charts), desired_state, clock,
);
```

`OciRegistry::new` takes the API base URL and the host repositories are
*named* under (usually the same value, but a test points the base URL at a
loopback socket while manifests still say `ghcr.io/...`) — see
`OciRegistry::path` for how a caller's repository string is reconciled
against either spelling.

## Common tasks

- **Asking whether a version is a release unit, from a shell** —
  `cargo run -q -p fabric-registry --example release_unit -- <repository>
  <version> [--base-url <https url>] [--host <host>]` reads the registry
  anonymously (by default at `https://<the repository's host>`, named by
  that host — `https://ghcr.io` as `ghcr.io` for GHCR — 10 s) and applies
  `fabric_platform_management::evaluate` — the same rule discovery uses —
  with every repository counted as registered. It prints `complete` (with
  the component descriptor's digest and each role's), `undescribed`,
  `incoherent`, `invalid` with its reason code, or `not tagged`; exits `0`
  only when complete, `1` otherwise, `2` on bad arguments — including a
  repository or version `fabric-component` refuses, which would otherwise
  read as an answer about the release. It is what a
  release job runs after pushing a component descriptor.

- **Pointing at a self-hosted or Enterprise registry** — `OciRegistry::new`
  takes an arbitrary `https://` `base_url`; nothing here is GHCR-specific
  beyond the default host most deployments use and the token path
  (`{base}/token`), which a registry whose challenge names another realm
  (Docker Hub) does not serve.
- **Debugging a chart repository read** — check `HelmCharts`'s transport
  refusal messages first (`charts/transport/index_url.rs`,
  `charts/transport.rs`): a repository URL carrying userinfo, a
  query, or a fragment is refused outright before any request is sent, and a
  redirect off HTTPS is refused mid-flight.
- **Understanding a `RegistryError::Refused` on chart versions** — read
  `charts/index.rs`'s module docs: it covers a non-mapping document, a
  duplicated `entries` key, a duplicated chart key, an unparseable version,
  and a precedence collision, all distinctly refused rather than silently
  dropped or picked between.
- **Debugging tags that seem to stop partway through** — check whether the
  registry paged past 50 pages of 100 tags each; `list_tags` refuses outright
  in that case (`RegistryError::Unavailable`) rather than returning a
  partial list, so a truncated listing is never mistaken for "there are no
  more tags".

## Gotchas

- The crate is `fabric-registry` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_registry` (underscore):
  `RUST_LOG=info,fabric_registry=debug`.
- A `404` resolving a tag is `Ok(None)`, **not** a `RegistryError` — it is
  the answer the whole multi-repository design rests on (a version missing
  from one of several image repositories is a publishing window, not a
  fault). A `404` that is an answer never reaches `status_failure`; it is
  handled at the call site first. A `404` that is not one — a later page of
  a tag or referrers listing, or the token endpoint — does, and is
  `Refused`. Likewise, a
  `404` listing tags for a repository is an empty `Vec`, not an error — an
  image that has never been published is a state discovery can describe.
- A `Refused` naming a tag and "a digest header that is not the digest of
  its bytes" means the registry's `Docker-Content-Digest` disagreed with
  what it served — not a transient fault, and not retried into success.
- The registry's pull token is cached with **no expiry field at all** — this
  is deliberate, not an oversight: predicting expiry would be more code for
  the same outcome a `401`-triggers-retry already gives for free.
- `HelmCharts::plain_http_to_loopback` and `OciRegistry::plain_http_to_loopback`
  are `#[doc(hidden)]` — neither is a
  capability offered to any caller but this crate's own test suite, and
  neither should appear to be one when browsing the crate's public API.
- A chart repository's `index.yaml` is read fully into memory before
  parsing (bounded at 8 MiB) — this is a document format, not something this
  crate streams entry-by-entry; the bound exists purely to cap the
  allocation, not to enable partial reads.
- `reqwest`'s own scheme/redirect handling runs *before* this crate's
  additional per-URL checks on some paths — see
  `charts/transport/index_url.rs`'s note on why `shown()` (not a URL's own
  `Display`) is used for every message that might render a redirect target,
  including ones that never passed back through the initial-URL validator.
