# fabric-registry — LLM context

Reads published artifacts (OCI images, Helm chart versions) anonymously, for
`fabric-platform-management`. In neither plane (see
`docs/architecture/crate-dependencies.md`). Depends on
`fabric-platform-management` (implements its `Registry` and `ChartIndex`
ports and returns its `RegistryError`/`Resolved`/`Provenance`/`Version`
types), plus `async-trait`, `reqwest`, `serde`, `serde_json`, `serde_norway`
(YAML), `tracing`. Does not itself declare `fabric-core` as a dependency,
though `scripts/check_architecture.py`'s `expected` table permits it (the
check is a subset check).

## Public surface (all re-exported from `lib.rs`)

- `OciRegistry` — implements `fabric_platform_management::Registry`.
  `new(base_url: impl Into<String>, registry_host: impl Into<String>,
  timeout_seconds: u64) -> Result<Self, String>`. `async fn tags(repository)
  -> Vec<String>`. `async fn resolve(repository, tag) -> Option<Resolved>`
  (`None` on a `404` tag; never an error for a missing tag).
- `HelmCharts` — implements `fabric_platform_management::ChartIndex`.
  `new(http_timeout_seconds: u64) -> Result<Self, String>` (HTTPS-only, the
  only production constructor). `#[doc(hidden)] fn
  plain_http_to_loopback(http_timeout_seconds: u64) -> Result<Self, String>`
  (test-only; permits plain HTTP strictly to a loopback host, and never after
  an HTTPS hop has already occurred in the same redirect chain). `async fn
  versions(repository, chart) -> Vec<Version>`.

## Internal modules

