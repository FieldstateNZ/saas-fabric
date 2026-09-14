# fabric-connector-ndc

Speaks the Hasura NDC protocol (v0.2.13) as an implementation of
`fabric_connector::DataConnector`.

This is the **only** crate in the workspace that knows NDC exists. Nothing here
is re-exported upward. See
[ADR 0001](../../../docs/decisions/0001-ndc-as-connector-boundary.md).

## The licence situation — read this before adding a dependency

Hasura publishes `ndc-models`, a crate containing exactly the wire types in
`src/wire/`. **We do not use it.**

`hasura/ndc-spec` carries no licence at all: no `LICENSE` file anywhere in the
repository (a GitHub code search returns zero), no `license` field in its
manifests, no statement in its README. Absent a grant the default is all rights
reserved, so it cannot enter an Apache-2.0 platform.

This is worth internalising because the surrounding ecosystem *is* open source —
`ndc-sdk-rs` and `ndc-postgres` are both genuinely Apache-2.0. The
protocol-definitions crate is the one that isn't, and it is the one we would
have linked against.

So the wire types are hand-written from the published specification.
Implementing a protocol for interoperability is a different act from
incorporating someone's implementation of it.

**Connector processes are fine.** They are consumed over HTTP, never linked.
`ndc-postgres` v3.1.0 is Apache-2.0. Verify any new connector individually.

## How multi-tenancy works over NDC

Connectors are normally configured with one connection at startup — the opposite
of what per-tenant placement needs. NDC solves this with **request-level
arguments**, added in spec 0.2.4 for values that apply to a whole request.

| Tenant placement | Request argument | Value |
|---|---|---|
| Shared server | `connection_name` | Stable name from the runtime binding |
| Dedicated database | `connection_string` | Assembled from a resolved secret |

Named routing is strongly preferred: the credential stays inside the connector's
configuration instead of travelling in a request body. Both argument names are
configurable — nothing in the specification fixes them.

## Operator portability

Connectors name their own operators (`_eq`, `eq`, `equals`), but `/schema`
declares each one's *semantics*. `SchemaIndex` reads that at startup and builds
the mapping, so nothing hardcodes a vendor's spelling.

Two consequences worth knowing:

- **NDC has no "not equal".** Inequality is translated to a negated equality,
  which any connector supporting equality can serve.
- **An operator the connector never declares is refused**, not guessed.

`Filter::In` is the one place with a fallback: if the connector declares no `in`
operator, it becomes a disjunction of equalities. That is not a degradation —
`x IN (a,b)` and `x = a OR x = b` are the same predicate. Membership of the
*empty* set is refused rather than translated: it is the only shape that would
otherwise reach the wire without a single schema lookup, and nothing upstream
builds one on purpose.

## What a refusal is allowed to say

When translation refuses, the caller is told the **capability** and never the
identifier:

```
this operation is not supported: the equal comparison        ✅
this operation is not supported: comparing t.tenant_key ...  ❌
```

The reason is narrow and specific. `fabric-data-api` masks every connector error
except `Unsupported`, whose capability name it forwards into a 400 body. And
`to_expression` runs *after* `MutationSpec::for_target` / `QuerySpec::for_target`
have conjoined the tenant discriminator, so the predicate being refused may be
one the caller never wrote, over the very column holding their tenant boundary
up. Naming it would hand an application the shared table and its isolation key.

**This is now enforced by the type, not by this paragraph.** The published half
is a closed `fabric_connector::UnsupportedFeature` with nowhere to put a
collection, field, or procedure, and its `as_str` returns `&'static str`, so a
variant carrying runtime text could not be rendered at all. Getting it wrong is
a compile error.

The identifiers are still what an operator needs, so they ride alongside in a
`RefusalDetail` — deliberately not `Display`, so it cannot be formatted into a
message by accident. It reaches `ndc.operation_refused` (via
`ConnectorError::operator_message`) and `data_api.connector_refused`, and no
response body. Build refusals as
`UnsupportedFeature::…refused_because(detail)`.

## Mutations are procedure calls

This surprises people. Core NDC 0.2 has **no generic insert/update/delete**. The
only mutation operation is invoking a *procedure* the connector declares.
`ndc-postgres` generates `insert_customers` and friends from its configuration;
another connector might call them something else entirely.

So `CollectionProcedures` must be configured per collection, and a collection
with no mapping simply cannot be written to. The platform does not guess
procedure names — unwise for an insert, indefensible for a delete.

A mapping for an update or delete **must** declare `filter_argument`. Without
somewhere to put the predicate, the tenant scoping added by
`MutationSpec::for_target` would silently vanish and the write would reach every
tenant's rows. This is checked at config validation *and* again at translation.

Naming *an* argument is not enough — it has to be one the procedure actually
declares. NDC's `ProcedureInfo` carries `arguments: {name → ArgumentInfo{type}}`,
so `GET /schema` says exactly which names exist and which of them are
predicate-typed. `registration::procedure_arguments` checks every configured
`payload_argument` and `filter_argument` against that at startup, because an
argument a connector never declared is not promised to do anything: a mapping
saying `filter_argument: "where"` against a procedure declaring `filter` would
put the tenant predicate somewhere the connector ignores and leave the real
filter empty, and the delete that follows is unscoped and returns `200`.

An insert or update mapping must also declare `payload_argument`. That one fails
closed rather than open — every write is refused at translation — but it is
startup-detectable, and "every write 400s in production" is a bad way to find
out.

## Some procedures are keyed, not just filtered

