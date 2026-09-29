# fabric-registry — LLM context

Reads published artifacts (OCI images, component descriptors attached to
them, Helm chart versions) for `fabric-platform-management`, each registry
by its kind's rules (ADR 0026 sections 4 and 5), with a credential only for
the repositories it was registered for. In
neither plane (see `docs/architecture/crate-dependencies.md`). Depends on
`fabric-platform-management` (implements its `Registry` and `ChartIndex`
ports and returns its `RegistryError`/`Resolved`/`Provenance`/`Attached`/
`AttachedDescriptor`/`Unusable`/`Version` types), `fabric-component` (only
`ARTIFACT_TYPE_FAMILY_PREFIX` via `family_version`, and `MAX_DOCUMENT_BYTES`;
never parses a document), plus `arc-swap` (the `Registries` map, swapped
whole), `async-trait`, `reqwest`, `ring` (only `ring::digest`, SHA-256),
`serde`, `serde_json`, `serde_norway` (YAML), `tokio` (only
`net::lookup_host`, for the public-address resolver), `tracing`. Does not itself declare `fabric-core`, though
`scripts/check_architecture.py`'s `expected` table permits it (the check is a
subset check).

## Public surface (all re-exported from `lib.rs`)

- `RegistrySettings` — one registry, built by kind: `ghcr()`,
  `docker_hub()` (served from `registry-1.docker.io`, naming `docker.io`,
  realm `https://auth.docker.io/token` service `registry.docker.io`),
  `distribution(endpoint, RealmRule)` (an HTTPS origin, no IP literal, no
  path/userinfo/query/fragment; naming host is `host[:port]`; `Fixed` is
  refused; answers `Basic`), `deployment(base_url, naming_host)`
  (`FollowChallenge`, `AddressPolicy::Any`, the old base-URL checks), then
  `with_credential(Credential)` and, for an operator's registration of the
  deployment's host, `at_deployment(base_url)` (the deployment's endpoint and
  `Any`, every other rule the kind's). `realm()` and `address()` read back
  what was built. `#[doc(hidden)] serve_from(loopback)` and
  `treat_loopback_as_public()` exist for tests alone (the latter honoured only
  after `serve_from`). `Debug` never shows the secret.
- `RealmRule::{Fixed { realm, service }, Recorded { origin: Option },
  FollowChallenge}`; `AddressPolicy::{PublicOnly, Any}`.
- `Credential::new(username, RegistrySecret, repositories)` (username
  non-empty with no `:`; secret non-empty; messages never carry either);
  `sharing_refusal(Arc<AtomicBool>)` hands it the refusal mark every client
  built from the same stored credential shares.
  `RegistrySecret` has no `Display`, prints `RegistrySecret(redacted)`, and
  its value is reachable only through the crate-private `expose()`.
- `OciRegistry` — implements `fabric_platform_management::Registry`.
  `with_settings(RegistrySettings, timeout_seconds)`; `new(base_url,
  registry_host, timeout_seconds) -> Result<Self, String>` (the deployment's:
  HTTPS only; refuses userinfo, query, fragment, an empty host, a zero
  timeout; the message names the field, never the value).
  `#[doc(hidden)] plain_http_to_loopback(same args)` (tests only; plain HTTP
  to loopback, never after an HTTPS hop). `prove() -> Proof` (`GET /v2/`;
  with no credential a `Bearer` challenge naming an allowed realm *is* the
  proof and the realm is not asked; with one, through the challenge with it;
  `Proof::realm_origin()` is the realm origin a `Bearer` challenge named,
  `None` for `Basic` or none). `prove_repository(repository) ->
  Readability<()>` and `version_tags(repository) -> Readability<Vec<String>>`
  (`Readability::{Readable(T), NotReadable}`: a `401`/`403`/`404` is the
  answer *not readable through this registry*; a realm change, an address
  refusal or `Denied` stays an error). `credential_refused()`. `naming_host()`. `tags(repository) ->
  Vec<String>`.
  `resolve(repository, reference) -> Option<Resolved>` — `reference` is a tag
  or `sha256:<64 hex>`; `None` on a `404`. `component_descriptor(repository,
  subject) -> Attached` (`Nothing | One(AttachedDescriptor) | Several {
  digests } | Unusable { reason: NotAnIndex | OtherSubject | Malformed }`).
