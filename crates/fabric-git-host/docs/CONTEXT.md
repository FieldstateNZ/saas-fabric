# fabric-git-host — LLM context

Authenticates to a Git host as a GitHub App: mints, caches and expires
installation tokens. In neither plane (see
`docs/architecture/crate-dependencies.md`) — shared *within* the control
plane by two otherwise-separate adapters. Depends only on `fabric-core` plus
`jsonwebtoken`, `reqwest`, `serde`, `serde_json`, `thiserror`, `time`,
`tokio` (`sync` feature only).

## Public surface (all re-exported from `lib.rs`)

- `GitCredential` — `Token(String)` (presented as-is; test/non-GitHub use
  only) | `App { app_id: String, installation_id: String, private_key: String }`
  (production posture). Constructors `token(impl Into<String>)` and
  `app(app_id, installation_id, private_key)`. Hand-written `Debug` prints
  only the posture name (`GitCredential::App(redacted)` /
  `GitCredential::Token(redacted)`) — never the key or ids, even nested
  inside another `#[derive(Debug)]` struct.
- `BearerSource` — `new(credential: GitCredential, api_base_url: String,
  clock: Arc<dyn Clock>) -> Self`. `async fn bearer(&self, http:
  &reqwest::Client) -> Result<String, TokenError>` — returns the cached
  token if still usable, else mints one under a lock held across the mint
  (concurrent callers share one exchange). `async fn invalidate(&self)` —
  drops the cached token so the next `bearer()` call mints fresh; call after
  a `401` from the host, since a token can stop working before its stated
  expiry (App uninstalled, key rotated, installation suspended).
- `sign_app_assertion(app_id: &str, private_key: &str, now_unix: u64) ->
  Result<String, TokenError>` — builds the RS256 App JWT presented to the
  installation-token endpoint. `iat = now_unix - 60`; `exp = iat + 60 + 540`
  — a 10-minute span from `iat` to `exp` (GitHub's stated ceiling, reached
  exactly, not undercut), while `exp` itself lands only 9 minutes past the
  real current time because `iat` was already backdated. Shared with
  provisioning flows that mint a token to *prove* an installation works
  before recording it.
- `TokenError` (`thiserror`, `Debug`, `Clone`, `PartialEq`, `Eq`) —
  `NotPermitted` (key/installation/permissions refused — needs a human),
  `Unavailable { detail: String }` (endpoint unreachable, failed, or
  rate-limited — retry), `Rejected { detail: String }` (host understood and
  refused — no retry fixes it). `detail` never carries an upstream body or a
  credential.

## Internal modules

- `bearer.rs` + `bearer/` — `BearerSource`, and its three private
  submodules:
  - `assertion.rs` — `sign_app_assertion`, `JWT_LIFETIME_SECONDS` (540 = 9
    * 60), `JWT_BACKDATE_SECONDS` (60).
  - `lifetime.rs` — `usable_for(expires_at: &str, now_unix: u64) ->
    Duration`: parses the host's RFC 3339 `expires_at`, subtracts a 5-minute
    `MARGIN`, clamps to `[MINIMUM (30s), MAXIMUM (55min)]`. Unparseable input
    → `UNREADABLE` (5 minutes). Already-expired-by-this-clock → `MINIMUM`,
    never zero.
  - `wire.rs` — `InstallationToken { token, expires_at }`, `pub(super)`, no
    `Debug` derive (would print the token).
- `credential.rs` — `GitCredential` and its hand-written `Debug`.
- `errors.rs` — `TokenError`, plus `pub(crate)` classifiers
  `transport_failure(error: &reqwest::Error) -> TokenError` and
  `status_failure(status: StatusCode, headers: &HeaderMap) -> TokenError`
  (the latter checks `x-ratelimit-remaining` to distinguish a rate limit —
  `Unavailable` — from a genuine `401`/`403` refusal — `NotPermitted`).

## Hard invariants — do not break

1. **`GitCredential`'s `Debug` never prints a key, an app id, or an
   installation id** — only the posture name. This is what stops a config
   struct's derived `Debug` from leaking a credential the first time it
   reaches a log line.
2. **A minted token's usable lifetime is measured monotonically, never in
   wall-clock time.** `lifetime::usable_for` returns a `Duration`; the caller
   (`BearerSource::bearer`) adds it to `clock.now()` (monotonic) to get the
   cache deadline — never to a wall-clock instant, which would move under an
   NTP step.
3. **`invalidate()` exists because a stated expiry is not a guarantee.**
   Every adapter calling into this crate must invalidate on a `401`, or a
   revoked/rotated credential is presented on every request until its local
   deadline, failing identically each time.
4. **`BearerSource::bearer`'s lock is held across the network call**, not
   released before it. This is what makes two concurrent sweeps mint one
   token between them rather than two.
5. **No repository, path, or API detail appears anywhere in this crate.** It
   knows one endpoint (`{api_base_url}/app/installations/{installation_id}/access_tokens`)
   and nothing about what a caller does with the resulting bearer.

## Notes

- `TokenError`'s three variants are deliberately not collapsed: a caller
  branches on them to decide "look at the App" vs. "retry" vs. "give up",
  and each adapter (`fabric-platform-git`, `fabric-client-git`) maps this
  crate's `TokenError` into its own error enum via a hand-written `From`
  impl rather than reusing it directly.
- No `tests/` directory in this crate, and no inline unit tests on
  `BearerSource` or `sign_app_assertion` — only `bearer::lifetime::usable_for`
  (9 cases, exercising the margin, floor, ceiling and unreadable-input
  paths) and `GitCredential`'s `Debug` (4 cases) carry `#[cfg(test)]`
  modules. `BearerSource` and `GitCredential` are otherwise exercised through
  the two adapters that depend on this crate.
- Workspace-wide, no `git2`/`gix`/`gitoxide`/`kube*` client may be linked
  (`scripts/check_architecture.py`, `CONTROL_PLANE_CLIENTS`) — this crate
  and its adapters speak the host's HTTPS API only.
- `scripts/check_architecture.py`'s `expected` table pins this crate's only
  internal dependency to `fabric-core`; both `fabric-client-git` and
  `fabric-platform-git` depend on it and not on each other.
