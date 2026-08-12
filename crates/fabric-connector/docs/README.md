# fabric-connector

The neutral data-execution boundary.

## Why it exists

Something has to actually run a query against a real database. We deliberately
do not write that per dialect — the platform delegates to a connector process
that already speaks the datastore. See
[ADR 0001](../../../docs/decisions/0001-ndc-as-connector-boundary.md).

This crate is the seam that keeps that delegation a *choice*. Today the only
implementation is `fabric-connector-ndc`. Tomorrow it could be a native
PostgreSQL provider, and nothing above this crate would change.

## The one rule

**No protocol-specific or database-specific type may appear in this crate.**

No NDC types. No SQL. No driver types. No wire formats. If you are reaching for
one here, the abstraction is leaking — put it in the implementation crate.

That rule is the entire mechanism behind "NDC must be replaceable". It is easy
to state and easy to break by accident.

## What's in here

| Type | Role |
|---|---|
| `DataConnector` | The trait every backend implements. |
| `QuerySpec` / `MutationSpec` | Neutral operation model. |
| `Filter` / `ComparisonOperator` | Neutral predicate AST. |
| `SortField` / `Row` | Ordering and records. |
| `ExecutionTarget` | Where a tenant's data physically lives. |
| `IsolationModel` | Dedicated database, per-tenant schema, or discriminator (§18). |
| `ConnectionSelector` | Which connection within a connector. |
| `ConnectorCapabilities` | What a backend can actually do. |
| `ConnectorSchema` | What collections it holds. |
| `ConnectorRegistry` | Connector id → implementation. |
| `SecretRef` / `ResolvedSecret` / `SecretResolver` | Credentials (§21). |

## The two things most worth understanding

### 1. `for_target` is not optional

`QuerySpec::for_target` and `MutationSpec::for_target` are what make
discriminator isolation work. With a dedicated database or a per-tenant schema,
isolation is structural — the connection cannot see other tenants. With a
discriminator, every tenant's rows share a table and isolation exists **only**
because the platform adds a predicate.

Forget it on a read and you return every tenant's data. Forget it on a delete
and you destroy every tenant's data. Neither raises an error.

So there is exactly one place the predicate is produced
(`IsolationModel::tenant_predicate`) and one place it is applied (`for_target`),
and every route to a connector goes through it.

For mutations `for_target` does more than filter:

- **Inserts** get the discriminator *stamped* onto every row — a caller-supplied
  value for that column is overwritten, not merged, so nobody can insert into
  another tenant.
- **Updates** are both scoped and stamped, so a row cannot be moved to another
  tenant.
- **Deletes** with no predicate become "delete this tenant's rows", not "empty
  the table".

### 2. Capabilities fail closed

When a backend cannot express part of an operation, the operation is **refused**,
never approximated.

Degrading quietly — dropping a predicate the backend cannot handle — is merely
wrong in a single-tenant app. Here the dropped predicate might be the tenant
boundary, and the failure looks exactly like success: rows come back, status 200,
nothing logged. §28 requires failing closed.

## Gotchas

- The crate is `fabric-connector`; the log target is `fabric_connector`.
- `ResolvedSecret` has a `Debug` that prints `<redacted>`. That is deliberate —
  secrets reach logs by accident, not design (§29). Reaching the value needs
  `.expose()`, which is greppable.
- `ExecutionTarget` holds a `ConnectionSelector`, never a resolved credential.
  Resolution happens in the connector implementation, as late as possible.
- `QueryOutcome::total_count` of `None` means "not counted", **not** zero.
  Counting is expensive and connectors may decline.
- `for_target` does *not* rewrite the collection name to add a schema.
  Per-tenant schemas are selected by the connection (a named connection per
  schema, or `search_path` in a connection string). `IsolationModel::schema()`
  is there for implementations that need it.
- The five name newtypes come from one macro in `ids/identifier_newtype.rs`.
  That is to stop five hand-written copies drifting apart — one of them quietly
  losing its serde validation is exactly the bug it prevents.
