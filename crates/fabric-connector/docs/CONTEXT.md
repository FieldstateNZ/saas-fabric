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
- `ExecutionTarget::new(tenant, revision, connector, connection, isolation)` +
  accessors + `physical_resource_identifier()` (telemetry, no secrets).
- `IsolationModel` — `Database` | `Schema{schema}` | `Discriminator{column, value}`.
  `tenant_predicate() -> Option<Filter>`, `schema()`, `telemetry_label()`.
- `ConnectionSelector` — `Default` | `Named{name}` | `Secret{reference}`.
  `telemetry_label()`, `needs_secret()`.
- `QuerySpec` — `collection`, `fields`, `filter`, `sort`, `limit`, `offset`.
  Builders `with_fields/with_filter/with_sort/with_paging`. **`for_target()`**.
- `MutationSpec` — `Insert{collection, rows}` | `Update{collection, filter, changes}` |
  `Delete{collection, filter}`. `collection()`, `operation_name()`, **`for_target()`**.
- `QueryOutcome{rows, total_count}`, `MutationOutcome{affected_rows, returned_rows}`.
- `Filter` — `And{clauses}` | `Or{clauses}` | `Not{clause}` | `Compare{field, operator, value}`
  | `IsNull{field}` | `In{field, values}`. `and()` (flattens), `referenced_fields()`,
  `referenced_operators()`.
- `ComparisonOperator` — Equal, NotEqual, LessThan, LessThanOrEqual, GreaterThan,
  GreaterThanOrEqual, Contains (substring, not SQL LIKE).
- `ConnectorCapabilities` — `filtering`, `ordering`, `paging`, `mutations`,
  `transactional_mutations`, `total_count`, `comparisons: BTreeSet<_>`.
  `baseline()`, `ensure_supports_query()`, `ensure_supports_mutation()`.
- `ConnectorSchema` / `CollectionSchema` — `ensure_fields()`, `collection()`,
  `has_field()`. No type modelling, field sets only.
- `Row` — `BTreeMap<FieldName, Value>` newtype. Deterministic ordering.
- `SecretRef` (loggable), `ResolvedSecret` (`Debug` = `<redacted>`, `.expose()`),
  `SecretResolver` (async trait).
- Names: `ConnectorId`, `CollectionName`, `FieldName`, `ConnectionName`,
  `SchemaName` — all from `identifier_newtype!`, all use
  `fabric_core::naming::parse_identifier`.
- `ConnectorError` — `UnknownConnector`, `Unsupported{feature}`,
  `UnknownCollection`, `SecretUnavailable{reference}`, `Unreachable{connector, source}`,
  `Rejected{connector, message}`, `MalformedResponse{connector, detail}`,
  `InvalidOperation`. `is_internal()` drives 5xx-vs-4xx.

## Hard invariants — do not break

1. **No NDC / SQL / driver / wire types in this crate.** This is the whole point.
2. **Every path to a connector goes through `for_target`.** It applies the
   tenant predicate for discriminator isolation and stamps insert/update rows.
   Bypassing it is a cross-tenant read or write.
3. **Insert/update stamping overwrites, never merges.** A caller-supplied
   discriminator value must not survive.
4. **Capabilities refuse, never degrade.** An unsupported predicate is
   `Unsupported`, not a dropped clause.
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
