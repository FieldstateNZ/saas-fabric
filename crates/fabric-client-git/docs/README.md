# fabric-client-git

The Git-backed desired-state repository. Every Git-hosting detail in this
platform lives inside this crate.

```text
ClientService               client, identity, revision
      ↓
ClientRepository            the port — still SaaS Fabric's words
      ↓
GitClientRepository         ← the translation happens here, and only here
      ↓
Git hosting contents API    paths, blobs, commits, branches
```

## Optimistic concurrency, not last-writer-wins

A revision is the stored file's **blob hash**, and a write carries the hash the
caller believed it was editing. The hosting API applies the write only if that
hash is still current, and answers `409` otherwise.

The check is therefore **atomic on the server**, not a read-then-write in this
process that a second control-plane replica could interleave with. ADR 0008 is
explicit that a concurrent edit must be refused rather than merged or
overwritten.

The blob hash rather than the commit, and the difference matters: a commit
touching another client's document moves the branch but not this file's hash, so
revisions built from commits would make every operator's edit conflict with
every other operator's unrelated edit.

## No Git library, deliberately

This crate speaks the hosting provider's contents API over HTTPS. It does not
link `git2`, `gix`, or anything else that would put a Git implementation in the
workspace's dependency graph — and `scripts/check_architecture.py` fails the
build if one appears.

That check is what keeps "Git is never in the request path" a structural fact
about *every binary the workspace builds*, rather than a claim about the runtime
crates that a control plane could quietly undermine.

It also means the platform needs no working copy, no clone, and no disk: a
control-plane replica is stateless, and two of them behave the same way because
neither has a local view to diverge.

## Layout and cost

A client's document is `{path_prefix}/{client id}/{document_file}` — by default
`clients/acme/client.yaml`. Configurable, because the desired-state repository
is a separate repository with its own conventions and a platform that hard-coded
them could not follow a change to them without a release.

Listing is **one request per client**, plus one for the directory: the contents
API returns a directory listing without file contents. At the scale SaaS Fabric
operates — tens of clients, one operator, a screen refreshed by hand — that is a
few hundred milliseconds and no cache to invalidate. It is written down rather
than left to be discovered, because the fix when it stops being acceptable is a
different API, not a tweak here.

## What happens to a broken document

| Situation | Answer | Because |
|---|---|---|
| A directory not named like a client | skipped, logged | it is not a client directory |
| A client directory with no document | skipped, logged | not a broken client — not a client at all |
| A document that will not parse | **fails the whole listing** | see below |

Skipping the third was considered and rejected. A client silently missing from
the operator console is the worst possible presentation of a broken document:
everything looks fine, and the one client that needs attention is the one nobody
can see. Failing names the client and the rule it broke, which is what an
operator can act on.

## "Malformed desired state is not committed"

The document handed in is valid by construction — `ClientDocument` has no
constructor that produces an invalid one. That is an argument, though, and this
is a commit to the platform's source of truth, so the argument is checked: the
rendered text is parsed with exactly the code that will read it back, and a
failure aborts *before* the write.

`desired_state_repository.rs` asserts the property directly — it decodes what
reached the host and parses it.

## The credential is an App, not a token

The production posture is a **GitHub App** installed on the desired-state
repository and nothing else, with `contents: read` and `contents: write`.

The platform holds the App's private key. Every request needs a bearer, so the
adapter signs a short-lived assertion with that key, exchanges it at
`/app/installations/{id}/access_tokens`, and presents the *minted* token — which
GitHub expires after an hour, and which the adapter caches for fifty minutes.

The distinction that matters: the durable secret is a key that is never sent
anywhere, and the thing that *is* sent expires on its own. A personal access
token inverts both — it is the durable secret, it is sent on every request, and
it outlives whoever issued it. `GitAuthConfig::Token` exists for a host that is
not GitHub and for tests that drive a socket; it is not what a deployment runs.

`installation_id` is configured rather than discovered. Looking it up would mean
granting the App enough scope to enumerate its own installations, and repeating
that lookup on every process start for a value that changes only when somebody
reinstalls the App.

## Attribution

Commits are authored by the platform's machine identity, because that is who
holds the token. Without more, Git's history would record only that SaaS Fabric
changed a client and never who asked — so the commit message carries a
`Requested-by:` trailer.

That is a *second* copy of the audit record, not the only one. The control plane
emits its own event, because a refused write leaves no commit and is still worth
knowing about.

## Failures

| Reported | From | Because |
|---|---|---|
| `NotFound` | `404` for a named client | that client is absent |
| `Unavailable` | `404` while listing | the repository's layout has moved, which is not "no clients" |
| `Unavailable` | `403` with no quota left | a rate limit is transient |
| `NotPermitted` | `401`, `403` otherwise | the platform's token is wrong |
| `Conflict` | `409`, `422` | the write's precondition did not hold |
| `Unavailable` | `5xx`, `429`, transport | the host is unwell |
| `Rejected` | other `4xx` | no retry fixes it |

Rate limits and refused tokens both arrive as `403`, and telling them apart
matters: reporting a rate limit as a refused credential sends an operator to
rotate a secret that is perfectly fine.
