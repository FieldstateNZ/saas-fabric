# fabric-runtime-publication — LLM context

The wire contract for the runtime's three published files, plus the port and
filesystem adapter that write them. In neither plane; non-dev dependencies
are `fabric-core`, `async-trait`, `serde`, `serde_json`, `thiserror` only. No
production caller yet — that is a control-plane crate ADR 0018 names but does
not build.

## Public surface (all re-exported from `lib.rs`)

- `TenantBindingDocument` — one element of `tenants.json`. Fields: `tenant:
  TenantId`, `revision: BindingRevision`, `data: TenantDataBindings`
  (non-empty whenever this crate builds one; `#[serde(default =
  "TenantDataBindings::empty_for_deserialisation")]` still lets an absent key
  parse, matching the consumer), `configuration:
  Option<ConfigurationBindingDocument>`, `secrets: Option<String>` (reference
  path only), `features: BTreeMap<String, bool>`, `storage: BTreeMap<String,
  StorageBindingDocument>`. Mirrors `fabric_tenant_runtime::TenantRuntimeBinding`
  field for field, as a separate declaration.
- `TenantDataBindings` — the non-empty wrapper around `BTreeMap<LogicalDataSourceName,
  TenantDataBindingDocument>`. `try_new` refuses an empty map; `values()` and
  `IntoIterator for &TenantDataBindings` cover the ways the rest of the crate
  reads one. See its own rustdoc for why deserialisation stays permissive of
  an empty map while construction does not.
- `TenantDataBindingDocument` — `data_source: DataSourceId`, `isolation:
  IsolationModelDocument`. Mirrors `TenantDataBinding`.
- `ConfigurationBindingDocument` — `store: String`, `profile:
  Option<String>`. Mirrors `ConfigurationBinding`.
- `StorageBindingDocument` — `provider`, `container`: `String`; `prefix`,
  `credentials`: `Option<String>` (credentials is a reference path only).
  Mirrors `StorageBinding`.
- `IsolationModelDocument` — `Database {}` | `Schema { schema: SchemaName }` |
  `Discriminator { column: FieldName, value: String }`. Internally tagged on
  `kind`, `deny_unknown_fields`. Every variant is struct-shaped — including
  the empty `Database {}` — because an internally tagged *unit* variant has
  no field list for `deny_unknown_fields` to check a surplus against; see the
  rustdoc on this type for the exact bug that gap once caused on the consumer
  side. Mirrors `fabric_connector::IsolationModel`.
- `DataSourceDocument` — one element of `data-sources.json`. Fields: `id:
  DataSourceId`, `revision: BindingRevision`, `connector: ConnectorId`,
  `connection: ConnectionSelectorDocument` (**required, no default**),
  `placement: PlacementClassDocument`, `residency: DataResidencyDocument`,
  `pool: PoolSettingsDocument` (`#[serde(default)]`), `capabilities:
  DataSourceCapabilitiesDocument` (`#[serde(default)]`, closed), `labels:
  BTreeMap<String, String>`. Mirrors `fabric_tenant_runtime::DataSource`.
- `ConnectionSelectorDocument` — `Default {}` | `Named { name:
  ConnectionName }` | `Secret { reference: String }` (reference path only).
  Same struct-shaped-variant reasoning as `IsolationModelDocument`. Mirrors
  `ConnectionSelector`.
- `PlacementClassDocument` — `Shared | Dedicated | HighAvailability |
  Regulated | Development | Ephemeral`, `rename_all = "snake_case"`. Mirrors
  `PlacementClass`. Descriptive only; nothing branches on it.
- `DataResidencyDocument` — `region: String`, `jurisdiction:
  Option<String>`. Mirrors `DataResidency`.
- `PoolSettingsDocument` — `max_connections: u32`, `idle_timeout_seconds:
  u64`, `acquire_timeout_seconds: u64`. `Copy`. Struct-level
  `#[serde(default)]`; `Default` matches the consumer's numbers (20 / 300 /
  5) exactly. Mirrors `PoolSettings`.
