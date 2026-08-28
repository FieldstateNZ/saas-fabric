# fabric-core

The shared kernel. Every other crate in the workspace depends on this one, and
this one depends on nothing but `serde` and `thiserror`.

## What lives here

Only things that (a) more than one crate needs and (b) nothing in the platform
can reasonably disagree about:

| Type | Why it exists |
|---|---|
| `TenantId` | The value the entire runtime plane pivots on. Validated to a DNS-label character set so it is safe in a schema name, a pool key, and a metric label. |
| `LogicalDataSourceName` | A *logical* data source (`primary`, `audit`). Intent, never infrastructure. |
| `LogicalResourceName` | A logical resource an application addresses (`customers`). Arrives in the URL path, so it is validated on the way in. |
| `BindingRevision` | Monotonic revision on every runtime binding. Drives cache invalidation, migration cut-over, and diagnostics. |
| `event_id` / `EventType` | The structured event-ID scheme, so alert rules key off a stable number rather than log wording. |
| `Clock` / `SystemClock` | The time seam. Pool eviction is time-driven, and tests must not sleep. |

## The one idea to take away

**Parse, don't validate.** The identifier newtypes have exactly one fallible
constructor each, and it does the full character-set check. Once you hold a
`TenantId`, the check has already happened — there is no code path that produces
an unvalidated one.

This matters most at the SQL boundary. When a tenant is placed on a shared
database with per-tenant schemas, the tenant id ends up interpolated into a SQL
identifier, and SQL identifiers cannot be parameterised. A `String` would leave
that one careless `format!` away from injection. `TenantId` closes the path at
the point of parsing, and the `deserialising_runs_the_same_validation_as_the_constructor`
test pins the JSON boundary shut as well.

## Adding something here

Think twice. Everything added to this crate is compiled by every other crate and
paid for on every build. In particular:

- **No I/O.** No database calls, no HTTP clients, no filesystem access. Those
  belong in a domain crate.
- **No domain logic.** If only one crate uses it, it goes in that crate.

## Gotchas

- The crate is `fabric-core` (hyphen) but the Rust identifier is `fabric_core`
  (underscore). This catches everyone once, in `use` statements and in
  `RUST_LOG` filters.
- `TenantId::try_new` rejects uppercase rather than lowercasing it. Silently
  folding case would make `Acme` and `acme` the same tenant in the registry and
  different tenants in a log query — a genuinely nasty class of bug.
- `MAX_LENGTH` is 63 because that is simultaneously the DNS label limit and the
  PostgreSQL identifier limit. Raising it would break one or the other.
