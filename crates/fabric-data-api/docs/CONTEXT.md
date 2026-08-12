# fabric-data-api — LLM context

The Data API. Event-ID domain **4**. Depends on `fabric-core`,
`fabric-identity`, `fabric-tenant-runtime`, `fabric-connector`, `axum`, `http`.

**Never depends on `fabric-connector-ndc`.** Nothing protocol-shaped may appear
in this crate's public contract.

## Public surface

- `build_data_api(&DataApiConfig, ResourceCatalog, ResourcePermissions,
  Arc<RuntimeResolver>, ConnectorRegistry, Arc<IdentityResolver>) -> Result<Router, String>`.
- `data_routes(DataApiState) -> Router` — returns a router already rooted at
  `API_PREFIX`; the host must mount it as-is, not nest it under a further
  prefix.
- `API_PREFIX: &str = "/v1/data"` — the full external path prefix. Public so
  the host and this crate's own tests share one literal.
- `DataApiState { service, identity }`; `FromRef<DataApiState> for Arc<IdentityResolver>`
  is what makes the `TenantIdentity` extractor work.
- `DataApiService` — `list`, `read`, `create`, `update`, `delete`, `catalog()`,
  `config()`. Holds an `Arc<RuntimeResolver>`, not registries.
  Split by responsibility across `execution/`: `data_api_service` (the struct),
  `prepare` (catalogue → authorization → DataSource → connector),
  `prepared`, `read_operations`, `write_operations`, `row_mapping`.
- `ResourceCatalog` — `resolve()`, `names()`, `len()`. Deserialises from a JSON
  object keyed by resource name.
- `ResourceDefinition { data_source, collection, key_field ("id"),
  operations (["read","list"]), queryable_fields ([]) }`. `allows()`, `permits_field()`.
- `OperationKind { Read, List, Create, Update, Delete }` — `as_str()`,
  `is_write()`, `required_scope(resource)` → `data:{resource}:{read|write}`.
- `ResourcePermissions { require_scopes (true), administrator_role ("platform-admin") }`,
  `permits(&TenantIdentity, OperationKind, &str) -> bool`.
- `DataApiConfig { default_limit (50), max_limit (1000), max_filters (25),
  max_sort_fields (5), max_select_fields (50), max_filter_depth (4),
  max_request_body_bytes (1 MiB), max_mutation_batch_size (500) }`,
  `effective_limit()`, `validate()`. See docs/README.md's "Request limits".
- `ListQuery::parse(raw, &ResourceDefinition)`, `to_filter()`. Complexity
  bounds are checked separately, in `limits::enforce_query`, called from
  `DataApiService::list` — not inside `parse` itself.
- `ListResponse::from_outcome(&QueryOutcome, limit, offset)`, `PagingInfo`,
  `RowResponse`, `WriteResponse::from_outcome()`.
- `DataApiError` — `Identity`, `Resolve`, `UnknownResource`, `OperationNotAllowed`,
  **`ResourceIsReadOnly`**, `Forbidden`, `BadRequest`, `NotFound`, `Connector`.
  Split across `errors/`: `data_api_error` (the enum), `status_mapping`
  (`status()`, `code()`), `response` (`public_message()`, `IntoResponse`, and
  the `request_id`/logging wiring described below).

## Internal-only modules

Not re-exported from `lib.rs`, but load-bearing enough to know about:

- `limits` — `enforce_query(&ListQuery, &DataApiConfig)`,
  `enforce_batch_size(usize, &DataApiConfig)`. Every complexity/size bound
  except request body size; see docs/README.md.