- `DataSourceCapabilitiesDocument` — `writable: bool`, `accepts_new_tenants:
  bool`. `Copy`. Struct-level `#[serde(default)]`; `Default` is
  `false`/`false` (fail closed). Mirrors `DataSourceCapabilities`.
- `CatalogDocument` — `catalog.json` as a whole: `#[serde(transparent)]` over
  `BTreeMap<LogicalResourceName, ResourceDefinitionDocument>`. `new`, `len`,
  `is_empty`, `resources()` (iterates `(&LogicalResourceName,
  &ResourceDefinitionDocument)`), `get(&LogicalResourceName)`,
  `canonical_json`. Mirrors `fabric_data_api::ResourceCatalog`, which is
  `Deserialize`-only — this is the type that closes the "nothing can write a
  catalogue" gap.
- `ResourceDefinitionDocument` — `data_source: LogicalDataSourceName`,
  `collection: CollectionName`, `key_field: FieldName` (`#[serde(default =
  "default_key_field")]`, defaults to `"id"`), `operations:
  Vec<OperationKind>` (`fabric_core::OperationKind`, reused directly;
  `#[serde(default = "default_operations")]`, defaults to `[Read, List]`),
  `queryable_fields: Vec<FieldName>` — **not** `Option`, always emitted, even
  empty; `#[serde(default)]` only affects deserialisation of documents that
  omitted the key (e.g. the shipped examples). Mirrors `ResourceDefinition`.
- `DocumentManifest` — `contract_version: u32`, `document: DocumentKind`,
  `revision: DocumentRevision`. Three fields, no timestamp.
  `deny_unknown_fields`. `new(document, revision)` stamps `CONTRACT_VERSION`
  so nothing builds one under the wrong version by hand; `canonical_json`
  (`pub`) renders it the same way every other document is rendered.
- `DocumentKind` — `Tenants | DataSources | Catalog`, `rename_all =
  "kebab-case"` → `"tenants" | "data-sources" | "catalog"`. `manifest_file()`
  returns this crate's own manifest file-name constant for that kind — the
  filesystem adapter derives every manifest path from this, never from a
  literal. There is no `payload_file()`: the payload path is always the one
  the caller supplied to `FilesystemRuntimePublication::new` (the runtime's
  own `tenants_path` / `data_sources_path` / `catalog_path`), so this crate
  has no fixed payload file name to return — only `TENANTS_FILE`,
  `DATA_SOURCES_FILE` and `CATALOG_FILE` as the constants a caller may use to
  name that path.
- `DocumentRevision` — newtype over `u64`, `#[serde(transparent)]`, `Ord`.
  **A document's revision, never a resource's** — `fabric_core::BindingRevision`
  is the resource revision, reused directly on `TenantBindingDocument.revision`
  and `DataSourceDocument.revision`. Do not conflate the two; see the type's
  own rustdoc.
- `CONTRACT_VERSION: u32 = 1`.
- File name constants: `TENANTS_FILE`, `TENANTS_MANIFEST_FILE`,
  `DATA_SOURCES_FILE`, `DATA_SOURCES_MANIFEST_FILE`, `CATALOG_FILE`,
  `CATALOG_MANIFEST_FILE`. Each matches the ConfigMap data-key character set
  `[-._a-zA-Z0-9]+`.
- `ConnectorId`, `ConnectionName`, `FieldName`, `CollectionName`, `SchemaName`
  — re-declared newtypes over `fabric_core::naming::parse_identifier`,
  because the canonical types live in `fabric-connector` (runtime plane) and
  this crate may not depend on it. ADR 0018, Decision part 1 names all five
  explicitly.