- `client.rs` + `client/{provenance,resolve,tags,token,wire}.rs` —
  `OciRegistry`'s implementation.
  - `token.rs`: `get` (mint-if-needed + retry-once-on-401), `token`
    (per-repository cache, `Mutex<BTreeMap<String, String>>`, **no expiry
    tracked**).
  - `resolve.rs`: `resolve_tag` — reads the manifest, reads the
    `docker-content-digest` response header (the digest that gets pinned;
    for a multi-arch image this is the **index's** digest, not one
    platform's), calls `provenance_of`. A missing `docker-content-digest`
    header on an otherwise-successful response is `RegistryError::Refused`,
    not silently accepted. `MANIFEST_TYPES` accepts both OCI and legacy
    Docker manifest/index media types.
  - `provenance.rs`: `provenance_of` — one label
    (`org.opencontainers.image.revision`) per manifest, or per **deployable**
    child of an index (skips children with no `platform`, or
    `os == "unknown" || architecture == "unknown"` — the shape Buildx
    attestation manifests take). Zero deployable children → `Absent`, not
    `Agreed`. A child the index names that the registry then refuses to
    serve makes the whole index `Absent` (`configs_of` returns an empty
    list, which `provenance_of` folds to `Absent`), not an error.
  - `tags.rs`: `list_tags` — follows `Link: <...>; rel="next"` pagination
    (`PAGE_SIZE = 100` tags requested per page, `MAX_PAGES = 50`), resolving
    a relative next-page target against `self.base_url` and refusing to
    follow an absolute one (a registry naming its own next URL as anything
    but a path is never followed). Paging past `MAX_PAGES` is
    `RegistryError::Unavailable`, not a truncated `Ok(Vec<String>)`. A `404`
    listing tags is `Ok(vec![])` — a repository never published is a state
    discovery can describe, not a fault.
  - `wire.rs`: `TagList`, `PullToken`, `Manifest`, `Descriptor`,
    `PlatformManifest`, `Platform`, `Config`, `Labels` wire shapes.
- `charts.rs` — `HelmCharts`, its two constructors, the `ChartIndex` impl
  (builds `{repository}/index.yaml`, validates the URL, does one bounded
  streamed GET, hands the body to `index::versions_of`).
- `charts/read.rs` — `bounded_get(http, url) -> Result<String, RegistryError>`.
  `MOST = 8 * 1024 * 1024` bytes, enforced as the body streams (`response.chunk()`
  in a loop), not after full buffering. A `reqwest` redirect-policy refusal
  surfaces via `error.is_redirect()` and `std::error::Error::source()` (the
  underlying `transport::RedirectRefused`), not as a generic transport
  failure.
- `charts/index.rs` + `charts/index/seed.rs` + `charts/index/seed/entries.rs`
  — `versions_of(body, chart) -> Result<Vec<Version>, RegistryError>`.
  `seed::entries_of` is a hand-written `serde::de::DeserializeSeed` walk
  (not `serde_norway::Value`) that deserialises **only** the requested
  chart's raw entries out of `entries:`, consuming every other chart's value
  with `IgnoredAny` — an aliased YAML node under an unrelated chart is one
  parse event whether skipped once or 200,000 times, so an unrelated chart's
  shape costs time proportional to bytes read and no meaningful memory,
  regardless of alias expansion. A document with no top-level `entries` key
  at all yields an empty `Vec` (a repository with nothing published), not an
  error; a duplicated top-level `entries` key, or a duplicated key for the
  requested chart under it, is refused. `versions_of` itself refuses (never
  skips) an unparseable version string and refuses (never silently keeps one
  of) two entries of equal `SemVer` precedence, via `BTreeSet::insert` on
  `Version`'s `Ord`.
- `charts/transport.rs` + `charts/transport/{index_url,redirect,shown}.rs` —
  the HTTPS-everywhere policy.
  - `Transport::{Https, LoopbackToo}`, `MAX_REDIRECTS = 10`, `decide(transport,
    previous_hops, next) -> Result<(), String>` — pure, unit-tested without
    any HTTP connection. Under `LoopbackToo`, plain HTTP to a loopback host
    is permitted only while *no earlier hop in the chain* — including the
    very first request — has already used HTTPS; once HTTPS has been used
    once, falling back to HTTP is refused even to loopback.
  - `index_url.rs`: `validated_index_url` — refuses a URL that fails to
    parse, `cannot_be_a_base()` (e.g. `oci:user:secret@host/charts`,
    `mailto:...`), carries userinfo, or carries a query/fragment (which
    would absorb the string-concatenated `/index.yaml` suffix instead of the
    path gaining it) — all *before* the first request is sent.
  - `redirect.rs`: `policy(Transport) -> reqwest::redirect::Policy` — wires
    `decide` into `reqwest`'s own redirect callback via a `RedirectRefused`
    error type carrying the refusal reason, recoverable through
    `reqwest::Error::is_redirect()` / `source()`.
  - `shown.rs`: `shown(&Url) -> String` — the *only* way a URL should be
    rendered into any message this crate raises. Strips userinfo, query and
    fragment; caps the path at 200 chars; renders a cannot-be-a-base URL as
    `"{scheme}:[opaque]"` only. Exists because `RegistryError::Refused`'s
    `detail` reaches both the console (verbatim) and the log, and a
    credential embedded in a configured or redirected-to URL must never
    reach either.
- `errors.rs` — `transport_failure(operation, &reqwest::Error) ->
  RegistryError::Unavailable`, `status_failure(operation, StatusCode,
  &HeaderMap) -> RegistryError` (a `404` is handled by the caller and never
  reaches this function; `429` or a quota-exhausted `403` → `Unavailable`;
  everything else → `Refused`).

## Hard invariants — do not break

1. **This crate holds no credential.** `OciRegistry` exchanges an anonymous
   pull token per repository; `HelmCharts` sends none at all. Neither is
   ever handed the platform repository's GitHub App credential, and no
   change here should introduce a path for one to arrive.
2. **A missing tag/version is `None`/absent, never an error**, and nothing
   is cached between calls about what was or was not found — a partial
   publish across parallel CI jobs must read as "not yet", indefinitely,
   never as a remembered "no".
3. **Every URL this crate might render in a message goes through `shown()`**,
   never a `Url`'s own `Display` — including a redirect target, which can
   reach a message without ever passing back through
   `validated_index_url`.
4. **`HelmCharts` enforces HTTPS on every hop, not only the first request.**
   `Transport::LoopbackToo` exists only for this crate's own tests and is
   `#[doc(hidden)]`; `HelmCharts::new` never constructs it.
5. **The chart index reader never materialises an unrelated chart's entries
   as a `Value`.** `seed::entries_of`'s `IgnoredAny` walk is what keeps a
   hostile or merely large unrelated chart from becoming an unbounded
   in-memory allocation despite the byte-bounded stream.
6. **Two chart-index entries of equal `SemVer` precedence are refused, not
   resolved by picking one.**
7. **Provenance for an index requires *every deployable child* to agree**;
   a non-deployable child (no `platform`, or `unknown/unknown`) never
   participates, and zero deployable children is `Absent`.
8. **A tag listing that pages past `MAX_PAGES` is an error, never a silently
   truncated result.** A truncated list is indistinguishable from "nothing
   newer exists", which would make discovery quietly stop advancing.

## Notes

- `tests/reading_a_registry.rs` and `tests/reading_a_chart_repository.rs`
  drive fake HTTP servers (`tests/support/fake_registry.rs`,
  `tests/support/http_server.rs`) rather than a real GHCR or chart
  repository.
- `OciRegistry::path(repository)` strips a leading `{registry_host}/` if
  present, so a manifest can name a repository as
  `ghcr.io/fieldstatenz/saas-fabric` while a test points `base_url` at a
  bare loopback socket without renaming every fixture repository.