A real `ndc-postgres` v3.1.0 does not generate `delete_articles(pre_check)`. It
generates `delete_articles_by_id_and_tenant_key(key_id, key_tenant_key,
pre_check)` — one procedure per collection's primary key, with the key columns
as their own required arguments alongside the predicate. A neutral
`MutationSpec::Delete { filter }` has no separate concept of "the key"; it only
has the predicate `MutationSpec::for_target` built.

`ProcedureBinding::key_arguments` bridges that gap: a `BTreeMap<String,
String>` from a physical field name (as it appears in the neutral `Filter`) to
the procedure's own argument for that field's value.

```toml
[connectors.procedures.articles.delete]
procedure = "delete_articles_by_id_and_tenant_key"
filter_argument = "pre_check"
key_arguments = { id = "key_id", tenant_key = "key_tenant_key" }
```

Three things follow from that, and all three are deliberate:

- **The key value is read off the predicate, never off the caller's payload,
  and never guessed.** After the full predicate is placed under
  `filter_argument`, translation looks for exactly one equality per key field
  among the predicate's *direct* clauses — a bare `Filter::Compare`, or a
  direct clause of a top-level `Filter::And`. It does not descend into `Or`,
  `Not`, or a nested `And` (a value that only holds along one branch is not a
  key value), and `Filter::In` is never accepted even with one value (a set
  the platform never collapsed to an equality is not one). No equality, or
  more than one with differing values, is refused — a keyed procedure cannot
  be called without its key, or against a contradictory one.
- **The tenant discriminator can reach the connector twice.** When a key field
  is also the discriminator column, its value goes out once as a key argument
  and once inside the predicate. That is defence in depth, not redundancy to
  tidy away — the two are built independently, so a mistake in one does not
  silently disable the other.
- **A procedure's required (non-nullable) argument that nothing in the mapping
  supplies is refused at startup.** `key_id` and `key_tenant_key` are plain,
  non-nullable arguments in the schema `ndc-postgres` publishes; before
  `key_arguments` existed, a mapping naming only `filter_argument` passed
  every check this crate ran and failed on the connector's first delete. That
  gap is now a boot failure: `registration::required_arguments` walks every
  argument the schema marks non-nullable and refuses to start if the mapping
  — payload, filter, or a key — supplies nothing for it.

An update's payload can need reshaping too. `ndc-postgres`'s
`update_columns` argument on a keyed update procedure does not take
`{"title": "new title"}`; it takes `{"title": {"_set": "new title"}}`, a
per-column operation rather than a bare value. `ProcedureBinding::payload_shape`
says which shape a mapping's payload argument expects — `values` (the
default, and the only shape an insert's `objects` argument is ever observed
to take) or `set_operations`. Only an update may declare `set_operations`: an
insert sends an array of row objects and a delete sends no payload, so
neither has anywhere for a per-column wrapper to go, and config validation
refuses both.

## When a call to a connector fails

The three transport failures this crate reports are **not interchangeable**, and
which one comes back depends on where in the exchange it broke:

| What happened | Error | Did the write happen? |
|---|---|---|
| Connect refused, DNS failure, connect timeout | `Unreachable` | No — nothing was sent |
| Request sent, no conclusive answer (total timeout after send, reset mid-flight, closed with no status) | `OutcomeUnknown` | Maybe |
| Success status read, body died mid-stream | `ResultLost` | Yes — only the result was lost |

`reqwest::Error::is_connect()` draws the first line; the response's own status
draws the second. Collapsing all three into `Unreachable` — which this crate did
— made a connect refusal indistinguishable from a write that had already
committed, and a retryable status on the latter tells a client to apply it
twice. `ConnectorError::effect()` is the API for this; `client/delivery_tests.rs`
pins each case against a real socket that counts what it applied before
misbehaving.

## Timeout ownership

A request passing through this crate is bounded by three separate clocks,
owned by three different places. `NdcConnectorConfig` only ever configures
the first one:

| Clock | Owner | Configured where |
|---|---|---|
| The HTTP call to the connector | This crate | `NdcConnectorConfig::http_timeout_seconds` (total) and `http_connect_timeout_seconds` (connect phase, a subset of the total) |
| Database execution inside the connector | The connector itself | The connector process's own configuration — `ndc-postgres`'s statement timeout, for example. Not settable from here. |
| The overall Data API request budget | The host application | `fabric-api`, which sees the whole request — auth, tenant resolution, and this HTTP call among other work — not just this one hop. |

`http_connect_timeout_seconds` must not exceed `http_timeout_seconds` — it is
a subset of the total call, not a second budget alongside it. Configuration
validation rejects the combination where it would.

## What happened to connection pooling (§22)

It moved into the connector process. That is a real consequence of this ADR, not
an oversight.

What this crate manages is the HTTP keep-alive pool to the connector. Pool
sizing, idle eviction, and connection recovery are now the connector's
configuration.

The §22 *objective* still holds, and arguably holds better: database connections
concentrate in a handful of connector processes rather than multiplying across
every application replica. The mechanism is just no longer ours to write.

## Gotchas

- **`src/wire/` does not follow house naming.** It mirrors NDC exactly —
  `predicate` not `filter`, `order_by` not `sort`. Check the spec before
  "fixing" a name.
- **Version pinning is strict on major.minor, lenient on patch.** Our wire types
  are hand-written against one spec version; a minor bump could add fields we do
  not read. A patch difference only warns.
- **`total_count` is always `None`.** NDC reports counts through aggregates,
  which the Data API does not use. `None` means "not counted", not zero.
- **An absent `rows` in a row set is an empty result, not an error.** It means
  the query asked for no fields.
- **Procedure results have no defined shape.** `translate/response.rs`
  recognises the common conventions (`{affected_rows, returning}`, a bare array,
  a bare object) and falls back conservatively.
- Log target is `fabric_connector_ndc`.