- `tenants_canonical_json(&[TenantBindingDocument]) -> Result<Vec<u8>, serde_json::Error>`
  and `data_sources_canonical_json(&[DataSourceDocument]) -> Result<Vec<u8>, serde_json::Error>`
  — sort by key (`tenant` / `id`), then render as canonical JSON.
  `CatalogDocument::canonical_json(&self)` needs no sort: `BTreeMap` already
  orders by key.
- `RuntimePublication` — `#[async_trait]` port: `current() -> Result<PublishedRevisions,
  PublicationError>`, `publish(&RuntimeSnapshot) -> Result<PublicationReport, PublicationError>`,
  `describe() -> String` (never a credential).
- `RuntimeSnapshot` — `tenants: DocumentInput<Vec<TenantBindingDocument>>`,
  `data_sources: DocumentInput<Vec<DataSourceDocument>>`, `catalog: DocumentInput<CatalogDocument>`.
  All three on every call; no partial-publish path.
- `DocumentInput<T>` — `revision: DocumentRevision`, `payload: T`, `emptying: Emptying`.
  `new(revision, payload)` defaults `emptying` to `NotIntended`; `.emptying_intended()` opts in.
- `Emptying` — `NotIntended` (default) | `Intended`. Per-document opt-in for taking a
  currently non-empty document to empty (ADR 0018 part 6). Ignored by the catalogue's own
  empty-check, which refuses unconditionally regardless of this value.
- `PublishedRevisions` — `tenants`, `data_sources`, `catalog`: each `Option<DocumentRevision>`,
  `None` where no manifest has ever been published.
- `PublicationReport` — `tenants`, `data_sources`, `catalog`: each a `DocumentOutcome`.
- `DocumentOutcome` — `Written | Unchanged`. `From<Verdict>` (internal) maps `Write` →
  `Written`, `Unchanged` → `Unchanged`.
- `PublicationError` — `thiserror` enum: `StaleRevision { document, held, offered }`,
  `DivergentPayload { document, revision }`, `DanglingDataSource { tenant, logical,
  data_source }`, `RetiredDataSourceStillBound { data_source, tenant }`,
  `EmptyingNotIntended { document }`, `EmptyCatalogue`, `EmptyTenantData { tenant }`
  (reachable only through `Deserialize` — construction refuses an empty `data` map,
  but the consumer would drop such a binding on arrival and keep whatever was held),
  `HeldPayloadLost { document }` (the held tenants document's manifest is present but
  its payload is gone — refused rather than read as empty, because guessing "empty"
  would disarm the retirement guard and the emptying guard, both of which read the
  held tenants document), `Unreadable { document, cause: Box<dyn Error + Send + Sync> }`,
  `Unwritable { document, cause }`.
- `FilesystemRuntimePublication` — the adapter. `new(tenants_path, data_sources_path,
  catalog_path)` (each `impl Into<PathBuf>`); implements `RuntimePublication`.

## Internal modules

- `canonical` — `pub(crate) fn to_canonical_bytes` — two-space pretty JSON
  plus a trailing newline. The one formatting decision every document shares;
  every `canonical_json`-shaped function delegates to it.
- `document` — every wire type, one per file under `src/document/`.
- `ids` — the five re-declared identifier newtypes.
- `manifest` — `DocumentManifest`, `DocumentKind`, `CONTRACT_VERSION`, the six
  file-name constants.
- `document_revision` — `DocumentRevision` alone, kept separate from
  `manifest` because it is a distinct concept from the envelope that carries
  it.
- `port` — the `RuntimePublication` trait alone.
- `snapshot` — `RuntimeSnapshot`, `DocumentInput<T>`, `Emptying`.
- `published_revisions` — `PublishedRevisions` alone.
- `report` — `PublicationReport`, `DocumentOutcome`, and `From<Verdict> for DocumentOutcome`.
- `errors` — `PublicationError` alone (121-150 line band: one enum, every variant's
  rustdoc names the ADR 0018 rule it enforces).
