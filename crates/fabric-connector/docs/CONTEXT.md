# fabric-connector — LLM context

The neutral data-execution boundary. Depends on `fabric-core`, `async-trait`,
`serde`, `serde_json`, `thiserror`. **No protocol or database types.**

## Public surface

- `DataConnector` (async trait, `Arc<dyn>`) — `id()`, `capabilities() -> &ConnectorCapabilities`,
  `schema() -> &ConnectorSchema`, `query(&ExecutionTarget, &QuerySpec)`,
  `mutate(&ExecutionTarget, &MutationSpec)`, `health()`.
  `capabilities()`/`schema()` return references — must be cached, no I/O per call.
- `ConnectorRegistry` — `new()`, `with(Arc<dyn DataConnector>)`, `get(&ConnectorId)`,
  `all()`. Fixed at startup. `get` fails closed with `UnknownConnector`.
- `ExecutionTarget::new(tenant, revision, data_source, connector, connection, isolation)`
  + accessors + `physical_resource_identifier()` (telemetry, no secrets).
  Six arguments because it is assembled from **both** halves of the resolution
  chain: `data_source`/`connector`/`connection` from the DataSource,
  `tenant`/`revision`/`isolation` from the tenant binding (ADR 0003). Only
  `fabric_tenant_runtime::RuntimeResolver` builds one.
- `IsolationModel` — `Database` | `Schema{schema}` | `Discriminator{column, value}`.
  `tenant_predicate() -> Option<Filter>`, `schema()`, `telemetry_label()`.
  `Schema` is deferred (ADR 0006) and `schema()` has zero production callers —
  nothing qualifies a collection reference. Do not read it as working.
- `ConnectionSelector` — `Default` | `Named{name}` | `Secret{reference}`.
  `telemetry_label()`, `needs_secret()`.
- `QuerySpec` — `collection`, `fields`, `filter`, `sort`, `limit`, `offset`.
  Builders `with_fields/with_filter/with_sort/with_paging`. **`for_target()`**.
  An empty `fields` is *no projection constraint*, not a default — a connector
  may answer with every column, discriminator included. Callers that must limit
  disclosure populate it **and** filter the response.
- `MutationSpec` — `Insert{collection, rows}` | `Update{collection, filter, changes}` |
  `Delete{collection, filter}`. `collection()`, `operation_name()`, **`for_target()`**.
- `QueryOutcome{rows, total_count}`, `MutationOutcome{affected_rows, returned_rows}`.
- `Filter` — `And{clauses}` | `Or{clauses}` | `Not{clause}` | `Compare{field, operator, value}`
  | `IsNull{field}` | `In{field, values}`. `and()` (flattens), `referenced_fields()`,
  `referenced_operators()`, `requires_null_check()`.
  `referenced_operators()` reports what the backend must be able to express, not a
  literal census of `Compare` nodes: `In` reports `Equal` (membership is a
  disjunction of equalities), `IsNull` reports nothing and is covered by
  `requires_null_check()`. A variant that reports neither is silently exempt from
  the capability gate — that was a real defect.
- `ComparisonOperator` — Equal, NotEqual, LessThan, LessThanOrEqual, GreaterThan,
  GreaterThanOrEqual, Contains (substring, not SQL LIKE).
- Inline tests live in sibling `*_tests.rs` modules with a shared `testing.rs`
  fixture, so the type files stay small.
- `ConnectorCapabilities` — `filtering`, `ordering`, `paging`, `mutations`,
  `transactional_mutations`, `total_count`, `null_checks`, `comparisons: BTreeSet<_>`.
  `baseline()`, `ensure_supports_query()`, `ensure_supports_mutation()` (the last
  two in `capabilities/support_check.rs`, sharing one private
  `ensure_supports_filter` so read and write checks cannot drift).
  `null_checks` is its own flag, not a `ComparisonOperator`: a null test is unary,
  and under three-valued logic `x = NULL` is unknown for every row, so equality
  support proves nothing about it. Not `serde`-derived — no config file sets it.
- `ConnectorSchema` / `CollectionSchema` — `ensure_fields()`, `collection()`,
  `has_field()`. No type modelling, field sets only.
- `Row` — `BTreeMap<FieldName, Value>` newtype. Deterministic ordering.
- `SecretRef` (loggable), `ResolvedSecret` (`Debug` = `<redacted>`, `.expose()`),
  `SecretResolver` (async trait).
- Names: `ConnectorId`, `CollectionName`, `FieldName`, `ConnectionName`,
  `SchemaName` — all from `identifier_newtype!`, all use
  `fabric_core::naming::parse_identifier`.
- `ConnectorError` — `UnknownConnector`, `Unsupported{feature, detail}`,
  `UnknownCollection`, `SecretUnavailable{reference}`, `Unreachable{connector, source}`,
  `Rejected{connector, message}`, `MalformedResponse{connector, detail}`,
  `InvalidOperation`. `is_internal()` drives 5xx-vs-4xx; `operator_message()` is
  the log-only rendering that includes a refusal's `RefusalDetail`.
- `UnsupportedFeature` — the closed vocabulary a refused caller may be told, and
  the only thing `Unsupported.feature` can hold. `as_str() -> &'static str`, so
  a variant carrying runtime text would not compile. Built into an error with
  `.refused()` / `.refused_because(detail)`.
- `RefusalDetail` — the operator's half of a refusal. No `Display`, on purpose:
  it cannot be interpolated into a caller-facing message.

## Hard invariants — do not break

1. **No NDC / SQL / driver / wire types in this crate.** This is the whole point.
2. **Every path to a connector goes through `for_target`.** It applies the
   tenant predicate for discriminator isolation and stamps insert/update rows.
   Bypassing it is a cross-tenant read or write.
3. **Insert/update stamping overwrites, never merges.** A caller-supplied
   discriminator value must not survive.
4. **Capabilities refuse, never degrade.** An unsupported predicate is
   `Unsupported`, not a dropped clause.
4a. **`Unsupported.feature` crosses to an application; nothing else here does.**
   `fabric-data-api` masks every other variant. Keep `UnsupportedFeature` closed
   and keep `as_str` returning `&'static str` — that return type is what makes a
   leak a compile error. Physical identifiers go in `RefusalDetail`.
5. **`ExecutionTarget` never holds a resolved credential** — only a selector.
6. **`ResolvedSecret` must keep its redacting `Debug`.** Do not derive `Debug`.
7. `ConnectorError::Rejected.message` is backend text — internal telemetry only,
   never returned to an application (it names physical tables and servers).

## Design notes

- `Arc<dyn DataConnector>` rather than a generic: the implementation set is not
  statically known (chosen by id at request time), and dispatch is free next to
  a network hop. This is *not* the executor-generic repository case — there is
  no cross-call transaction to compose at this level; transactionality is a
  connector-declared capability.
- `for_target` deliberately does not schema-qualify collection names. Schema
  isolation is enforced by the connection.
