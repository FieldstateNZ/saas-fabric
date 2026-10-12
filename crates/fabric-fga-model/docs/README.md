# fabric-fga-model

The model writer (ADR 0027 §2a). It turns what a client declares under
`spec.authorization` (ADR 0013) into the authorization model a tenant's
store is pinned to (ADR 0016).

## What a declaration becomes

```yaml
resources:
  - resource: customers
    relations:
      - name: viewer
        permits: [read, list]
      - name: editor
        permits: [read, list, create, update]
```

| Type | Direct relations | Computed relations |
|---|---|---|
| `user` | | |
| `customers` | `viewer`, `editor` | `can_read` (viewer or editor), `can_update` (editor), `can_delete` (nobody) |
| `customers_collection` | `viewer`, `editor` | `can_list` (viewer or editor), `can_create` (editor) |

- **Records and collections are different types.** `read`, `update` and
  `delete` are asked about `customers:42`. `list` and `create` name no record,
  so they are asked about `customers_collection:customers`. Neither type
  defines the other's operations, so a record allow cannot answer a
  collection question.
- **Nothing links them.** `viewer` on the collection grants nothing on a
  record, and the reverse.
- **"Nobody" is a denial, not an error.** An operation no relation permits is
  still defined, as `<first> but not <first>`, where `<first>` is the least
  relation by name, so declaration order never changes the model. One
  contributor is that relation itself, because OpenFGA refuses a union of one.
- **No configuration, no model.** An empty configuration is unmanaged, and
  `authorization_model` returns `None` for it, never a principal-only model.
- **Reserved names are refused.** A managed resource may not be named `user`,
  `self` or `this`. A relation may not be named `self`, `this` or
  `can_<operation>`. A duplicate is refused, never overwritten.
- **`can_list` does not filter rows.** It authorizes the collection response.
  `ListObjects` and row filtering are a later refinement.

The collection type and the `can_<operation>` names come from `fabric-core`,
so this crate and `fabric-fga-check` (which asks the runtime's questions)
cannot spell them differently.

## Status

Not called. Writing the model through the front's control-plane surface and
pinning it per issuer is ADR 0027's Owed item 3. Nothing here talks to
anything.

## Tests

- `src/translate/translate_tests.rs`: the JSON for ADR 0013's example, the
  split between the two types, "nobody", order independence, `None`, and
  refusal of invalid or reserved input.
- `tests/model_against_openfga.rs`: the model against OpenFGA `v1.19.0`,
  pinned by digest. CI's `authorization-model` job runs it in required mode
  (`FABRIC_REQUIRE_OPENFGA=1`) against a throwaway container on the runner.
  Elsewhere it skips unless `FABRIC_TEST_OPENFGA_PORT` is set.
