# ADR 0004 — Write support ships in the first release, with a named gap

- **Status:** Accepted
- **Date:** 2026-08-13
- **Applies to:** the Data API's mutation path
- **Related:** [ADR 0001](0001-ndc-as-connector-boundary.md), [ADR 0003](0003-data-sources-are-first-class-resources.md), specification §18, §21, §28

## Context

Review asked directly whether writes belong in the first merge, on the grounds
that mutation support might be materially less mature than reads — and that
write support should not survive merely because it already exists.

That is a fair challenge, and the answer needs to separate two things that are
easy to conflate.

### The tenant-isolation machinery is mature

Under discriminator isolation, a tenant boundary on a write exists **only**
because the platform adds a predicate and stamps a column. Get it wrong and an
update reaches another tenant's rows, or a delete empties a shared table.

That machinery is in `fabric-connector`, is one code path
(`MutationSpec::for_target`), and is covered adversarially: hostile callers
supplying another tenant's discriminator on insert and update, callers trying to
widen with `Or` and `Not`, deletes with no predicate, multi-row inserts where
only some rows are hostile, and the same set repeated against dedicated-database
and per-tenant-schema placement. It is also enforced structurally — a caller
cannot construct an `ExecutionTarget`, and every route to a connector passes
through `for_target`.

Removing writes would take that machinery and its tests out of the first merge
and require re-adding them later, with the isolation logic re-derived rather
than reviewed once.

### The NDC procedure mapping is not

Core NDC 0.2 has no generic insert/update/delete. The only mutation operation is
invoking a **procedure** the connector declares, so mapping a neutral
`MutationSpec` onto a real backend needs per-collection configuration: procedure
names, and the argument names that carry the payload and the predicate.

Those argument shapes — `objects`, `filter`, and their siblings — are read off
`ndc-postgres` documentation. **They have never been exercised against a running
connector.** A wrong `filter_argument` name is not a 500; it is the predicate
going missing, which is a data-loss bug.

## Decision

**Writes ship.** The isolation guarantees are the hard part, they are done, and
they are tested against a hostile caller rather than a careless one.

The immature part is contained and made loud:

1. **Writes are off by default.** A collection with no procedure mapping cannot
   be written to at all; the connector reports `mutations: false`. Enabling
   writes is a deliberate configuration act.
2. **A DataSource must declare `writable: true`.** Capabilities fail closed
   ([ADR 0003](0003-data-sources-are-first-class-resources.md)), so a read
   replica that nobody remembered to mark is refused, not written to.
3. **An update or delete mapping without a `filter_argument` is rejected at
   startup**, and again at translation time. Both checks exist deliberately: the
   cost of this one failing open is other tenants' data.
4. **A mutation reaching the connector with no predicate is refused**, even
   though `for_target` should always have added one. That check is there for the
   case where something bypassed it.

## The gap, stated plainly

**No procedure mapping in this repository has been executed against a real
`ndc-postgres` instance.** The argument names are documentation-derived.

Before any deployment enables writes against a real connector:

- verify the procedure names and argument names against that connector's own
  `GET /schema` output, not against documentation;
- exercise an insert, an update and a delete end to end in a non-production
  environment, and confirm the predicate actually scoped the write;
- confirm with a second tenant on the same DataSource that the discriminator
  held.

The startup check that a mapped procedure exists in the connector's schema
catches a wrong *procedure* name. It does not catch a wrong *argument* name,
because NDC procedure arguments are not introspectable in a way that lets us
verify the semantics — which is exactly why this gap is written down rather than
assumed away.

## Consequences

### Good

- The isolation model ships reviewed and tested once, rather than being rebuilt.
- Writes are unusable by accident: three independent switches must all be set.
- The gap has a name and a checklist instead of being an unstated assumption.

### Bad, and accepted

- A deployment that enables writes without running the checklist above can
  configure a wrong argument name and lose the predicate. Mitigated by the
  startup validation, the translation-time check, and this ADR — not eliminated.
- Mutations remain less exercised than reads until a real connector is wired up.

## Revisit when

A real `ndc-postgres` is available in CI. At that point the checklist becomes an
integration test, this gap closes, and this ADR should be superseded rather than
quietly left to rot.
