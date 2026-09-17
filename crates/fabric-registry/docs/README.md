# fabric-registry

Reads published artifacts — container images from an OCI registry, chart
versions from a classic Helm chart repository — for
`fabric-platform-management`. Anonymous, read-only, and nowhere near the
platform's Git credential.

Sits in neither plane (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
Depends on `fabric-platform-management` (whose `Registry`/`ChartIndex` ports
it implements); does not declare a dependency on `fabric-core` even though
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
  `/v2/<name>/blobs/<digest>`) plus the anonymous token endpoint
  (`/token?service=...&scope=repository:<name>:pull`). Holds one pull token
  per repository, cached with **no expiry tracking** — an aged-out token
  simply comes back as `401`, which the client (`client/token.rs`) retries
  once with a fresh token. Cheaper to notice than to predict, and it is the
  same path a token revoked early would take anyway.
- **Listing tags follows pagination, bounded.** `client/tags.rs::list_tags`
  follows the registry's `Link: <...>; rel="next"` header (100 tags
  requested per page) for up to 50 pages, resolving a relative `Link` target
  against this client's own `base_url` rather than following it wherever it
  points. Paging past the bound is a hard error, not a silently truncated
  list — a truncated tag list would look exactly like a component whose
  newer versions do not exist, and discovery would quietly stop advancing.
- **Nothing is remembered about what was *found*.** This is a correctness
  property, not a missing optimisation: a component's images are published
  by parallel CI jobs, so a version present in two of three repositories and
  not the third is an ordinary minutes-long window. An adapter that cached
  "not there" would still believe it an hour later.
- **`provenance_of`** — for a plain manifest, the source-commit label on its
  one config blob. For a multi-architecture *index*, every **deployable**
  child must agree: a child is skipped if it names no concrete platform, or
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
   /v2/.../tags/list, manifests, blobs   GET {repository}/index.yaml
   anonymous pull token per repository   anonymous, HTTPS end to end
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

- **Pointing at a self-hosted or Enterprise registry** — `OciRegistry::new`
  takes an arbitrary `base_url`; nothing here is GHCR-specific beyond the
  default host most deployments use.
- **Debugging a chart repository read** — check `HelmCharts`'s transport
  refusal messages first (`charts/transport/index_url.rs`,
  `charts/transport/redirect.rs`): a repository URL carrying userinfo, a
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
  fault). `status_failure` never sees a `404`; it is handled at the call
  site before falling through to the generic status classifier. Likewise, a
  `404` listing tags for a repository is an empty `Vec`, not an error — an
  image that has never been published is a state discovery can describe.
- The registry's pull token is cached with **no expiry field at all** — this
  is deliberate, not an oversight: predicting expiry would be more code for
  the same outcome a `401`-triggers-retry already gives for free.
- `HelmCharts::plain_http_to_loopback` is `#[doc(hidden)]` — it is not a
  capability offered to any caller but this crate's own test suite, and it
  should not appear to be one when browsing the crate's public API.
- A chart repository's `index.yaml` is read fully into memory before
  parsing (bounded at 8 MiB) — this is a document format, not something this
  crate streams entry-by-entry; the bound exists purely to cap the
  allocation, not to enable partial reads.
- `reqwest`'s own scheme/redirect handling runs *before* this crate's
  additional per-URL checks on some paths — see
  `charts/transport/index_url.rs`'s note on why `shown()` (not a URL's own
  `Display`) is used for every message that might render a redirect target,
  including ones that never passed back through the initial-URL validator.
