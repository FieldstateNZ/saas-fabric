# fabric-connector-ndc — LLM context

NDC v0.2.13 implementation of `DataConnector`. Event-ID domain **3**.
Depends on `fabric-connector`, `fabric-core`, `reqwest`, `async-trait`, `serde`,
`serde_json`, `thiserror`, `tracing`.

**No Hasura crates. `hasura/ndc-spec` is unlicensed — never add it.**
Wire types are hand-written in `src/wire/` from the published spec.

## Public surface

- `NdcConnector` — implements `DataConnector`. `schema_index()` for diagnostics.
- `build_ndc_connector(config, secrets) -> Result<Arc<NdcConnector>, String>` —
  async; does `GET /capabilities` + `GET /schema`, checks version, caches both.
- `NdcConnectorConfig { id, endpoint, timeout_seconds (10), connection_name_argument
  ("connection_name"), connection_string_argument ("connection_string"), procedures }`.
  `validate()`, `has_writes()`.
- `CollectionProcedures { insert, update, delete: Option<ProcedureBinding> }`,
  `is_writable()`.
- `ProcedureBinding { procedure, payload_argument, filter_argument }`.
- `SchemaIndex` — `neutral()`, `supported_operators()`, `has_procedure()`,
  `operator_name(collection, field, SemanticOperator)`.
- `SemanticOperator { Equal, In, LessThan, LessThanOrEqual, GreaterThan,
  GreaterThanOrEqual, Contains }` — **no NotEqual**; `for_neutral()` maps
  `ComparisonOperator::NotEqual` to `Equal` (negated at translation).
- `NDC_VERSION = "0.2.13"`, `NDC_VERSION_HEADER = "X-Hasura-NDC-Version"`.

## Internal modules

- `wire/` — `query.rs`, `expression.rs`, `mutation.rs`, `response.rs`,
  `negotiation.rs`. All `pub(crate)`. Field names mirror NDC exactly.
- `translate/` — `query.rs`, `expression.rs`, `mutation.rs`, `response.rs`,
  `capabilities.rs`.
- `routing.rs` — `request_arguments(config, selector, secrets)`.
- `client.rs` — `NdcHttpClient`: `get`, `post`, `health`, error mapping.
- `schema_index.rs`, `config.rs`, `connector.rs`, `logging.rs`, `registration.rs`.

## Protocol facts worth remembering

- Endpoints: `GET /health`, `GET /metrics`, `GET /capabilities`, `GET /schema`,
  `POST /query`, `POST /query/explain`, `POST /mutation`, `POST /mutation/explain`.
- `QueryRequest { collection, query, arguments, collection_relationships,
  variables, request_arguments }`. **`request_arguments` (spec 0.2.4) carries
  per-tenant connection routing.**
- `Query { aggregates, fields, limit, offset, order_by, predicate, groups }` —
  the predicate field is called `predicate`.
- `Expression`: `and`/`or`/`not`/`unary_comparison_operator`/
  `binary_comparison_operator`/`array_comparison`/`exists`. Operator names are
  connector-chosen strings.
- `ComparisonOperatorDefinition`: Equal, In, LessThan, LessThanOrEqual,
  GreaterThan, GreaterThanOrEqual, Contains, ContainsInsensitive, StartsWith(+
  Insensitive), EndsWith(+Insensitive), Custom. **No NotEqual.**
- `QueryResponse` is `Vec<RowSet>`; `RowSet { aggregates, rows, groups }`.
- **Mutations are procedures only**: `MutationOperation::Procedure { name,
  arguments, fields }`. `MutationOperationResults::Procedure { result }` — shape
  undefined by the spec.
- Filtering/ordering/paging are core, not negotiated capabilities.

## Hard invariants — do not break

1. **Nothing NDC-shaped escapes this crate.** No re-exports of `wire::*`.
2. **Never widen an operation.** Untranslatable → `Unsupported`. A dropped
   predicate may be the tenant boundary.
3. **Update/delete require `filter_argument`.** Checked in `config::validate`
   and again in `translate::mutation`. Both checks must stay.
4. **A mutation reaching translation with no predicate is refused.**
5. **`ResolvedSecret::expose()` is called in exactly one place** —
   `routing.rs`, straight into the request body. Never logged, never in a span,
   never in an error.
6. **Writes stay off until a procedure mapping exists**, even if the connector
   could accept them.
7. Connector error text (`ConnectorError::Rejected.message`) is logged, never
   returned to an application.

## Notes

- Version check: major.minor must match; patch difference warns.
- `SchemaIndex::supported_operators()` is the *union* across scalar types
  (permissive); the authoritative per-field check is `operator_name()`, which
  fails closed.
- `ContainsInsensitive` satisfies `Contains` — a deliberate widening, safe
  because tenant scoping is always equality, never containment.
