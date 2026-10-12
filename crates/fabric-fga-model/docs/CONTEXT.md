# fabric-fga-model — LLM context

The control plane's model writer (ADR 0027 §2a). Depends on `fabric-core`,
`fabric-client-model` and `serde`. `reqwest`, `serde_json` and `tokio` are
dev-only. No transport, and nothing depends on it.

## Public surface

- `authorization_model(&AuthorizationConfiguration) -> Result<Option<AuthorizationModel>, DesiredStateError>`
  — runs `validate()` first (fields are public, so input is revalidated).
  For an empty configuration it returns `Ok(None)`, which means unmanaged.
  Otherwise it emits `user` and then, for each declared resource in order,
  `<resource>` and `<resource>_collection`. A duplicate type or relation is
  `DesiredStateError::Duplicate`, never an overwrite.
- `AuthorizationModel` — `Serialize` only; the OpenFGA schema 1.1
  `WriteAuthorizationModel` body (`schema_version`, `type_definitions`).

## Mapping (do not change without ADR 0027)

- Every declared relation → `{"this": {}}` on both types, metadata
  `directly_related_user_types: [{"type": "user"}]`.
- Record type: `can_read`, `can_update`, `can_delete`. Collection type:
  `can_list`, `can_create` (`OperationKind::is_collection_operation`).
- `can_<op>`: one granting relation → `computedUserset` (never a union of
  one); several → `union`; none → `difference` of the least relation by name
  with itself.
- Reserved, from `fabric_core::authorization_names`: managed resources `user`,
  `self` and `this`; relations `self`, `this` and every `can_<op>`.
- Names: `OperationKind::computed_relation`, `CollectionObject::type_name`.

## Hard invariants

1. A type never defines another kind's operation.
2. No tupleset or computed link between the record and collection types.
3. `can_<op>` is never direct, so no tuple can grant it.
4. A declaration that fails `validate()` is never translated.

## Internal modules

- `model.rs` — the serde shapes. The principal type is
  `fabric_core::authorization_names::PRINCIPAL_TYPE` (`user`), which must
  match `fabric-fga-auth`'s `USER_TYPE`; there is no edge between the two.
- `tests/model_against_openfga.rs` — the required CI proof (job
  `authorization-model`, OpenFGA `v1.19.0` pinned by digest).
- `translate.rs` (tests in `translate/translate_tests.rs`).