- `Registries` — implements `Registry` by the repository's host (the text
  before the first `/`, port included): `new(map)`, `replace(map)` (an
  `ArcSwap`, swapped whole), `get(host) -> Option<Arc<OciRegistry>>`. A host
  with no registry, or a name with no host, is `Refused` naming the host —
  never a fall-through to a default.
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
  `same_origin`, `is_loopback`, `is_ip_literal`, `policy(decide)` (wires a reader's decision
  into `reqwest` through `RedirectRefused`, recoverable via `is_redirect()` /
  `source()`), `shown(&Url)`, `bounded_body(response, most, operation)` (a
  declared `Content-Length` past the bound is refused before reading; then
  streamed via `chunk()`).
- `settings.rs` + `settings/{kinds,applied,endpoint,credential}.rs` — the public
  settings types above; `endpoint.rs` holds the origin, base-URL and loopback
  checks and `naming_host`.
- `address.rs` + `address/{ranges,resolver}.rs` — `Address { policy,
  loopback_is_public }` (`refuses_literal`, `check`, `resolver`),
  `NOT_PUBLIC` (the whole text of every address refusal), `NotPublic` and
  `refused_in` (finds it in a send's error chain, looking inside an
  `io::Error`). `ranges::is_public` refuses loopback, unspecified,
  link-local, private, shared `100.64/10`, unique-local, site-local,
  multicast, reserved, documentation, benchmarking and discard-only ranges,
  after normalising IPv4-mapped, -compatible, NAT64 and 6to4 forms, and
  refuses local-use NAT64 (`64:ff9b:1::/48`) and Teredo outright.
  `PublicResolver` implements `reqwest::dns::Resolve` over
  `tokio::net::lookup_host`, handing back only permitted addresses and
  failing with `NotPublic` when none remain; `answering(addresses)` is its
  test lookup.
- `registries.rs` — `Registries`.
- `client.rs` — `OciRegistry { api, blobs, transport, address, base_url,
  origin, registry_host, realm, honours_basic, credential, tokens, verified
  }`; `naming.rs` (`url`, `path`, `naming_host`), `port.rs` (the `Registry`
  impl).
  - `build.rs` (`new`, `plain_http_to_loopback`, `with_settings`: transport
    and address checks on the endpoint, the zero timeout) and `http.rs` (two
    clients, both `referer(false)`, `no_proxy()`, a user agent, the timeout
    and — under `PublicOnly` — the `PublicResolver`; the API client's
    redirect policy is `permits`, no IP literal under `PublicOnly`, **and**
    same origin as the first request; the blob client follows **no**
    redirect itself; refusals never name the target).
  - `challenge.rs` + `challenge/cursor.rs`: `WWW-Authenticate` parsed as
    RFC 9110 writes it — several challenges per header, quoted strings with
    escapes, case-insensitive names; a value that does not parse offers
    nothing. `realm.rs`: `Realm::{Fixed, Recorded, Follow}` and `allow`, which
    refuses a realm on another origin naming both origins (never a path or
    query) before anything is sent to it. `answer.rs`: `bearer_realm`
    (address, then transport, then the rule — so `Follow` never records a
    realm it could not ask), then a token, held; `Basic` only when
    `honours_basic`, the request presents the credential, and toward the
    registry's own origin. `token.rs`: `token_url` (the realm's own `scope`
    and `service` pairs dropped, then `service`,
    `scope=repository:<path>:pull` or none when proving), HTTP `Basic` to the
    realm when presenting; a `401` to a credential, or a non-quota `403` to a
    credentialed proof of `/v2/`, marks it refused (`Denied`); a `403` to one
    repository's scope, or any refusal of an anonymous request, is
    `Minted::Declined` — no token, and the registry's own `401` stands;
    bounded at 16 KiB, `token` or `access_token`. `presented.rs`: `Presented`
    (repository paths, the shared `Arc<AtomicBool>` refusal), `presents(scope)`,
    `not_refused`, `refuse`, `credential_refused`. `scope.rs`: `Scope::{Registry, Repository}`,
    `Held::{Bearer, Basic}`, holdings keyed by `(path, credentialed)`,
    `authorize`. `prove.rs`: `Proof`, `prove`. `readable.rs`: `Readability`,
    `prove_repository`, `version_tags`. `tags.rs`: `list_tags` over
    `listing`, which tells a first page's `404` (`Absent`) and `401`/`403`
    (`Closed`) apart from every other failure.
  - `call.rs` + `send.rs`: `Via::{Api, Blob}`, `Call`, `Exchange`, `get`,
    `send`, `exchange` (`Denied` without contact when presenting a refused
    credential; first attempt with whatever is held — nothing, the first
    time; on a `401`, answer the challenge once and retry; a `401` to `Basic`
    marks the credential refused), `attempt`. `reqwest`
    alone would not keep a token off a CDN: `tower-http` rebuilds each hop
    from the first request's headers and `reqwest` strips `Authorization`
    only when a hop changes host or port *from the hop before*, so a second
    hop on the same CDN would get it back. Held tokens carry **no expiry**:
    one that aged out comes back as a fresh challenge.
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
- `errors.rs` — `transport_failure`, `send_failure` (an address refusal
  anywhere in the chain is `Refused` with `NOT_PUBLIC`; a redirect-policy
  refusal is `Refused` with the policy's own wording), `rate_limited`, `unreadable` (a body
  within bounds that is not the document expected: `Unavailable`),
  `status_failure` (a `404` that is an answer is handled where the request
  is made and never reaches it — a `404` that is not an answer, such as a
  later referrers or tag page or the token endpoint, does, and is
  `Refused`; `429`, a quota-exhausted `403`
  and `5xx` → `Unavailable`; everything else → `Refused`).

