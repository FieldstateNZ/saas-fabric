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
`x IN (a,b)` and `x = a OR x = b` are the same predicate.

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
