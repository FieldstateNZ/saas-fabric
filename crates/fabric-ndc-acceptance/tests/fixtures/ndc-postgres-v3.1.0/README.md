# `ndc-postgres` v3.1.0 configurations

Two connector configurations, both generated once by the real
`/bin/ndc-postgres-cli update` shipped inside
`ghcr.io/hasura/ndc-postgres:v3.1.0` (see
`crates/fabric-ndc-acceptance/tests/support/images.rs` for the pinned
digest). `configuration-static.json` is that CLI output verbatim, never
hand-written. `configuration-named.json` starts from the same CLI output,
with its `dynamicSettings` block added by hand afterwards — see
"Regenerating" below for exactly what was added and why: the CLI introspects
a database schema, not an operator's named-connection topology, so that one
block genuinely cannot come from introspection. Hand-writing anything the
CLI *can* introspect would reintroduce exactly the documentation-derived
guessing issue #62 exists to end (see `m2-ndc/plan.md` §1.1, assumption
A12); the `dynamicSettings` block is the one documented exception, not a
precedent for more.

Both were introspected against one running `postgres:16-alpine` holding
exactly the table `crates/fabric-ndc-acceptance/tests/support/postgres.rs`
seeds:

```sql
CREATE TABLE articles (
  id         text NOT NULL,
  tenant_key text NOT NULL,
  title      text NOT NULL,
  body       text,
  PRIMARY KEY (id, tenant_key)
);
```

## `configuration-static.json`

`connectionSettings.connectionUri` only — declares no request-level
arguments. Used for the version/schema smoke test and the startup-refusal
proof (a configuration that asks for name routing must be refused, since
this connector never declares it).

## `configuration-named.json`

Identical `metadata`, but `connectionSettings.dynamicSettings` set to:

```json
{
  "mode": "named",
  "connectionUris": { "map": { "shared-au-east": { "variable": "CONNECTION_URI" } } },
  "fallbackToStatic": false,
  "eagerConnections": false
}
```

`fallbackToStatic: false` is the load-bearing line: with it `false`, a
request missing `connection_name` is a `400`, not a silent fall-through to
the static connection. Declares `connection_name` as a request-level
argument for both queries and mutations.

## Neither file embeds a connection string

Both name `connectionSettings.connectionUri.variable` (and, in the named
file, `connectionSettings.dynamicSettings.connectionUris.map."shared-au-east".variable`)
as `CONNECTION_URI` — the connector reads the physical connection from that
environment variable at container start, not from this JSON. The harness
never rewrites these files; it assembles `CONNECTION_URI` fresh per test run
from that run's own postgres container name, user, and password and passes
it with `-e CONNECTION_URI=...` — see `support/connector.rs`.

## Regenerating

```bash
docker network create m2ndc
docker run -d --name m2ndc-pg --network m2ndc \
  -e POSTGRES_USER=fabric -e POSTGRES_PASSWORD=fabric -e POSTGRES_DB=fabric \
  postgres:16-alpine
# wait for pg_isready, then create the table above and seed it (rows are
# irrelevant to introspection; only the schema matters)

docker run --rm --network m2ndc -e CONNECTION_URI='postgresql://fabric:fabric@m2ndc-pg:5432/fabric' \
  -v "$PWD:/etc/connector" --entrypoint /bin/ndc-postgres-cli \
  ghcr.io/hasura/ndc-postgres:v3.1.0 --context /etc/connector initialize

docker run --rm --network m2ndc -e CONNECTION_URI='postgresql://fabric:fabric@m2ndc-pg:5432/fabric' \
  -v "$PWD:/etc/connector" --entrypoint /bin/ndc-postgres-cli \
  ghcr.io/hasura/ndc-postgres:v3.1.0 --context /etc/connector update
```

`initialize` writes the file the first time; `update` re-introspects an
existing one. Copy the result to `configuration-static.json`, then apply the
`dynamicSettings` block above (only) to produce `configuration-named.json`.
