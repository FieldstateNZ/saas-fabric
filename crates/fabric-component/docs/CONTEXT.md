# fabric-component — LLM context

The component descriptor contract of ADR 0026 (slice 1): types, strict
reader, validator and canonical renderer for the JSON document a component
attaches to its primary image, plus the configuration-field, application
resource and capability shapes it shares with the catalogue. In neither plane
(see `docs/architecture/crate-dependencies.md`); a domain crate in
`scripts/check_architecture.py` (no HTTP crate). Internal edges exactly
`fabric-core` and `fabric-runtime-publication`; external deps `serde`,
`serde_json`, `serde_norway`, `thiserror`. Depended on by
`fabric-client-model`; `fabric-platform-management` gains its edge in slice 2.

## Public surface (all re-exported from `lib.rs`)

- `ContractError` — `Invalid { detail }` (`Display` is the detail alone) and
  `UnsupportedVersion { found }` ("a component descriptor of version {found}
  is not one this build reads; this build reads v1"), `found` always spelled
  `v<N>` (`v2`), whether an `apiVersion` or an `artifactType` named it.
- Moved from `fabric-client-model`, unchanged: `ConfigurationField`,
  `FieldKind`, `ApplicationResource` (with `into_definition`).
- Validators moved from `fabric-client-model`: `text(value, label, required,
  max)`, `unique(iter, label)`, `check_key`, `is_timezone`,
  `validate_fields`, `check_value(field, value)`,
  `validate_resources(&[ApplicationResource])`; and `is_hostname`,
  `is_identifier`, re-declared over `fabric_core::naming::parse_dns_label` to
  match `Host::try_new` and `ClientId::try_new`.
- `PlatformCapability` — seven variants serialized as `Identity`,
  `Database`, `Secrets`, `Authorization`, `Routing`, `Object storage`,
  `Messaging`; `ALL`, `as_str`, `parse -> Option`.
- Constants: `ARTIFACT_TYPE`, `DOCUMENT_MEDIA_TYPE`,
  `ARTIFACT_TYPE_FAMILY_PREFIX`, `API_VERSION` (`fabric.fieldstate.nz/v1`),
  `KIND` (`Component`), `DOCUMENT_FILE_NAME` (`component.json`),
  `MAX_DOCUMENT_BYTES` (16 KiB), `MAX_IMAGES` (8), `MAX_FIELDS` (64),
  `MAX_RESOURCES` (64); `family_version(artifact_type) -> Option<&str>`
  (digits after the prefix, else `None`).
- Newtypes (`try_new`, `as_str`, `Display`, serde via `String`):
  `ComponentName`, `Role`, `ComponentVersion`, `Digest`, `Repository`
  (`host()` with port, `path()`).
- `ImageReference { repository, digest }`, `ComponentSpec { name, title,
  description, version, images: BTreeMap<Role, ImageReference>,
  capabilities, fields, resources }`.
- `ComponentDescriptor` — `new(spec)`, `from_json(&[u8])`,
  `from_artifact(artifact_type, &[u8])`, `validate()`, `to_json() -> Vec<u8>`,
  `spec()`, `role_of(&Repository, &Digest) -> Option<&Role>`.
- `ComponentSource` (with `ComponentSourceSpec`, `SourceImage`) —
  `from_yaml(&str)`, `spec()`, `images()` yielding `(&Role, &Repository)`,
  `render(version, &BTreeMap<Role, Digest>) -> Result<ComponentDescriptor>`.

## Internal modules

- `errors.rs` — `ContractError`, `invalid()`.
- `fields.rs`, `resource.rs` — the moved shapes (resource keeps its tests).
- `capability.rs` — `PlatformCapability`.
- `validation.rs` (`text`, `unique`) + `validation/{fields,values,resource,names}.rs`.
- `descriptor.rs` — `ComponentDescriptor`, `new`, `validate`, `to_json`,
  accessors; `descriptor/newtype.rs` (the `contract_newtype!` macro);
  `component_name.rs`, `role.rs`, `version.rs`, `digest.rs`,
  `repository.rs` + `repository/{host,path}.rs`; `constants.rs`;
  `spec.rs`; `envelope.rs` (`Read<S>`, `Written<S>`, `check` — run before the
  spec is parsed); `strict_json.rs` (a serde `Visitor` over
  `serde_json::Deserializer` refusing a repeated key in any object);
  `format_characters.rs` (general category `Cf` ranges); `read.rs`
  (`from_json`, `from_artifact`); `validate.rs`; `source.rs` +
  `source/render.rs`. Tests: `descriptor_tests.rs`, `newtypes_tests.rs`, `v1_shape_tests.rs`.
- `examples/descriptor/{main,render}.rs` — `check`, `images`, `render`;
  exit status 2 on any failure.
- `tests/this_repositorys_component.rs` — the root `component.yaml` against
  `.github/workflows/release.yml`'s build matrix (roles, repositories,
  `primary`), rendered and read back.

## Hard invariants — do not break

1. **v1's field set and enumerations are fixed, and its rules only relax.**
   A new field (even optional), a new `PlatformCapability`, a new
   `FieldKind`, or any stricter rule — including a new format-character
   range — is a new component descriptor version, not an edit.
2. **The moved validators' messages are part of the catalogue's API.**
   Byte-identical to what `fabric-client-model` produced; its tests and
   `fabric-control-plane`'s assert on them.
3. **The moved shapes serialize byte-identically** — same derives, serde
   attributes and field order. `fabric-client-model/tests/fixtures/catalogue.yaml`
   must keep re-rendering unchanged.
4. **No transport.** No HTTP crate, no tokio, no registry reads. Fetching,
   hashing and counting attached artifacts belong to an adapter.
5. **Duplicate keys are refused** in any JSON object, at any depth, never
   resolved by the last one winning.
6. **`to_json` is canonical**: compact, envelope first, struct fields in
   declared order, maps sorted, every field serialized (no
   `skip_serializing_if`); `from_json(to_json(x)) == x`. The canonical size
   bound is checked in `validate`, so every `ComponentDescriptor` — however
   made — has bytes its own `from_json` reads. `descriptor/v1_shape_tests.rs`
   pins the ADR example's canonical bytes and every shared shape's fields.
7. **The envelope is checked before the spec.** A `Component` of another
   `fabric.fieldstate.nz/v<N>` version is `UnsupportedVersion { found: "v<N>" }`,
   the spelling `from_artifact` uses for a later `artifactType`; any other
   document is `Invalid` as the wrong document.
8. **Format characters are refused only by the descriptor** (`validate`),
   never by `validate_fields`, which also reads stored catalogues.
9. **A repository has one spelling**: host lower case and a name (not an IP
   address: WHATWG's "ends in a number", so `127.1`, `0x7f000001` and
   `registry.123` are refused too), no `:443`, Docker Hub only as `docker.io` with at least two path
   segments, no scheme, tag or digest.
10. **`is_hostname` / `is_identifier` agree with `Host` / `ClientId`**
    (`fabric-client-model/tests/value_rules_agree.rs`).

## Notes

- `ComponentSource::from_yaml` trusts its input (the publisher's own file);
  YAML aliases are why only JSON is published.
- `from_artifact` with a v1 `artifactType` and a document of another
  `apiVersion` is `Invalid` ("disagrees"), not `UnsupportedVersion`.
- `render` names every missing and extra role; the size bound is
  `validate`'s.
- No repository is named by two roles: one repository has one tag of a
  version, so two roles there could only name one image twice, and
  `role_of` could not choose.
