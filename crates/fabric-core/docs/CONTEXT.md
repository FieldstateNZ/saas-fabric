# fabric-core — LLM context

Shared kernel. Zero I/O. Depends only on `serde` + `thiserror`. Every other
workspace crate depends on it.

## Public surface (all re-exported from `lib.rs`)

- `TenantId` — newtype over `String`. `try_new` enforces DNS-label rules:
  1..=63 bytes, `[a-z0-9-]`, must start and end alphanumeric. Serde uses
  `try_from = "String"` so deserialisation validates too. `Ord + Hash`.
- `DataSourceName` — the **logical** name an application's resource is bound
  to (`primary`, `audit`). Identifier rules: 1..=63 bytes, `[A-Za-z0-9_-]`,
  must start with an ASCII letter.
- `DataSourceId` — the **DataSource resource** that logical name resolves to
  (`sql-au-east-03`). Same rules. **Do not confuse the two**: one is intent,
  the other is a configured physical destination (ADR 0003).
- `LogicalResourceName` — same rules as `DataSourceName`. Separate type so the
  compiler stops you passing a resource where a data source belongs.
- `BindingRevision` — newtype over `u64`. `ZERO`, `new`, `get`, `next`
  (saturating). `Ord` is the point of the type.
- `IdentifierError` — `Empty` / `TooLong` / `DisallowedCharacter` / `BadBoundary`.
  Reports the offending character, never the whole value (it can be
  attacker-influenced).
- `event_id(domain_id, EventType, number) -> u32` — `domain*1000 + type*100 + n`.
- `EventType` — `Success=0, Validation=1, Error=2, Warning=3, Debug=4, Trace=5`.
- `Clock` trait — `now() -> Instant` (monotonic, for durations) and
  `now_unix_seconds() -> u64` (wall clock, for records). `SystemClock` is the
  production impl; `SystemClock::shared()` returns `Arc<dyn Clock>`.

## Internal modules

- `ids::slug` — `parse_dns_label` and `parse_identifier`, both `pub(crate)`.
  Shared by the newtypes; the two rule sets are deliberately separate functions
  rather than one parameterised one.

## Domain ID allocation for `event_id`

| ID | Crate |
|---|---|
| 1 | `fabric-identity` |
| 2 | `fabric-tenant-runtime` |
| 3 | `fabric-data` |
| 4 | `fabric-data-api` |

## Invariants to preserve

- No I/O, no async, no domain logic in this crate.
- Identifier newtypes have exactly one fallible constructor; there is no
  `from_unchecked` and there must not be one.
- `BindingRevision::next` saturates. Wrapping would make a newer binding compare
  as older, which would strand a migration.
