# ADR 0020 — Keyed writes name their key arguments

- **Status:** Accepted
- **Date:** 2026-09-15
- **Applies to:** `fabric-connector-ndc`'s procedure mapping, and every
  configuration that maps an update or delete onto a connector procedure
- **Supersedes:** [ADR 0004](0004-write-support-in-the-first-release.md)
- **Related:** [ADR 0001](0001-ndc-as-connector-boundary.md),
  [ADR 0007](0007-isolation-is-checked-against-an-observed-fact-not-a-label.md),
  specification §18, §21, §28; issues #62 and #67;
  `docs/verification.md`, "Connector acceptance"

## Context

ADR 0004 shipped write support with a named gap: no procedure mapping had
been executed against a real connector, and the argument names were read
from documentation. Issue #62 closed most of that gap against a running
`ndc-postgres` v3.1.0 and falsified four assumptions. Three were corrected in
place. The fourth could not be:

> Update and delete are keyed by primary key, not by a predicate alone.
> `update_articles_by_id_and_tenant_key` and
> `delete_articles_by_id_and_tenant_key` take **required** `key_id` and
> `key_tenant_key` arguments alongside `pre_check`, and `CollectionProcedures`
> has nowhere to put a required key argument.

So a neutral `MutationSpec::Delete { filter }` — which is what the Data API's
`DELETE /v1/data/{resource}/{key}` becomes — could not be expressed against
the procedures the connector actually generates. The shipped example's update
and delete mappings were commented out rather than shipped wrong, and the
catalogue stopped advertising the two operations.

The same observation run found a second shape the mapping could not state:
the update procedure's `update_columns` argument takes per-column update
operations (`{"title": {"_set": "…"}}`), not column values.

## Decision

**A procedure mapping may name which of the procedure's arguments carry the
logical key, and how its update payload is shaped. Key values are taken from
the predicate the platform already built, never from the caller's body and
never inferred; and the full predicate still goes out under
`filter_argument`.**

```toml
[connectors.procedures.articles.delete]
procedure       = "delete_articles_by_id_and_tenant_key"
filter_argument = "pre_check"
key_arguments   = { id = "key_id", tenant_key = "key_tenant_key" }

[connectors.procedures.articles.update]
procedure        = "update_articles_by_id_and_tenant_key"
payload_argument = "update_columns"
payload_shape    = "set_operations"
filter_argument  = "pre_check"
key_arguments    = { id = "key_id", tenant_key = "key_tenant_key" }
```

Four rules follow, and each is enforced rather than documented:

1. **A key value comes from an equality the platform can see.** Translation
   looks for `field == value` among the direct clauses of the scoped
   predicate — the one `MutationSpec::for_target` produced, which is the
   caller's key conjoined with the tenant discriminator. A key field with no
   such equality, or with two disagreeing ones, or one only reachable under
   `Or` or `Not`, is refused. Nothing is defaulted, and nothing is read from
   the request body: a caller cannot name a key argument, and cannot make one
   say anything the predicate does not.

2. **The tenant discriminator reaches a keyed procedure twice.** Once as a
   key argument (`key_tenant_key`) and once inside `pre_check`, both derived
   from the same conjunct. That is deliberate. Under discriminator isolation
   the predicate *is* the tenant boundary (ADR 0004, "the tenant-isolation
   machinery"), and a keyed procedure gives the platform a second place to
   state it. The key conjuncts are copied into the arguments, not moved out
   of the predicate.

   Measured rather than assumed: with `pre_check` removed from a keyed
   delete, `ndc-postgres` v3.1.0 still deleted exactly one row, because the
   `articles` primary key is `(id, tenant_key)` and the key arguments alone
   named it (`docs/verification.md`, mutation M3). So on that table the key
   is a tenant boundary by itself, and the predicate is the second line — the
   one that holds on a table whose primary key does *not* include the
   discriminator, which is the ordinary shape of a shared table.

3. **A required argument nothing supplies is a startup failure.** The
   connector's own `/schema` types every procedure argument, and a
   non-nullable type is a required argument. Startup now refuses a mapping
   whose procedure requires an argument that neither the payload, the
   predicate nor a key argument supplies. This is the check that would have
   turned F3 into a boot failure on the first day rather than a `400` on the
   first write; it holds for every connector, not only this one.

4. **The payload shape is a closed enum.** `values` (`{col: value}`, the
   default and what every existing mapping sends) or `set_operations`
   (`{col: {"_set": value}}`). Only an update mapping may declare
   `set_operations`; an insert carrying it is refused at configuration
   validation. What a payload argument *should* look like remains
   connector-defined, so the enum grows by observation, never by guess.

Key arguments are also checked at startup against the schema: each must be
an argument the procedure declares and must not be predicate-typed (a key is
never a predicate), and names may not collide with the payload or predicate
argument — one argument map, and the second write wins.

## What this changes for a caller

Nothing. `PATCH /v1/data/{resource}/{key}` and `DELETE /v1/data/{resource}/{key}`
are unchanged, and no NDC vocabulary — `key_id`, `pre_check`, `_set`, a
procedure name — appears in any response. The acceptance suite asserts that
directly.

## Consequences

### Good

- Update and delete work against a real `ndc-postgres`, proven in
  `fabric-ndc-acceptance` against a running connector: a delete of a key the
  other tenant owns affects zero rows and the row survives; a delete under a
  shared logical key removes only this tenant's physical row; an update
  changes only this tenant's row.
- ADR 0004's checklist is complete, and the gap it named is closed rather
  than carried. Its "revisit when" condition — a real connector in CI — is
  met, so it is superseded rather than amended a third time.
- A mapping that cannot work is refused before the platform serves anyone,
  for a class of mistake (a required argument nobody maps) that previously
  surfaced only under traffic.

### Bad, and accepted

- Key extraction is strict. A predicate that carries a key only under
  `Filter::In` with one value, or under a nested `And`, is refused even
  though a human can see what was meant. The Data API never builds those
  shapes, and loosening the rule is a one-line change once a real caller
  needs it — the cost of being wrong in the other direction is another
  tenant's row.
- A dedicated-database tenant whose mapping names a discriminator key
  argument is refused on every write, because `for_target` adds no
  discriminator conjunct for that placement. That is the mapping being wrong
  for the placement, and the message says which field was missing; it is not
  something to paper over by omitting the argument.
- Two places now state the tenant boundary on a keyed write, and a reader
  may take the second for redundancy. The rustdoc on the translation says
  why it is not.

## Revisit when

A connector is observed whose keyed procedures take the key as a structured
argument (an object of key columns) rather than one argument per column, or
whose update payload takes a shape other than the two named here. Extend the
closed enum from the observation, with the fixture checked in, as this
decision was.
