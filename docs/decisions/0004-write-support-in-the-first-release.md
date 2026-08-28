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
3. **Every mapped argument is checked against the connector's own schema at
   startup.** An update or delete without a `filter_argument` is rejected;
   an insert or update without a `payload_argument` is rejected; and any
   argument the procedure does not declare — or that is declared with the
   wrong kind, a `filter_argument` pointing at a non-predicate, a
   `payload_argument` pointing at a predicate — is rejected. The predicate
   half is re-checked at translation time. Both exist deliberately: the cost
   of this one failing open is other tenants' data.
4. **A mutation reaching the connector with no predicate is refused**, even
   though `for_target` should always have added one. That check is there for the
   case where something bypassed it.
5. **A write may not report success unless the count agrees with what was
   sent.** A response claiming fewer rows than were submitted is
   `500 partial_write`, explicitly non-retryable, and says the platform cannot
   determine which rows landed. One claiming more is a malformed response.

## The gap, stated plainly

**No procedure mapping in this repository has been executed against a real
`ndc-postgres` instance.** The argument names are documentation-derived.

Before any deployment enables writes against a real connector:

- ~~verify the procedure names and argument names against that connector's own
  `GET /schema` output~~ — now automatic; the connector refuses to build
  otherwise. What remains manual is confirming the payload argument's expected
  **value shape**, which the schema does not describe;
- exercise an insert, an update and a delete end to end in a non-production
  environment, and confirm the predicate actually scoped the write **and that
  the affected-row count means rows, not statements**;
- confirm with a second tenant on the same DataSource that the discriminator
  held.

### Two things this ADR originally said, both wrong

The first draft claimed **"NDC procedure arguments are not introspectable in a
way that lets us verify the semantics."** That was false when it was written.
`schema_response.jsonschema` v0.2.13 requires `arguments: {name → ArgumentInfo}`
on every `ProcedureInfo`, and a predicate argument is typed
`{"type": "predicate", "object_type_name": …}`. Names are fully introspectable
and the predicate type is checkable. This crate was discarding them at parse:
`NdcNamed` had one field, so a schema declaring arguments round-tripped to
`[{"name": "delete_customers"}]`.

The consequence was not academic. A `filter_argument` naming an argument the
procedure never declared passed configuration validation *and* translation, and
the tenant predicate went out under a name nothing read — an unscoped delete
against a connector that ignores unknown arguments. The one check that would
have turned a documented data-loss gap into a startup failure was declined on a
premise the code disproves.

It also claimed **"the startup check that a mapped procedure exists in the
connector's schema catches a wrong procedure name."** There was no startup
check; `ensure_procedure_exists` ran at translation time only. There is one
now.

What is *genuinely* not introspectable is much narrower: the payload argument's
expected **value shape** — whether `objects` wants an array of row objects or
something else. That remains documentation-derived.

### And a second gap, underneath the first

`affected_rows` is not an NDC concept on `/mutation` at all.
`MutationOperationResults` has one variant carrying one opaque `result` field:
no per-row status, no error variant, no count. (`affected_rows` exists only in
the experimental *relational* mutation API.) The word "atomic" does not appear
in the specification source.

So the count this platform returns is a heuristic read of a connector-private
result shape — `null` reads as 0, an object without a count reads as 1. Decision
item 5 above exists because of this: the platform cannot trust the number, so it
refuses to report success when the number disagrees with what it sent.

## Consequences

### Good

- The isolation model ships reviewed and tested once, rather than being rebuilt.
- Writes are unusable by accident: three independent switches must all be set.
- The gap has a name and a checklist instead of being an unstated assumption.

### Bad, and accepted

- ~~A deployment that enables writes without running the checklist can configure
  a wrong argument name and lose the predicate.~~ No longer possible: it fails
  at startup.
- A hostile row now poisons its whole batch. The discriminator column is
  refused case-insensitively before a `Row` is built, so a batch containing one
  such row is a `400` and nothing dispatches — where previously the value was
  silently overwritten and the batch succeeded. Refusing is the right side, but
  it is a behaviour change worth stating.
- The affected-row count is a heuristic, and a connector whose procedure returns
  an unusual result shape will now surface as `partial_write` rather than a
  quiet wrong number. Louder, and more likely to need an operator.
- Mutations remain less exercised than reads until a real connector is wired up.

## Revisit when

A real `ndc-postgres` is available in CI. At that point the checklist becomes an
integration test, this gap closes, and this ADR should be superseded rather than
quietly left to rot.
