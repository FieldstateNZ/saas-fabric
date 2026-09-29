# fabric-registry — LLM context

Reads published artifacts (OCI images, component descriptors attached to
them, Helm chart versions) anonymously, for `fabric-platform-management`. In
neither plane (see `docs/architecture/crate-dependencies.md`). Depends on
`fabric-platform-management` (implements its `Registry` and `ChartIndex`
ports and returns its `RegistryError`/`Resolved`/`Provenance`/`Attached`/
`AttachedDescriptor`/`Unusable`/`Version` types), `fabric-component` (only
`ARTIFACT_TYPE_FAMILY_PREFIX` via `family_version`, and `MAX_DOCUMENT_BYTES`;
never parses a document), plus `async-trait`, `reqwest`, `ring` (only
`ring::digest`, SHA-256), `serde`, `serde_json`, `serde_norway` (YAML),
`tracing`. Does not itself declare `fabric-core`, though
`scripts/check_architecture.py`'s `expected` table permits it (the check is a
subset check).

## Public surface (all re-exported from `lib.rs`)

- `OciRegistry` — implements `fabric_platform_management::Registry`.
  `new(base_url, registry_host, timeout_seconds) -> Result<Self, String>`
  (HTTPS only; refuses userinfo, query, fragment, an empty host, a zero
  timeout; the message names the field, never the value).
  `#[doc(hidden)] plain_http_to_loopback(same args)` (tests only; plain HTTP
  to loopback, never after an HTTPS hop). `tags(repository) -> Vec<String>`.
  `resolve(repository, reference) -> Option<Resolved>` — `reference` is a tag
  or `sha256:<64 hex>`; `None` on a `404`. `component_descriptor(repository,
  subject) -> Attached` (`Nothing | One(AttachedDescriptor) | Several {
  digests } | Unusable { reason: NotAnIndex | OtherSubject | Malformed }`).
- `HelmCharts` — implements `fabric_platform_management::ChartIndex`.
  `new(http_timeout_seconds)` (HTTPS-only, the only production constructor).
  `#[doc(hidden)] plain_http_to_loopback(http_timeout_seconds)`. `versions(
  repository, chart) -> Vec<Version>`.

- `examples/release_unit.rs` + `examples/release_unit/arguments.rs` — not
  public surface: the release-unit evaluator a release job runs (`evaluate`
  with `Expectation::Registered` accepting everything); exit `0` only on
  `complete`, `2` on arguments that are not UTF-8, a repository or version
  `fabric-component` refuses, or a `--host` the repository is not on.

## Internal modules

- `transport.rs` + `transport/{bounded,origin,redirect,shown}.rs` — shared by
  both readers. `Transport::{Https, LoopbackToo}`, `MAX_REDIRECTS = 10`,
  `permits(transport, previous, next) -> Result<(), Refusal>` (pure;
  `Refusal::{TooManyRedirects, NotHttps}` — each reader words it),
  `same_origin`, `is_loopback`, `policy(decide)` (wires a reader's decision
  into `reqwest` through `RedirectRefused`, recoverable via `is_redirect()` /
  `source()`), `shown(&Url)`, `bounded_body(response, most, operation)` (a
  declared `Content-Length` past the bound is refused before reading; then
  streamed via `chunk()`).
