# fabric-client-git — LLM context

The Git-backed `ClientRepository`. Depends on `fabric-core`,
`fabric-client-model`, `fabric-control-plane`, `async-trait`, `base64`,
`reqwest`, `serde`, `serde_json`, `tracing`. Event domain `13`.

**Only `fabric-control-plane-api` may depend on this crate.** It depends
*inward* on `fabric-control-plane` for the port it implements.

## Public surface

Three items, deliberately:

- `GitClientRepository::new(&GitRepositoryConfig, GitCredential) -> Result<Self, String>`.
  Implements `fabric_control_plane::ClientRepository`.
- `GitRepositoryConfig { api_base_url, owner, repository, branch, path_prefix,
  document_file, token_ref, committer_name, committer_email, http_timeout_seconds }`
  + `validate()`. All non-secret.
- `GitCredential::new(impl Into<String>)`. No `Display`; `Debug` prints
  `GitCredential(redacted)`. `expose()` is `pub(crate)`.

Everything else — `github::*` — is `pub(crate)`.

## Internal shape

- `github::GitHost` — `list_directory()`, `read_document(&ClientId)`,
  `write_document(&ClientId, &str, &ClientRevision, &str)`, `describe()`.
  Headers: `Accept: application/vnd.github+json`,
  `X-GitHub-Api-Version: 2022-11-28`, a `User-Agent` (the host refuses requests
  without one), and bearer auth.
- `github::wire` — `ContentsEntry`, `PutContents` (carries `sha`, which is what
  makes the write conditional), `Committer`, `PutContentsResponse`.
- `github::errors` — `transport_failure`, `status_failure(op, status, headers, Option<&ClientId>)`.
  Reads `x-ratelimit-remaining` to tell a rate limit from a refused token.
- `repository::list` — directory listing, then one read per client.
- `repository::write` — render, re-parse, then conditional PUT.

## Hard invariants — do not break

1. **No Git library may enter the graph.** `git2`/`gix`/`gitoxide` are banned
   workspace-wide by the architecture check, and this crate is the reason
   somebody might reach for one.
2. **A revision is the blob `sha`, never a commit sha.**
3. **`PutContents.sha` is always sent.** Omitting it makes every write an
   unconditional overwrite.
4. **The rendered document is re-parsed before the write.**
5. **No response body and no token in any error.**
6. **A `404` while listing is not `NotFound`.** There is no client to name, and
   reporting one would send an operator looking for the wrong thing.
7. **A document that will not parse fails the listing.** Do not "improve" this
   into a skip.
8. **`ClientId` is a validated DNS label**, which is why it is interpolated into
   a path without escaping. Keep it that way.

## Design notes

- The host wraps base64 at 60 columns; `decode` strips whitespace before
  decoding, which is otherwise a confusing intermittent failure.
- The commit message is `{summary}\n\nRequested-by: {requested_by}\n`.
- `tests/support/fake_git_host.rs` is **stateful**: it stores files, moves
  hashes on accepted writes, and refuses a stale hash. A stub returning a canned
  `409` would prove nothing about whether the adapter sends the hash at all.
