# Observed `ndc-postgres` v3.1.0 documents

Everything in this directory is **observed**, not written -- including the
three response files flagged below as reconstructed from the plan's quoted
record rather than a saved capture (`mutation-insert-affected-only.json`,
`mutation-delete-other-tenant.json`, `error-parse-422.json`): each still
describes a response the connector actually produced and that was
transcribed immediately afterward, not a guess at one. Most files are a
response a real `ndc-postgres` connector produced when driven against a real
PostgreSQL database, captured on 2026-09-06 (issue #62, slice 1); the three
`request-*.json` files run the other direction -- the exact request bodies
that produced three of those responses, extracted verbatim from the capture
script rather than retyped. None of it is a guess at what the wire protocol
should look like — that is the failure mode this issue exists to close (see
`docs/decisions/0004-write-support-in-the-first-release.md` and
`docs/verification.md`'s "no test has spoken to a running connector").

## What was running

| | |
|---|---|
| Connector image | `ghcr.io/hasura/ndc-postgres:v3.1.0` |
| Index digest | `sha256:f91910ef5107aa80d31d82639e149b7f41f4a5bb3af9a369397d7d5965d79a57` |
| Licence | Apache-2.0 -- verified in `docs/decisions/0001-ndc-as-connector-boundary.md`; consumed over HTTP, never linked |
| Database | `postgres:16-alpine` |
| Connector platforms | `linux/arm64`, `linux/amd64` (multi-arch; no emulation needed) |

## The table

A single physical `articles` table, deliberately shared by two tenants under
the same logical `id` -- the shape the tenant-isolation predicate exists to
scope:

```sql
CREATE TABLE articles (
  id         text    NOT NULL,
  tenant_key text    NOT NULL,
  title      text    NOT NULL,
  body       text,
  PRIMARY KEY (id, tenant_key)
);

INSERT INTO articles (id, tenant_key, title, body) VALUES
  ('1', 'tenant-acme-482',   'Acme Handbook',   'acme body'),
  ('1', 'tenant-globex-915', 'Globex Playbook', 'globex body'),
  ('2', 'tenant-acme-482',   'Acme Second',     NULL);
```

Two rows share the logical id `"1"`; one row (`id "2"`) has a `NULL` body.

## How the checked-in schema documents were produced

`ndc-postgres` ships `/bin/ndc-postgres-cli` in the same image. Both
`configuration.json` variants it produced against the live database above are
checked in under `crates/fabric-ndc-acceptance/tests/fixtures/ndc-postgres-v3.1.0/`,
not here -- that crate mounts them into a running connector as part of its
composed acceptance test; nothing under this directory reads either one. See
that crate's own fixture README for the exact `ndc-postgres-cli` commands,
`mutationsPrefix`/`mutationsVersion`, and how the named configuration's
`dynamicSettings` block was added by hand afterwards (the CLI has no way to
discover an operator's named-connection topology from the database alone).

`schema-static.json` and `schema-named.json` below are that same connector's
own live `GET /schema` responses to the two configurations, respectively.
`schema-named.json` in particular is what the connector computed from the
hand-added `dynamicSettings` block -- not something written by hand itself.

## The `/schema` and `/capabilities` documents

`capabilities.json`, `schema-static.json` and `schema-named.json` are each a
full, verbatim `GET /capabilities` or `GET /schema` response body, captured
with `curl` against the running connector (`X-Hasura-NDC-Version: 0.2.4`).
They are complete documents, not excerpts reconstructed from a summary --
the plan's raw captures already contained the full bodies.

## The query, mutation and error captures

Most of the small per-scenario files below are the **exact response body**
`curl` received, with only the trailing `HTTP <code>` status line (added by
`curl -w`) stripped so the file is parsable JSON on its own. The request that
produced each is described here rather than duplicated in every file, except
the two mutation requests a unit test pins the adapter's own output against
byte-for-byte (`wire::mutation_fields`'s and `translate::mutation`'s tests):
those are checked in as their own `request-*.json` fixtures, extracted from
the plan's `probe6.sh`, so the test reads the accepted body from a file
instead of repeating it as an inline literal. A third `request-*.json` file,
`request-delete-other-tenant.json`, is checked in the same verbatim way but
is not read by any test today; see its table row below for why it is kept
anyway. Every other request body here is preserved only in the planning
scripts this issue's plan references (`query_probes.sh`, `probe4.sh`,
`probe5.sh`, `probe6.sh`, `named_mode.sh`).

| File | Verbatim? | Request | Status |
|---|---|---|---|
| `query-isolated-acme.json` | Yes | `id = "1" AND tenant_key = "tenant-acme-482"` | 200 |
| `query-isolated-globex.json` | Yes | same, `tenant_key = "tenant-globex-915"` | 200 |
| `query-fields-absent.json` | Yes | `fields` omitted, `limit: 1` | 200 |
| `mutation-insert-no-fields-400.json` | Yes | `insert_articles`, `fields: null` | 400 |
| `mutation-insert-ok.json` | Yes | `insert_articles`, `fields` asking `affected_rows` and `returning` | 200 |
| `error-unknown-operator.json` | Yes | a predicate using operator `equals` (not a real one) | 400 |
| `request-insert-returning.json` | Yes | the request that produced `mutation-insert-ok.json` | -- |
| `request-insert-affected-only.json` | Yes | the request that produced `mutation-insert-affected-only.json` | -- |
| `request-delete-other-tenant.json` | Yes | the request that produced `mutation-delete-other-tenant.json` | No — read by no test; kept as the observed shape F3's follow-up must produce |

The three `request-*.json` files are requests, not responses, so "Status"
does not carry an HTTP code for any of them -- all three are extracted
verbatim from `probe6.sh`'s `curl -d` bodies, minified onto one line to match
this directory's other files. Two of the three are read back by a unit test
(see above); the column says so for the one that is not.

`mutation-insert-ok.json` keeps the odd `"result" : {` spacing exactly as
`ndc-postgres` emitted it -- this file was `tee`d straight from `curl` with
no reformatting pass, so the spacing is the connector's own, not a transcription
artefact.

### Two captures reconstructed from the plan's record, not from a saved file

Two probes were run and their result quoted in the planning document
(`plan.md` §2.8), but the terminal output was not `tee`d to a file at capture
time, so there is no raw byte-for-byte capture to check in. These two files
reproduce the response bodies **exactly as quoted in the plan**, which was
itself written immediately after observing the real run -- but they are
reconstructions of that record, not a second-hand capture, and are flagged
as such rather than presented as indistinguishable from the files above:

- `mutation-insert-affected-only.json` -- `insert_articles` with `fields`
  asking only for `affected_rows` (request: same insert as
  `mutation-insert-ok.json`'s, minus the `returning` field selection --
  checked in verbatim as `request-insert-affected-only.json`).
- `mutation-delete-other-tenant.json` -- `delete_articles_by_id_and_tenant_key`
  keyed to a real row, with a `pre_check` predicate scoping to the *other*
  tenant (checked in verbatim as `request-delete-other-tenant.json`).
  `affected_rows: 0`, and the row was confirmed still present by a
  follow-up query -- the isolation guarantee holding on the write path.

Only the **response** side of those two is a reconstruction; the request
that produced each was `tee`d nowhere, but is preserved verbatim in
`probe6.sh` itself, which is where `request-insert-affected-only.json` and
`request-delete-other-tenant.json` were extracted from -- those two files are
exact, not reconstructed.

`error-parse-422.json` is reconstructed the same way as the two mutation
responses above, from the plan's quote of the response to posting
`{"nonsense":true}` (a body that is not an NDC request at all) to `/query`.

## Regenerating this capture

The scripts that produced every file here (`fetch_image.sh`, `up.sh`,
`gen_config.sh`, `named_mode.sh`, `query_probes.sh`, `probe2.sh` through
`probe6.sh`, `serve_and_query.sh`) are kept with the M2 planning material for
issue #62. Pin the image by digest, not by tag, when reproducing this: `v3.1.0`
was the newest release tag at capture time, but tags move and digests do not.
