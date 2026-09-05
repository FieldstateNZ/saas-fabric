# fabric-runtime-publication

The wire contract for the three files the runtime already reads:
`tenants.json`, `data-sources.json`, and `catalog.json`. Nothing writes them
yet — this crate is the contract a future publisher will write against, and
the crate that proves the runtime can read what it produces.

## Why this crate exists

`fabric-tenant-runtime` and `fabric-data-api` already know how to *consume*
these three files. The reconciled-resource lifecycle, the divergent-payload
guard, the last-good snapshot on a failed load — all of it exists and is
tested. What was missing was a producer, and the producer has a structural
problem: it cannot reuse the consumer's own Rust types.

`TenantRuntimeBinding` and `DataSource` live in `fabric-tenant-runtime`.
`IsolationModel` and `ConnectionSelector` live in `fabric-connector`. Both are
runtime-plane crates, and a future control-plane publisher must never depend
on either — `scripts/check_architecture.py` fails the build if it does. So
this crate declares its **own** copy of every wire shape, and depends on
nothing but `fabric-core`, exactly as `fabric-core` itself depends on
nothing. See `docs/architecture/crate-dependencies.md` for the full argument.

## How fidelity is kept without a shared type

Two mechanisms, working together:

- Every consumer type (`TenantRuntimeBinding`, `TenantDataBinding`,
  `DataSource`, `ResourceDefinition`) is `#[serde(deny_unknown_fields)]`. A
  field this crate adds that the consumer does not know about is a
  deserialisation error; a required field this crate stops emitting is a
  missing-field error. Neither drift can pass silently.
- A round-trip test beside each document type in `src/document/` takes this
  crate's own canonical JSON and deserialises it as the consumer's type —
  `TenantRuntimeBinding`, `DataSource`, or `ResourceCatalog` — via
  dev-dependencies on `fabric-tenant-runtime` and `fabric-data-api`.

Both copies exist and will keep existing. A reviewer changing one should treat
it as a change to both.

## What lives here

| Type | What it is |
|---|---|
| `TenantBindingDocument` | One tenant's complete runtime binding — one element of `tenants.json`. |
| `DataSourceDocument` | One configured physical data destination — one element of `data-sources.json`. |
| `CatalogDocument` | The whole resource catalogue — `catalog.json` is a bare object, not an array. |
| `DocumentManifest` | The sidecar published beside every document: `contract_version`, `document`, `revision`. No timestamp. |
| `DocumentRevision` | A document's own revision. **Never** a resource's — see its rustdoc for why the two must stay separate types. |

Every identifier is validated at construction, through
`fabric_core::naming::{parse_dns_label, parse_identifier}`. Where the
consumer's own id type already lives in `fabric-core` (`TenantId`,
`DataSourceId`, `LogicalDataSourceName`, `LogicalResourceName`), this crate
reuses it directly. Where it lives in the runtime plane instead
(`ConnectorId`, `ConnectionName`, `FieldName`, all `fabric-connector`), this
crate re-declares a newtype of the same name over the same parse function —
so a value either side accepts is a value the other accepts too.

## Canonical serialisation

Every document is rendered through `crate::canonical::to_canonical_bytes`:
two-space indentation, UTF-8, a trailing newline, `BTreeMap` throughout, and
resource arrays (`tenants_canonical_json`, `data_sources_canonical_json`)
sorted by key before serialising. Publishing the same snapshot twice produces
byte-identical output — load-bearing, because a future publisher's
divergent-payload guard is a byte comparison, not a semantic diff.

## No field anywhere can hold a secret value

A connection is a selector: a name the connector already holds configuration
for, or a reference to a secret. A tenant's `secrets` field, and a storage
area's `credentials` field, are base paths — `vault/tenants/acme` — never
values. Nothing in this crate is handed a secret resolver, and it depends on
no crate that has one.

## Gotchas

- The crate is `fabric-runtime-publication` (hyphen), the Rust identifier is
  `fabric_runtime_publication` (underscore) — the usual trap in `use`
  statements and `RUST_LOG` filters.
- `queryable_fields` on `ResourceDefinitionDocument` is **not** `Option`, and
  is always emitted, even when empty. The consumer defaults a missing key to
  "unrestricted"; making the field non-optional here turns a forgotten field
  into a compile error wherever this crate builds a catalogue, rather than a
  silently-permissive resource.
- `connection` on `DataSourceDocument` has no default, matching the consumer
  exactly. Two DataSources that both said nothing about their connection used
  to mean "the connector's one database" for both — two ids, one physical
  database.
- This crate has no port and no filesystem adapter. There is, on purpose, no
  production caller of anything here yet.