## Hard invariants — do not break

1. **A registry credential is presented only where it was registered, and
   nowhere else.** An `OciRegistry` holds at most one — an operator's
   registry credential, never the platform repository's GitHub App
   credential — and presents it only for the repositories it was registered
   for (every other repository is read anonymously), only to the realm its
   kind's rule allows (a challenge naming another is refused before anything
   is sent), and never across origins (`Basic` only to a `distribution`
   registry's own origin; a blob hop off the registry's origin carries
   nothing). Nothing is presented before a challenge asks for it. It never
   reaches `Display`, `Debug`, a log, an error detail or a response. A
   credential its realm refuses is marked, and not presented again by any
   client sharing its mark; replacing it is building a new client with a
   fresh mark, so no token or refusal outlives it. `HelmCharts` sends no
   credential at all.
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
5a. **A registry an operator registered is read at public addresses only.**
   Under `AddressPolicy::PublicOnly` every name is resolved through
   `PublicResolver`, on every connection, and dialled only at public
   addresses; a URL naming an IP literal — endpoint, realm, API redirect,
   blob hop — is refused before its request. Every such refusal reads
   `NOT_PUBLIC`, wherever it happened. Loopback counts as public only
   through the test switch, only over the test transport.
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
  It challenges by default as GHCR does (`Bearer`, a realm on its own
  origin), or with `Basic`, or not at all; `start_with_realm()` runs the
  realm on a server of its own; `expect_credential` has the realm (and a
  `Basic` registry) accept only one credential; `realm_refuses()` answers
  every credential `401`; `realm_declines_scopes()` and
  `realm_declines_anonymous_unscoped()` answer `403 DENIED` as GHCR's realm
  does; `cdn_by_name()` names the CDN `localhost` for a registry held to
  public addresses. A registry held to public addresses is served
  under the name `localhost` with the loopback switch on.
- `OciRegistry::path(repository)` strips a leading `{registry_host}/` if
  present, so a manifest can name a repository as
  `ghcr.io/fieldstatenz/saas-fabric` while a test points `base_url` at a
  bare loopback socket without renaming every fixture repository.