- `extraction` — `BoundedJson<T>`, a `FromRequest<DataApiState>` extractor
  used in place of `axum::Json` on `create_resource`/`update_resource`. Reads
  the body through `axum::body::to_bytes` capped at
  `DataApiConfig::max_request_body_bytes`, so the limit is enforced against
  actual bytes read, not a caller-supplied `Content-Length`, and an overflow
  becomes a `DataApiError::BadRequest` (this crate's error shape) rather than
  axum's own bare `413`.
- `request_id` — a `tokio::task_local!`-scoped correlation id. `middleware`
  is applied once, in `data_routes`, and reads an inbound `X-Request-Id` or
  generates one; `current()` reads it back from anywhere in the call stack
  (used by `errors::response` when building a failure body/log). Task-local
  rather than a parameter, deliberately — see the module's own rustdoc for
  why threading it through `list`/`read`/`create`/`update`/`delete`/`prepare`/
  `execute_query`/`execute_mutation` was rejected.

## Routes

```
GET    /v1/data/{resource}          list_resource
POST   /v1/data/{resource}          create_resource     → 201
GET    /v1/data/{resource}/{key}    read_resource
PATCH  /v1/data/{resource}/{key}    update_resource
DELETE /v1/data/{resource}/{key}    delete_resource
```

`data_routes` builds the entire `/v1/data` prefix itself (`routes::API_PREFIX`);
the composition root merges the returned router rather than nesting it under
`/data` — nesting again would double the path to `/data/v1/data/...`. axum 0.8
path syntax (`{key}`, not `:key`).

## Status mapping

| Error | Status | Public message |
|---|---|---|
| `RuntimeUnavailable` | 503 | "the platform is starting up; retry shortly" |
| `UnknownTenant` | 403 | "this tenant has no resources here" (no tenant echoed) |
| `UnboundDataSource` | 500 | "internal error" |
| `MissingDataSource` | 500 | "internal error" (the id never leaks) |
| `ResourceIsReadOnly` | 405 | "…is read-only" (no placement detail) |
| `UnknownResource` / `NotFound` | 404 | as written |
| `OperationNotAllowed` | 405 | as written |
| `Forbidden` | 403 | as written |
| `BadRequest` | 400 | as written (includes every `limits`/`extraction` rejection) |
| `Connector` (internal) | 500 | "internal error" |
| `Connector::Unsupported` | 400 | names the feature only |

Every body is `{"error": {"code", "message", "request_id"}}` — `request_id` is
unconditional, not just on 5xx (see docs/README.md's "Correlation ids").

## Hard invariants — do not break

1. **The tenant comes from `TenantIdentity` and nowhere else.** No method here
   takes a tenant parameter. (§10, §11)
2. **Authorization never influences tenant selection.** (§23)
3. **Every read goes through `QuerySpec::for_target`; every write through
   `MutationSpec::for_target`.** That is what applies the tenant predicate and
   stamps the discriminator.
4. **Connector error text never reaches a caller.** `is_internal()` decides;
   there is a test asserting a table name does not leak.
5. **No unfiltered delete exists.** The route requires a key.
6. **`queryable_fields` is checked for filters, sorts, projections, and write
   payloads** — `models::field_reference::parse` and `execution::row_mapping::to_row`.
7. **Writes check `ResolvedDataSource::is_writable()` before dispatch.**
8. **No NDC / protocol / SQL types in this crate.**
9. Resources default to read-only.
10. **Every `limits`/`extraction` check runs before a connector is called**, and
    a violation must leave the recording connector's call count at zero in
    tests.
11. **`data_routes` owns the whole external path.** Nothing else may prepend
    another prefix in front of it.
12. **No handler, service method, or connector call is `tokio::spawn`ed onto a
    detached task.** A dropped request must cancel its in-flight connector
    call (item 37) — spawning would defeat that silently.

## Testing

Integration tests are split by concern, over a shared `tests/support/` module
holding a `RecordingConnector` that captures the `ExecutionTarget` and spec it
receives, a `DelayedConnector` for cancellation, and a hand-rolled
`tracing::Subscriber` (`tracing_capture`) for asserting on log events without
adding `tracing-subscriber` as a test dependency:

| File | Covers |
|---|---|
| `tenant_routing.rs` | tenant → DataSource routing, telemetry, no leakage |
| `tenant_isolation.rs` | discriminator predicate, insert/update stamping, batch stamping |
| `identity_boundary.rs` | header ban, missing/invalid/expired tokens |
| `failure_modes.rs` | unknown tenant, unprimed, missing DataSource, scopes |
| `data_source_capabilities.rs` | read-only placement |
| `querying.rs` | paging probe row, clamping, projection, sorting |
| `data_source_lifecycle.rs` | migration, rebinding, stale updates, removal |
| `request_limits.rs` | every `DataApiConfig` bound, at the limit and one over |
| `cancellation.rs` | a dropped request cancels the in-flight connector call |
| `schema_exposure.rs` | no physical collection/connector/DataSource name in any body |
| `error_contract.rs` | request id correlation, unknown-tenant anti-enumeration |

Every `app*` builder in `tests/support/mod.rs` funnels through
`app_with_config`, which takes a `&DataApiConfig` — reach for it directly
(rather than `app_with`, which always uses `DataApiConfig::default()`) when a
test needs a non-default limit.

Integration tests are their own crate, so `clippy.toml`'s
`allow-unwrap-in-tests` does not reach them — the allowances are declared at the
top of the file.
