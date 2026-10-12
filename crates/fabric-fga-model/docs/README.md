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
  still defined, as `<first relation> but not <first relation>`.
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
  split between the two types, "nobody", and refusal of an invalid
  declaration.
- `tests/model_against_openfga.rs`: the model against a real OpenFGA. This
  test is skipped unless `FABRIC_TEST_OPENFGA_PORT` names one (see the file).
  CI does not run it.
