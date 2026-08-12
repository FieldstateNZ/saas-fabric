# fabric-data-api — LLM context

The Data API. Event-ID domain **4**. Depends on `fabric-core`,
`fabric-identity`, `fabric-tenant-runtime`, `fabric-connector`, `axum`, `http`.

**Never depends on `fabric-connector-ndc`.** Nothing protocol-shaped may appear
in this crate's public contract.

## Public surface

- `build_data_api(&DataApiConfig, ResourceCatalog, ResourcePermissions,
  Arc<TenantRuntimeRegistry>, ConnectorRegistry, Arc<IdentityResolver>) -> Result<Router, String>`.
- `data_routes(DataApiState) -> Router`.
- `DataApiState { service, identity }`; `FromRef<DataApiState> for Arc<IdentityResolver>`
  is what makes the `TenantIdentity` extractor work.
- `DataApiService` — `list`, `read`, `create`, `update`, `delete`, `catalog()`,
  `config()`. Private `prepare()` walks the resolution chain.
- `ResourceCatalog` — `resolve()`, `names()`, `len()`. Deserialises from a JSON
  object keyed by resource name.
- `ResourceDefinition { data_source, collection, key_field ("id"),
  operations (["read","list"]), queryable_fields ([]) }`. `allows()`, `permits_field()`.
- `OperationKind { Read, List, Create, Update, Delete }` — `as_str()`,
  `is_write()`, `required_scope(resource)` → `data:{resource}:{read|write}`.
- `ResourcePermissions { require_scopes (true), administrator_role ("platform-admin") }`,
  `permits(&TenantIdentity, OperationKind, &str) -> bool`.
- `DataApiConfig { default_limit (50), max_limit (1000) }`, `effective_limit()`, `validate()`.
- `ListQuery::parse(raw, &ResourceDefinition)`, `to_filter()`.
- `ListResponse::from_outcome(&QueryOutcome, limit, offset)`, `PagingInfo`,
  `RowResponse`, `WriteResponse::from_outcome()`.
- `DataApiError` — `Identity`, `Resolve`, `UnknownResource`, `OperationNotAllowed`,
  `Forbidden`, `BadRequest`, `NotFound`, `Connector`. `status()`, `code()`,
  private `public_message()`.

## Routes

```
GET    /{resource}          list_resource
POST   /{resource}          create_resource     → 201
GET    /{resource}/{key}    read_resource
PATCH  /{resource}/{key}    update_resource
DELETE /{resource}/{key}    delete_resource
```

Mounted at `/data` by the composition root. axum 0.8 path syntax (`{key}`, not `:key`).

## Status mapping

| Error | Status | Public message |
|---|---|---|
| `RuntimeUnavailable` | 503 | "the platform is starting up; retry shortly" |
| `UnknownTenant` | 403 | "this tenant has no resources here" (no tenant echoed) |
| `UnknownDataSource` | 500 | "internal error" |
| `UnknownResource` / `NotFound` | 404 | as written |
| `OperationNotAllowed` | 405 | as written |
| `Forbidden` | 403 | as written |
| `BadRequest` | 400 | as written |
| `Connector` (internal) | 500 | "internal error" |
| `Connector::Unsupported` | 400 | names the feature only |

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
6. **`queryable_fields` is checked for filters and sorts, not only projections.**
7. **No NDC / protocol / SQL types in this crate.**
8. Resources default to read-only.

## Testing

`tests/data_api.rs` drives the assembled router with a `RecordingConnector`
that captures the `ExecutionTarget` and spec it receives. 23 tests covering:
tenant routing per placement, the discriminator predicate, insert stamping,
header rejection, fail-closed paths, paging probe rows, and live migration via
`apply_one`.

Integration tests are their own crate, so `clippy.toml`'s
`allow-unwrap-in-tests` does not reach them — the allowances are declared at the
top of the file.
