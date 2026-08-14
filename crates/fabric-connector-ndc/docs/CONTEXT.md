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
- `NdcConnectorConfig { id, endpoint, http_timeout_seconds (10),
  http_connect_timeout_seconds (5), connection_name_argument ("connection_name"),
  connection_string_argument ("connection_string"), procedures }`.
  `validate()`, `has_writes()`. The two `http_*` fields bound only the HTTP hop
  to the connector; database execution timeout is the connector's own
  configuration, and the overall Data API request budget is the host's — see
  the field docs on `http_timeout_seconds`.
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
  `capabilities.rs`, `schema.rs`, `ndc_type.rs`. All `pub(crate)`. Field names
  mirror NDC exactly.
- `translate/` — `query.rs`, `expression.rs`, `membership.rs`, `mutation.rs`,
  `procedure_arguments.rs`, `response.rs`, `capabilities.rs`. Refusals are built
  as `UnsupportedFeature::…refused_because(detail)` — see invariant 8.
- `schema_index/` — `schema_index_type.rs`, `semantic_operator.rs`,
  `operator_index.rs`, `collection_index.rs`.
- `config/` — `connector_config.rs`, `procedures.rs`.
- `client/` — `http_client.rs`, `error_mapping.rs`.
- `routing.rs` — `request_arguments(config, selector, secrets)`.
- `connector.rs`, `logging.rs`, `registration.rs`.

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
8. **`Unsupported.feature` is published text — and is now a closed type.**
   `fabric-data-api` masks every other connector error but forwards this one's
   capability name in a 400 body. It is a `fabric_connector::UnsupportedFeature`,
   so it carries only fixed vocabulary ("the equal comparison", "writes to this
   collection") and *cannot* name a collection, field, or procedure. The
   predicate case is why: `to_expression` runs after `for_target` conjoins the
   discriminator, so a refusal there is raised over the tenant isolation column.
   Physical detail goes in the accompanying `RefusalDetail`, which has no
   `Display` and surfaces only through `ConnectorError::operator_message` — read
   by `logging::operation_refused` here and `data_api.connector_refused` above.
   The old rule ("route every construction through one module") is gone with the
   module: the type is the enforcement now.

## Notes

- Version check: major.minor must match (either direction); patch difference
  warns. `registration::check_version` returns `VersionOutcome::Matched` or
  `VersionOutcome::PatchMismatch { connector_version }` rather than a bare
  `Result<(), _>`, so the two accepted outcomes are distinguishable in tests
  without a tracing subscriber.
- `client::NdcHttpClient::decode_body` is the pure, non-async half of response
  decoding (status + bytes → `T` or `ConnectorError`), split out of `decode`
  specifically so a malformed `/capabilities` body is unit-testable without a
  live connector.
- `SchemaIndex::supported_operators()` is the *union* across scalar types
  (permissive); the authoritative per-field check is `operator_name()`, which
  fails closed.
- `ContainsInsensitive` satisfies `Contains` — a deliberate widening, safe
  because tenant scoping is always equality, never containment.
