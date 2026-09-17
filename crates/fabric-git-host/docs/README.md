# fabric-git-host

Authenticates to a Git host as a GitHub App. The one place installation
tokens are minted, cached and expired — nothing else in the workspace touches
a JWT for this purpose.

This crate sits in neither plane (see
[`docs/architecture/crate-dependencies.md`](../../../docs/architecture/crate-dependencies.md)).
It is shared **within** the control plane by two adapters that must otherwise
stay completely apart.

## Why this crate exists

SaaS Fabric talks to a Git host for two independent reasons — a client's
desired state, and the platform's own desired state — and
[ADR 0011](../../../docs/decisions/0011-the-platform-creates-its-own-git-application.md)
requires them to be **separate GitHub Apps**: independently installable,
configurable and removable. Two integrations, two Apps, two repositories, two
credentials. `fabric-client-git` and `fabric-platform-git` are those two
adapters, and there is deliberately no edge between them.

What is *not* two things is how a private key becomes a bearer token. That is
one exchange with one endpoint (`POST /app/installations/{id}/access_tokens`),
guarded by a cache whose correctness is genuinely subtle: a stated expiry that
has to be read rather than assumed, a wall-clock remaining lifetime measured
against a monotonic deadline, and an invalidation path for a token that stops
working before its stated life is up. Two copies of that logic would be two
copies of the platform's credential-minting code, and a fix to one could
silently miss the other. So the *credential* is shared and the *integrations*
are not — each adapter depends on this crate, and not on each other.

## Key concepts

- **`GitCredential`** — how the platform authenticates. `App { app_id,
  installation_id, private_key }` is the production posture: the platform
  holds a private key and mints a short-lived installation token roughly
  hourly, so the durable secret is never itself a bearer. `Token(String)`
  presents a value unchanged; it exists for tests that drive a real socket
  and for a non-GitHub host, and is not something to deploy. Both variants
  have a hand-written `Debug` that prints only which posture is in use
  (`GitCredential::App(redacted)`) — never the key, never the ids. A `String`
  field one `{:?}` away from a log line is exactly how this kind of secret
  leaks, and the redaction is on the type so it cannot be forgotten at a call
  site.
- **`BearerSource`** — mints, caches and invalidates the bearer for each
  request. `bearer(&self, http)` returns a cached token if one is still
  usable, or mints a fresh one under a lock held across the mint (so two
  concurrent callers share one exchange rather than both hitting the host).
  `invalidate()` discards the cached token immediately, for a caller that got
  a `401` — a token can stop working before its stated expiry (the App
  uninstalled, the key rotated, the installation suspended), and without this
  the platform would keep presenting a dead token until its local deadline
  passed.
- **Token lifetime (`bearer::lifetime::usable_for`)** — the host's stated
  expiry is read, not assumed to be an hour just because that is what GitHub
  documents today. The usable duration is clamped between a 30-second floor
  and a 55-minute ceiling, with a 5-minute margin subtracted for clock skew
  and the round trip. An unreadable or malformed expiry falls back to 5
  minutes rather than 0 (which would mint on every call) or something long
  (which would extend a guess). The result is a *duration*, turned into a
  **monotonic** deadline by the caller — never a wall-clock deadline, which
  would move under an NTP step and either expire every cached token at once
  or extend one past its real life.
- **`TokenError`** — three variants that lead three different places:
  `NotPermitted` (the key, installation or permissions were refused — a human
  needs to look at the App), `Unavailable` (the endpoint could not be reached
  or failed — wait and retry), `Rejected` (the host understood and refused
  the request — no retry fixes it). A `403` with an exhausted
  `x-ratelimit-remaining` header is classified as `Unavailable`, not
  `NotPermitted`, so a rate limit does not send an operator hunting for a
  broken secret.

## How the pieces fit

```text
GitCredential          what the platform was given
      |
BearerSource           mints, caches and expires an installation token
      |
Authorization: Bearer  what every call to the host carries
```

Each adapter (`fabric-client-git`, `fabric-platform-git`) owns a
`BearerSource` and calls `.bearer(&http)` before every request to the host,
and `.invalidate()` after a `401`. Neither adapter knows or cares which
repository the other is talking to — this crate never sees a repository name,
a path, or an API beyond the token endpoint.

## Getting started

```rust,ignore
use std::sync::Arc;
use fabric_core::SystemClock;
use fabric_git_host::{BearerSource, GitCredential};

let credential = GitCredential::app(app_id, installation_id, private_key_pem);
let bearers = BearerSource::new(credential, "https://api.github.com".to_owned(), SystemClock::shared());

let http = reqwest::Client::new();
let token = bearers.bearer(&http).await?;
// use `token` as a bearer against the host's API
```

On a `401` from the host, call `bearers.invalidate().await` before retrying —
the adapter's own request loop is responsible for that retry; this crate only
supplies and expires the token.

## Common tasks

- **Adding a new Git-host adapter** — depend on this crate for `GitCredential`
  and `BearerSource`, and nothing else here. Map `TokenError` into the
  adapter's own error vocabulary (see `fabric-platform-git`'s `errors.rs` for
  the pattern); do not reuse `TokenError` as the adapter's public error type,
  since "the credential was refused" means something different to each
  adapter's caller.
- **Testing against a real socket** — use `GitCredential::token(...)` to
  present a fixed value, or `GitCredential::app(...)` against a fake token
  endpoint that returns a controlled `expires_at`.

## Gotchas

- The crate is `fabric-git-host` (hyphen); the Rust identifier — and the
  `RUST_LOG` filter target — is `fabric_git_host` (underscore):
  `RUST_LOG=info,fabric_git_host=debug`.
- `sign_app_assertion`'s underlying `jsonwebtoken` error is deliberately
  dropped and replaced with `TokenError::NotPermitted`. The library's error
  messages can include the offending input, and the offending input here is
  the private key.
- The App JWT presented to the token endpoint has `iat` backdated 60 seconds
  and `exp` set to `iat + 60 + 540` seconds — a 10-minute span from `iat` to
  `exp`, exactly GitHub's documented ceiling, not under it. What the 9-minute
  constant (`JWT_LIFETIME_SECONDS`) actually buys is headroom measured from
  the *real* clock: because `iat` is already 60 seconds in the past, the
  token's `exp` is only 9 minutes ahead of the true current time even though
  its stated span from `iat` is a full 10 minutes. The 60-second backdate is
  what covers ordinary clock skew between a cluster node and GitHub's own
  clock.
- `BearerSource::bearer` holds its lock across the mint itself, not just the
  cache check. That is deliberate: it is what makes two concurrent callers
  share one exchange instead of both minting redundant tokens.
- This crate has no `tests/` directory of its own, and no inline
  `#[cfg(test)]` module on `BearerSource` or `sign_app_assertion` either —
  only `bearer::lifetime::usable_for` and `GitCredential`'s `Debug` carry
  unit tests. The rest of this crate's behaviour is proven through the two
  adapters that depend on it (`fabric-platform-git`, `fabric-client-git`),
  which drive `BearerSource` against fakes and real sockets in their own test
  suites.
- `scripts/check_architecture.py`'s workspace-wide ban on `git2`/`gix`/`kube*`
  clients (`CONTROL_PLANE_CLIENTS`) applies here as everywhere: this crate
  reaches the host over HTTPS only, never by linking a Git library.
