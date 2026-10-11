# fabric-fga-check — LLM context

The HTTP adapter behind `fabric-data-api`'s `AuthorizationCheck` port
(ADR 0027). It calls the authorization front's runtime surface,
`POST /v1/check` (ADR 0016). Runtime plane. Depends on `fabric-data-api`,
`async-trait`, `reqwest`, `serde` and `serde_json`. `fabric-core`, `axum` and
`tokio` are dev-only.

**No edge to `fabric-fga-auth`.** It shares the front's wire contract, not its
code. **Nothing depends on it.** `fabric-api` composes no Check.

## Public surface

- `FrontEndpoint::parse(&str) -> Result<Self, String>` — `http`/`https` only.
  It refuses credentials, a query or a fragment, and trims a trailing `/`.
  `check_url()` → `{base}/v1/check`.
- `FrontCheck::new(FrontEndpoint) -> Result<Self, String>` — `reqwest` client
  with a 5s timeout, `redirect::Policy::none()` and `no_proxy()`.
  Implements `AuthorizationCheck`.
- `computed_relation(OperationKind) -> String` — `can_<operation>`.

## Mapping (do not change without ADR 0027)

- Transport error, or any `5xx` → `CheckFailure::Unavailable`.
- Any other non-`200` (including `401`, `403` and `3xx`) → `CheckFailure::Internal`.
- A `200` body that is not `{"allowed": bool}` → `CheckFailure::Internal`.
- `{"allowed": true}` → `Allowed`; `{"allowed": false}` → `Denied`.

## Hard invariants

1. The only credential sent is `CallerCredential::bearer()`, as
   `Authorization: Bearer`. This crate has no credential of its own.
2. The body is exactly `{relation, object}`. It never carries a user, store,
   tenant or model.
3. No failure becomes `Allowed` or `Denied`.
4. No redirect is followed, and no environment proxy is used.

## Internal modules

- `wire.rs` — `CheckBody<'a> { relation, object }` and `CheckAnswer { allowed }`.
- `front_check.rs`, `front_endpoint.rs`, `computed_relation.rs`.