- `client.rs` — `OciRegistry { api, blobs, base_url, origin, registry_host,
  tokens, verified }`, `url`, `path`, the `Registry` impl.
  - `build.rs` (constructors, base-URL checks) and `http.rs` (two clients,
    both `referer(false)`, `no_proxy()`, a user agent and the timeout; the API
    client's redirect policy is `permits` **and** same origin as the first
    request; the blob client follows **no** redirect itself; refusals never
    name the target).
  - `send.rs`: `Via::{Api, Blob}`, `get`, `send` (mint-if-needed, retry once
    on `401`), `attempt` (`bearer_auth` on the first request). `reqwest`
    alone would not keep a token off a CDN: `tower-http` rebuilds each hop
    from the first request's headers and `reqwest` strips `Authorization`
    only when a hop changes host or port *from the hop before*, so a second
    hop on the same CDN would get it back. `token.rs`: `token`
    (per-repository cache, **no expiry tracked**; response bounded at 16 KiB;
    `token` or `access_token`).
  - `digest.rs`: `Content { digest, bytes: Arc<[u8]> }`, only made by
    `Content::hashed` (SHA-256 via `ring`); `sha256(text, what)` (the only
    accepted form, `sha256:` + 64 lower-case hex); `matching(content,
    expected, what)` (mismatch is `Refused`). `reference.rs`: `Reference::{Tag,
    Digest}` (tag grammar checked, since it becomes a URL path; `:` means a
    digest, which must be `sha256`).
  - `verified.rs`: `Verified`, the cache of **verified bytes** by digest —
    at most 256 entries and 8 MiB, oldest evicted first; takes `Content`
    only. Used only after something fresh named the digest (`Held::Use`).
  - `by_tag.rs`: `manifest_by_tag` — `HEAD` (a `404` is `None`); a digest
    header → held bytes or `manifest_by_digest`; `405` or no header → `GET`
    the tag, hash, and a digest header present must equal the hash (else
    `Refused` naming the tag). `fetch.rs`: `manifest_by_digest(repository,
    digest, Held)` (4 MiB, hashed, `404` is `None`). `blob.rs`: `blob`
    (declared size and body bounded, hashed; `Found::{Missing, OtherSize,
    Bytes}` — a size other than the declared one is the caller's to judge:
    a config refuses it, a component descriptor's layer is `Malformed`);
    `blob/follow.rs`: `follow_blob` follows at most ten redirects **by
    hand**, each hop held to `permits` against the whole chain, the bearer
    attached only to a hop on the registry's own origin.
  - `resolve.rs`: `resolve_reference` — a tag via `manifest_by_tag`, a digest
    via `manifest_by_digest(Held::Ask)`; the digest returned is the one
    computed. `provenance.rs` + `revisions.rs`: revisions are config labels
    plus manifest annotations; an index adds its own annotations to every
    deployable child (not `unknown`, not platform-less); `verdict` is
    order-independent (any disagreement → `Disagreed`, else any image
    without → `Absent`, else `Agreed`; no images → `Absent`). A child or
    config the registry does not serve (`404`) → `Absent`; any other failure
    is an error.
  - `tags.rs` + `link.rs`: `list_tags` (1 MiB per page, `MAX_PAGES = 50`,
    past it `Unavailable`; a `404` is an empty list on the first page only,
    and an error on a later one); `next_page` resolves a `Link` target against the
    answering URL and refuses (`Refused`) another origin or userinfo — never
    a silent end of the list.
  - `attached.rs` + `attached/{listing,tag_schema,candidate,shape}.rs`:
    `component_descriptor` — referrers API (1 MiB per page, 10 pages,
    same-origin `Link`; `200` not an index or `404` not `NAME_UNKNOWN` → not
    served; `404 NAME_UNKNOWN` → `Refused` naming the repository) **and**
    always the tag schema `manifests/sha256-<hex>`, asked for with every
    manifest type so a strictly negotiating registry cannot answer `404` for
    what it holds (`404` nothing; not an OCI index → `NotAnIndex`); merged by
    digest; kept by `family_version`; more than 16 candidates → `Several`
    unfetched, with **no** digests (none was computed); each candidate fetched by
    digest (`Held::Ask`), `404` not attached, `shape::check` (OCI manifest,
    family `artifactType`, empty config, one layer `artifactType+json` ≤ 16
    KiB, `sha256` layer, subject equal → else `OtherSubject` / `Malformed`);
    the revision annotation read by `revisions_in`, the rule every image's is
    read by (trimmed, blank is none); one attached → its layer via
    `attached/document.rs` (a `404` layer, or one not its declared size, is
    `Malformed`; a hash mismatch is still `Refused`).
  - `wire.rs` + `wire/responses.rs`: third-party shapes, never
    `deny_unknown_fields`.
- `charts.rs` — `HelmCharts`; `charts/transport.rs` words `permits` for a
  chart index and keeps `index_url.rs` (`validated_index_url`);
  `charts/read.rs` — `bounded_get` over `bounded_body`, `MOST = 8 MiB`;
  `charts/index.rs` + `charts/index/seed.rs` + `charts/index/seed/entries.rs`
  — `versions_of(body, chart)`, the `IgnoredAny` walk, duplicate precedence
  refused.
- `errors.rs` — `transport_failure`, `send_failure` (a redirect-policy
  refusal is `Refused` with the policy's own wording), `unreadable` (a body
  within bounds that is not the document expected: `Unavailable`),
  `status_failure` (a `404` that is an answer is handled where the request
  is made and never reaches it — a `404` that is not an answer, such as a
  later referrers or tag page or the token endpoint, does, and is
  `Refused`; `429`, a quota-exhausted `403`
  and `5xx` → `Unavailable`; everything else → `Refused`).

## Hard invariants — do not break

1. **This crate holds no credential.** `OciRegistry` exchanges an anonymous
   pull token per repository; `HelmCharts` sends none at all. Neither is
   ever handed the platform repository's GitHub App credential, and no
   change here should introduce a path for one to arrive.
2. **A missing tag/version/digest is `None`/absent, never an error**, and
   nothing is cached about what was found — a `404`, a tag's current digest,
   a referrers list are asked again every time. A partial publish across
   parallel CI jobs must read as "not yet", indefinitely, never as a
   remembered "no".
3. **Every digest reported is one this crate computed.** Every manifest and
   blob is hashed with SHA-256 and compared with the digest asked for or
   named; a `Docker-Content-Digest` header is a pointer that is checked,
   never recorded. Only `sha256` is accepted anywhere.
4. **The verified-bytes cache holds content, not answers.** Keyed by the
   digest its bytes hash to (only `Content::hashed` makes an entry), bounded
   (256 entries, 8 MiB, oldest first), and consulted only once something
   fresh in the same call has named the digest (`Held::Use`). Resolving a
   digest, or checking a listed referrer, always asks the registry.
5. **HTTPS on every hop, for both readers.** `Transport::LoopbackToo` exists
   only for this crate's own tests and is reached only through the
   `#[doc(hidden)]` constructors. Manifests, tags, referrers and tokens never
   follow a redirect off their origin, and a `Link` is followed only on the
   registry's own; a blob follows one to any HTTPS origin, hop by hop, and
   the pull token goes only to a hop on the registry's own origin — never
   judged against the hop before (tested with two loopback servers, over one
   CDN hop and two). No `Referer`, no ambient proxy.
6. **Every body is bounded before it is parsed**: manifest or index 4 MiB,
   referrers page 1 MiB, token 16 KiB, tag page 1 MiB, config 1 MiB,
   component descriptor layer `fabric_component::MAX_DOCUMENT_BYTES` (and its
   declared size must match), chart index 8 MiB. No `.json()` on a response.
   Tests exercise both a declared length past the bound and a chunked body
   with none, which is only caught as it arrives.
7. **A component descriptor is found here and never parsed here.** Both the
   referrers API and the tag schema are read and merged; a registry's filter
   is never trusted; every candidate is fetched by digest and shape-checked;
   a registry that cannot be asked is an error, never `Attached::Nothing`.
8. **Every URL this crate might render in a message goes through `shown()`**,
   and a registry's redirect target is never rendered at all.
9. **The chart index reader never materialises an unrelated chart's entries
   as a `Value`**, and two entries of equal `SemVer` precedence are refused.
10. **Provenance needs every deployable child to agree**, over labels and
    annotations; zero deployable children is `Absent`; disagreement wins
    over absence.
11. **A listing is never silently truncated**: past `MAX_PAGES` (tags) or ten
    pages (referrers) is an error, so is a later page that answers `404`,
    and a `Link` to another origin is
    `Refused`.

## Notes

- The integration tests drive fake HTTP servers on loopback
  (`tests/support/fake_registry.rs` + `fake_registry/`, and
  `tests/support/http_server.rs`), never a real registry: the fake's digests
  are real SHA-256 digests, it answers `HEAD`, serves or refuses the
  referrers API (`404 MANIFEST_UNKNOWN`, as GHCR does), keeps tag-schema
  indexes, and can redirect blobs to a second server standing in for a CDN.
- `OciRegistry::path(repository)` strips a leading `{registry_host}/` if
  present, so a manifest can name a repository as
  `ghcr.io/fieldstatenz/saas-fabric` while a test points `base_url` at a
  bare loopback socket without renaming every fixture repository.
