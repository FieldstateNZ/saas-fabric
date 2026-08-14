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
- `DataApiService` — `list`, `read`, `create`, `update`, `delete`, `config()`.
  Holds an `Arc<RuntimeResolver>`, not registries.
  Split by responsibility across `execution/`: `data_api_service` (the struct),
  `prepare` (catalogue → authorization → DataSource → connector),
  `prepared`, `read_operations`, `write_operations`, `row_mapping`,
  `dispatch_write` (the one path every mutation takes), `write_integrity`
  (whether a backend's affected-row count agrees with the write sent).
  `list` takes the **raw query string**, not a parsed `ListQuery`, and there is
  deliberately **no `catalog()` accessor**: both exist so that nothing outside
  `prepare` can obtain a `ResourceDefinition`, and therefore nothing can
  validate a field name before authorization has run. See "Hard invariants" 13.
- `ResourceCatalog` — `resolve()`, `names()`, `len()`. Deserialises from a JSON
  object keyed by resource name.
- `ResourceDefinition { data_source, collection, key_field ("id"),
  operations (["read","list"]), queryable_fields ([]) }`. `allows()`,
  `permits_field()`, `projection(selected)`. `permits_field()` gates a field in
  **both** directions — what a caller may name, and what a response may carry.
  An empty `queryable_fields` still means "no restriction"; see the rustdoc on
  `permits_field` for why "expose nothing" is not a usable default here.
- `OperationKind { Read, List, Create, Update, Delete }` — `as_str()`,
  `is_write()`, `required_scope(resource)` → `data:{resource}:{read|write}`.
- `ResourcePermissions { require_scopes (true), administrator_role ("platform-admin") }`,
  `permits(&TenantIdentity, OperationKind, &str) -> bool`.
- `DataApiConfig { default_limit (50), max_limit (1000), max_filters (25),
  max_sort_fields (5), max_select_fields (50), max_filter_depth (4),
  max_request_body_bytes (1 MiB), max_mutation_batch_size (500) }`,
  `effective_limit()`, `validate()`. See docs/README.md's "Request limits".
- `ListQuery::parse(raw, &ResourceDefinition)`, `to_filter()`. Called from
  `DataApiService::list` **after** `prepare` has authorised, never from the
  handler — it validates field names, so calling it earlier turns 400-vs-403
  into a field-name oracle. Complexity bounds are checked separately, in
  `limits::enforce_query`, immediately after the parse — not inside `parse`
  itself.
- `VisibleFields::new(&ResourceDefinition, &IsolationModel)`, `permits(&FieldName)`
  — the two rules a field must pass to appear in a response. Built only from
  `Prepared::visible_fields()`, so obtaining one means the operation was
  resolved and authorised.
- `WritableFields::new(&ResourceDefinition, &IsolationModel)`, `permits(&FieldName)`
  — the write-path mirror, built only from `Prepared::writable_fields()`. Same
  two rules, but the discriminator is matched **case-insensitively**: a caller
  chooses the casing it sends, so `TENANT_KEY` must not slip past a comparison
  that `tenant_key` fails. Both types read the column through
  `models::discriminator::discriminator_column`, so they can never disagree
  about which column they are protecting.
- `ListResponse::from_outcome(&QueryOutcome, &VisibleFields, limit, offset)`,
  `PagingInfo`, `RowResponse::project(&Row, &VisibleFields)`,
  `WriteResponse::from_outcome(&MutationOutcome, &VisibleFields)` — one type per
  file under `models/`. `RowResponse` has **no** `From<&Row>`: the projecting
  constructor is the only one, so no path can serialise a row without the rules
  that apply to it. See invariant 6.
- `DataApiError` — `Identity`, `Resolve`, `UnknownResource`, `OperationNotAllowed`,
  **`ResourceIsReadOnly`**, `Forbidden`, `BadRequest`, `NotFound`,
  `PartiallyApplied`, `Connector { error, operation }`.
  Split across `errors/`: `data_api_error` (the enum), `status_mapping`
  (`status()`, `code()`, `retry_after()`), `response` (`public_message()`,
  `IntoResponse`, and the `request_id`/logging wiring described below),
  `connector_mapping` (a connector failure → status + code + message, given the
  operation), `connector_messages` (the wording, and why each sentence says what
  it does).
  **There is no `From<ConnectorError>`**: the operation is what decides whether a
  transport failure is retryable, so it must be supplied — normally by
  `Prepared::failed`.

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
  is applied once, in `data_routes`; `current()` reads it back from anywhere in
  the call stack (used by `errors::response` when building a failure
  body/log). Task-local rather than a parameter, deliberately — see the
  module's own rustdoc for why threading it through
  `list`/`read`/`create`/`update`/`delete`/`prepare`/`execute_query`/
  `execute_mutation` was rejected.
  - `request_id::correlation_id` — which id a request gets. An inbound
    `X-Request-Id` is adopted only if it is 1..=128 characters of
    `[A-Za-z0-9-_.:+/=]`; anything else (too long, whitespace, control
    characters) is **replaced with a fresh UUID, never truncated**, because a
    trimmed id would look like the caller's own but no longer match it, and two
    ids sharing a prefix would collapse onto one. The value is reflected onto
    the response header, into the error body, and into log fields, so it is
    bounded before any of that. `header_value()` is infallible, which is what
    lets `middleware` set the header unconditionally.

## Routes

```
GET    /v1/data/{resource}          list_resource
POST   /v1/data/{resource}          create_resource     → 201 (see below)
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
| `PartiallyApplied` | 500 | "{applied} of {requested} records were written; …cannot determine which" |
| `Connector` (internal) | 500 | "internal error" on a read; on a write, a sentence chosen by `effect()` so it never claims the mutation did not happen |
| `Connector::Unsupported` | 400 | "this operation is not supported: {feature}" — `UnsupportedFeature`'s own `&'static str` |

Every body is `{"error": {"code", "message", "request_id"}}` — `request_id` is
unconditional, not just on 5xx (see docs/README.md's "Correlation ids").

### Connector transport failures depend on the operation

The three variants that say *where on the wire the call broke* are the only
place `status()` depends on more than the error variant. `Retry-After` is
derived from the status, so it appears on every 503 and nothing else.

| Variant | Read | Write |
|---|---|---|
| `Unreachable` | 503 `connector_unavailable` | 503 `connector_unavailable` (+ `Retry-After`) |
| `OutcomeUnknown` | 503 `connector_unavailable` | 502 `write_outcome_unknown` |
| `ResultLost` | 503 `connector_unavailable` | 502 `write_result_unavailable` |

See docs/README.md's "What the platform promises about a write" for the meaning
each row carries and the at-most-once caveat.

## Hard invariants — do not break

1. **The tenant comes from `TenantIdentity` and nowhere else.** No method here
   takes a tenant parameter. (§10, §11)
2. **Authorization never influences tenant selection.** (§23)
3. **Every read goes through `QuerySpec::for_target`; every write through
   `MutationSpec::for_target`.** That is what applies the tenant predicate and
   stamps the discriminator.
4. **Connector error text never reaches a caller.** `is_internal()` decides for
   most arms. `Unsupported` is the one arm that repeats anything a connector
   said, and what it repeats is a `&'static str` out of `UnsupportedFeature`'s
   closed set — there are no connector-supplied bytes in that field to leak.
   This crate used to hold an allowlist (`errors::neutral_feature`) because the
   field was a `String`; the type carries the guarantee now and the allowlist
   was deleted rather than left looking load-bearing. The refusal's
   `RefusalDetail` is not readable from `public_message` even by mistake — it
   has no `Display` — and is recorded by `logging::connector_refused` via
   `ConnectorError::operator_message`.
5. **No unfiltered delete exists.** The route requires a key.
6. **`queryable_fields` gates both directions.** Inbound: filters, sorts,
   projections, and write payloads — `models::field_reference::parse` and
   `execution::row_mapping::to_row`. Outbound: every row a caller receives, via
   `RowResponse::project`, on list, read-by-key, and a connector's
   `returned_rows` alike. A control that refuses `?select=salary` and then
   returns `salary` is not a control. Do not re-add a `From<&Row>`.
7. **Writes check `ResolvedDataSource::is_writable()` before dispatch.**
7b. **A write reports success only if the backend's count agrees with what was
    sent.** `execution::write_integrity::ensure_consistent`, called from
    `dispatch_write` before the response is built. An insert of N rows must
    report exactly N (`PartiallyApplied` otherwise, 500 / `partial_write`); any
    write reporting more rows than were sent is a `MalformedResponse`. Do not
    "simplify" this to relaying `affected_rows`: a count that silently
    disagrees with the request is the defect, and no connector capability can
    substitute for the check — `transactional_mutations` is about the
    cardinality of NDC's `operations` array, not about atomicity within one
    operation.
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
13. **Authorization is decided before any check that could describe the
    resource.** `queryable_fields` validation — `field_reference::parse` and
    `row_mapping::to_row` — takes `prepared.resource`, so it cannot run before
    `prepare` has authorised. A 400 ahead of the 403 answers "is this a real
    field?" for a caller who was going to be refused. Checks that describe only
    a deployment-wide rule may run earlier, and two do: the request body size
    cap (enforced while the body is read) and the syntactic resource-name parse
    in the handlers. Pinned by `tests/authorization_ordering.rs`.
14b. **The tenant discriminator column can never be written by a caller
    either.** `models::WritableFields`, applied in `row_mapping::to_row`,
    rejects it under any casing before a `Row` is built. The stamp in
    `MutationSpec::for_target` remains as defence in depth, but the guarantee
    no longer depends on it overwriting the exact string a caller chose — which
    was a claim about backend collation this crate cannot enforce.
14. **The tenant discriminator column never appears in a response.** Independent
    of the catalogue and not overridable by it: `models::VisibleFields` drops it
    using the column name from the resolved `IsolationModel`, because §26 makes
    an application's unawareness of its isolation model an invariant rather than
    an operator's choice. `queryable_fields` alone did not hold this — a
    catalogue entry that enumerates nothing (the common case) returned the
    column and the tenant's internal surrogate key on every shared placement.

## Testing

Integration tests are split by concern, over a shared `tests/support/` module
holding a `RecordingConnector` that captures the `ExecutionTarget` and spec it
receives, a `DelayedConnector` for cancellation, a `ScriptedConnector` that
answers with whatever the test dictates (including any `ConnectorError`, built
by `connector_failures`), and a hand-rolled `tracing::Subscriber`
(`tracing_capture`) for asserting on log events without adding
`tracing-subscriber` as a test dependency:

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
| `error_contract.rs` | request id correlation, inbound-id bound, unknown-tenant anti-enumeration, connector-text masking, refusal logging |
| `field_exposure.rs` | `queryable_fields` and the discriminator gate every response body |
| `authorization_ordering.rs` | a refused caller gets one identical answer whatever the request shape |
| `write_outcome.rs` | a write may claim success only when the affected count agrees with what was sent |
| `transport_failures.rs` | each transport variant, read against write: status, code, `Retry-After`, and what the message may claim |

`tests/support/fixtures.rs`'s catalogue carries `restrictedCustomers`, the only
entry with a non-empty `queryable_fields`. Without it every field name is
permitted, so no test could distinguish "this resource exposes this field" from
"this resource exposes everything" — which is what `authorization_ordering.rs`
needs to ask.

`tracing_capture` installs its subscriber **globally, once**, and switches a
thread-local sink per `capture()` call. A thread-scoped subscriber loses a race
roughly one run in twenty-five: `tracing` caches a callsite's `Interest` from
the first firing thread's dispatcher, so a non-capturing test running in
parallel could permanently disable an event for a capturing one.

Every `app*` builder in `tests/support/mod.rs` funnels through
`app_with_config`, which takes a `&DataApiConfig` — reach for it directly
(rather than `app_with`, which always uses `DataApiConfig::default()`) when a
test needs a non-default limit.

Integration tests are their own crate, so `clippy.toml`'s
`allow-unwrap-in-tests` does not reach them — the allowances are declared at the
top of the file.