- `verdict` (`pub(crate)` only, not re-exported) — `Verdict { Write, Unchanged }`,
  `Held<'a> { revision, payload: Option<&'a [u8]> }`, `Incoming<'a> { document, revision,
  payload }`, and `verdict(held: Option<Held>, incoming: &Incoming) -> Result<Verdict,
  PublicationError>` — ADR 0018's presence table and revision table, verbatim, as one pure
  function. Paired with `verdict_tests.rs` (one test per table row).
- `validate` (`pub(crate)` only) — `validate_snapshot(&RuntimeSnapshot, held_tenants:
  &[TenantBindingDocument], held_data_sources: &[DataSourceDocument]) -> Result<(),
  PublicationError>` plus five private guard functions (empty catalogue, empty tenant
  data, dangling data source, retired-data-source-still-bound, unintended emptying —
  the last runs once for tenants and once for data sources), all pure — no filesystem.
  Paired with `validate_tests.rs`.
- `filesystem` — the adapter, split into `paths` (`DocumentPaths`: payload + derived
  manifest path), `held` (`HeldState`: reads all six files once per call and exposes
  each document's presence as a `verdict::Held`; `read_manifest` also checks a
  manifest's own `document` field against the file it was read from), `parse`
  (`parse_documents`: turns a held payload into a typed `Vec<T>`, absent → `vec![]`,
  unparseable → `PublicationError::Unreadable`; `parse_held_tenants`: the fail-closed
  sibling for the held *tenants* document alone, which additionally distinguishes a
  lost payload from one never published — see `PublicationError::HeldPayloadLost`),
  `plan` (`PublishPlan`: canonical bytes + resolved verdict
  for all three documents, computed before any write), `write` (`write_if_needed`: payload
  then manifest, skipped entirely on `Verdict::Unchanged`), `atomic_write` (temp-file +
  `fsync` + `rename` + a directory `fsync` to make the rename itself durable, sibling to
  the target, removed on every failure path), and `adapter`
  (`FilesystemRuntimePublication`'s `impl RuntimePublication`).

## Identifier reuse map

| Field | Type | Source |
|---|---|---|
| `tenant` | `TenantId` | `fabric-core`, reused |
| `data_source` (on a binding or a catalogue entry) | `DataSourceId` / `LogicalDataSourceName` | `fabric-core`, reused |
| a catalogue's map key | `LogicalResourceName` | `fabric-core`, reused |
| `connector` | `ConnectorId` | **this crate**, re-declared |
| `connection`'s `name` | `ConnectionName` | **this crate**, re-declared |
| `key_field`, discriminator `column`, `queryable_fields` entries | `FieldName` | **this crate**, re-declared |
| `collection` | `CollectionName` | **this crate**, re-declared |
| `schema` (Isolation::Schema) | `SchemaName` | **this crate**, re-declared |
| `secrets`, `credentials`, `reference` | `String` | not a validated newtype — see "Deliberately unvalidated" below |

## Deliberately unvalidated fields

`secrets`, `credentials`, and `Secret::reference` are plain `String`, not a
checked newtype: they are reference *paths*. `fabric_connector::SecretRef`,
the consumer's own equivalent, has no checked constructor either
(`#[serde(transparent)]` over a bare `String`) — there is no character-set
rule to mirror.

`collection` and `schema` are **not** in this list. ADR 0018, Decision part 1
names `CollectionName` explicitly alongside `ConnectorId`, `ConnectionName`,
and `FieldName` as an identifier the producer must re-declare and validate
itself; `SchemaName` follows the same identifier-newtype shape as the
consumer's own `fabric_connector::SchemaName`. Both are re-declared newtypes
here (`CollectionName`, `SchemaName`), not bare `String`s — a value either
side accepts is a value the other accepts too, and an invalid `collection` or
`schema` fails at construction rather than at the consumer's own startup or
refresh parse.

## The composed acceptance test

`tests/published_state_serves_two_tenants.rs` is the test the milestone
exists for (`docs/delivery.md`): it publishes a fixture — one shared
DataSource, two tenants isolated by different values in the same
discriminator column, and a one-resource `articles` catalogue — through the
real `FilesystemRuntimePublication`, then builds the real
`fabric_tenant_runtime::build_runtime` over the real `JsonFileSource` and the
real `fabric_data_api::build_data_api` over the real `ResourceCatalog`
(deserialised straight from the published `catalog.json`, since
`fabric-api`'s own `startup::catalog::load` is `pub(super)`), and drives the
assembled router with bearer tokens for both tenants. `tests/support/`
carries the fixture, a recording connector that applies the captured
predicate to a small shared corpus instead of dispatching on tenant identity
(so a missing or wrong predicate actually breaks a test rather than passing
by coincidence), and a stack builder every test in the file shares. It also
covers the producer-side refusals — stale revision, divergent payload at an
unchanged revision, a dangling DataSource reference, and an unintended
emptying — and the consumer-side survival guarantees a malformed published
document must not disturb, polling a bounded, real window rather than a
single sleep wherever a background refresh has to be observed taking effect.
This is why the crate's dev-dependencies grew to include `fabric-identity`
and `fabric-connector` alongside `fabric-tenant-runtime` and
`fabric-data-api` (see "Invariants to preserve" below).

## Invariants to preserve

- No dependency beyond `fabric-core` in `[dependencies]`. Dev-dependencies are
  `fabric-tenant-runtime` and `fabric-data-api`, for the round-trip tests
  beside each document type, plus `fabric-identity` and `fabric-connector`,
  for the composed acceptance test's bearer-token minting and recording
  connector. All four are already pre-declared in
  `scripts/check_architecture.py`'s `expected` table.
- Every document type derives both `Serialize` and `Deserialize`. The
  consumer's own types are asymmetric (`ResourceCatalog` /
  `ResourceDefinition` are `Deserialize`-only) precisely because nothing
  before this crate could write one.
- `queryable_fields` stays non-`Option`. Making it optional would silently
  reintroduce the "omission means unrestricted" ambiguity this type exists to
  remove.
- `connection` on `DataSourceDocument` stays required. A default here is the
  exact mistake the consumer's own rustdoc documents fixing.
- Canonical serialisation only ever adds a trailing newline and two-space
  indentation — do not add a digest, a hash, or any other derived field.
  Divergence detection is a byte comparison; anything that makes two
  semantically-identical documents serialise differently breaks it.
- `validate_snapshot` runs, and every document's verdict is resolved,
  *before* `FilesystemRuntimePublication::publish` writes anything. Do not
  interleave a write between them — a refused publication must never leave a
  partially-applied set.
- Write order is data sources, then catalogue, then tenants. Do not reorder:
  additions must land before a tenant binding can reference them.
- The retirement check (`RetiredDataSourceStillBound`) reads the *held*
  tenants document, never the one inside the incoming `RuntimeSnapshot`. Held
  absent → no constraint (nothing was ever bound); held present but
  unparseable → refuse via `Unreadable`, never guess.
- `verdict` and `validate_snapshot` take no `Path`, open no file, and must
  stay that way — they are what makes the presence/revision tables and the
  emptying guard testable without a temporary directory.
- `atomic_write`'s temporary file is always a sibling of its target, in the
  same directory (`rename` is atomic only within a filesystem), and is
  always cleaned up on a failed create/write/`fsync`/rename/directory-`fsync`
  — never left for a later call to trip over. The directory containing the
  target is `fsync`ed after every rename, so the rename itself survives a
  crash, not just the bytes it points at.
- `TenantDataBindings::try_new` refuses an empty map. `empty_for_deserialisation`
  is `pub(crate)`, not `Default`, and used only by `#[serde(default = "...")]`
  on `TenantBindingDocument::data` — the only other way to reach an empty
  value is deserialising a document that omits or empties `data`.
