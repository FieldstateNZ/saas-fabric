# fabric-fga-model — LLM context

The control plane's model writer (ADR 0027 §2a). Depends on `fabric-core`,
`fabric-client-model` and `serde`. `reqwest`, `serde_json` and `tokio` are
dev-only. No transport, and nothing depends on it.

## Public surface

- `authorization_model(&AuthorizationConfiguration) -> Result<AuthorizationModel, DesiredStateError>`
  — runs `validate()` first, then emits `user` and, for each declared
  resource in order, `<resource>` and `<resource>_collection`.
- `AuthorizationModel` — `Serialize` only; the OpenFGA schema 1.1
  `WriteAuthorizationModel` body (`schema_version`, `type_definitions`).

## Mapping (do not change without ADR 0027)

- Every declared relation → `{"this": {}}` on both types, metadata
  `directly_related_user_types: [{"type": "user"}]`.
- Record type: `can_read`, `can_update`, `can_delete`. Collection type:
  `can_list`, `can_create` (`OperationKind::is_collection_operation`).
- `can_<op>`: one granting relation → `computedUserset`; several → `union`;
  none → `difference` of the first relation with itself.
- Names: `OperationKind::computed_relation`, `CollectionObject::type_name`.

## Hard invariants

1. A type never defines another kind's operation.
2. No tupleset or computed link between the record and collection types.
3. `can_<op>` is never direct, so no tuple can grant it.
4. A declaration that fails `validate()` is never translated.

## Internal modules

- `model.rs` — the serde shapes; `PRINCIPAL_TYPE = "user"`, which must match
  `fabric-fga-auth`'s `USER_TYPE` (no edge; the OpenFGA test proves it).
- `translate.rs` (tests in `translate/translate_tests.rs`).
