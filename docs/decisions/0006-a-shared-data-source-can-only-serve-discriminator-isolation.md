# 6. A shared DataSource can only serve discriminator isolation

Date: 2026-08-13

## Status

Accepted.

## Context

An adversarial review of this branch found a cross-tenant data leak in the
shipped example configuration. It is worth writing down exactly, because the
shape of the mistake is more instructive than the fix.

§18 names three isolation models, and the runtime models all three:

- **Database** — the tenant has its own database.
- **Schema** — the tenant has its own schema inside a shared database.
- **Discriminator** — the tenant's rows live alongside others in a shared
  table, separated by a column.

Only the third contributes a predicate. `IsolationModel::tenant_predicate()`
returns `None` for the first two, deliberately and correctly: their
separation is meant to be *structural*, coming from the connection reaching
somewhere else rather than from a `WHERE` clause. `QuerySpec::for_target`
even documents this — "a per-tenant schema is normally selected by the
connection itself".

The problem is what "the connection" means in this model. A
`ConnectionSelector` lives on the **DataSource**, and a DataSource is shared
by every tenant bound to it. So for a tenant on a shared DataSource under
`Database` or `Schema` isolation there is:

- no predicate, because the model contributes none;
- no schema qualification, because `IsolationModel::schema()` had **zero
  production call sites** — the field was read by nothing;
- no distinct connection, because there is one per DataSource.

Nothing separated two such tenants. Not weakened isolation — none, and
silent: two tenants read each other's rows and delete each other's records
with no error anywhere.

`examples/tenants.json` shipped exactly this. `globex` was bound to
`shared-postgres-02` — `placement: "shared"`, `accepts_new_tenants: true` —
with `{"kind": "schema", "schema": "globex"}`. Adding a second tenant the same
way, which is precisely what `accepts_new_tenants: true` invites, was a full
cross-tenant read and write.

Three things let it through. The isolation-model docs described the intent
without stating the precondition. `tenant_isolation.rs` — whose own header
calls it the most important file in the suite — covered `Database` and
`Discriminator` and never constructed `Schema` at all. And
`scope-and-non-goals.md` advertised movement between all three models as
something that worked.

## Decision

**A DataSource whose placement is `Shared` may only serve `Discriminator`
isolation.** Any other placement may serve any model.

`RuntimeResolver::resolve_data_source` checks this and returns
`ResolveError::IsolationNotEnforceable`, which is a 500 with a generic public
message. The check runs on every request, so it survives a refresh that
introduces the combination after startup — a startup-only validation would
not.

## Consequences

**Structural isolation now requires a DataSource that is not declared
shared.** In practice that means one tenant per DataSource for `Database` and
`Schema`, which is what those models always required; it was simply never
enforced.

**Placement is read on the request path, for the first time.** This is a
real, narrow exception to §17 and to
`crates/fabric-tenant-runtime/src/resolution/placement_inertness_tests.rs`,
so it is worth being precise: the check can **veto**, never **choose**. With
a healthy dedicated DataSource sitting right there, an unenforceable binding
still fails rather than being quietly re-pointed at it. There is a test for
exactly that.

**The example changed.** `shared-postgres-02` became `globex-schema-01` —
`placement: dedicated`, connection `globex-schema`, `accepts_new_tenants:
false` — and `globex` binds to it. The example still demonstrates all three
isolation models; it now demonstrates the third one correctly.

**Two existing test fixtures were wrong and are now right.** `replica-01` and
`draining-01` both paired `Shared` placement with `Database` isolation. They
were testing capability semantics, where placement is incidental, so both
became `Dedicated`. Two of this branch's own placement-inertness tests had
the same defect and were corrected the same way. That the new check found
them is the argument for the check.

**`IsolationModel::Schema` is still not doing anything.** This decision makes
the dangerous configuration impossible; it does not make `Schema` isolation
*work*. On a dedicated DataSource, `Schema` behaves identically to
`Database`: one tenant, one connection, and the `schema` field still read by
nothing. It is safe and it is redundant.

Making it real means routing the schema per request — NDC's
`request_arguments`, which this codebase already uses for connection
selection, is the obvious mechanism. Until that exists, an operator who wants
per-tenant schemas inside one physical database must model each schema as its
own DataSource with its own connection, as the example now does.

## Alternatives considered

**Serve it and log a warning.** Rejected. The argument for it is that most
such deployments are probably single-tenant in practice. "Probably" is not a
property to bet a tenant boundary on, and the failure mode is silent
cross-tenant access rather than anything an operator would notice.

**Delete the `Schema` variant.** Rejected, narrowly. §18 requires the model,
and deleting it would leave no vocabulary for a deployment that has genuinely
solved schema routing at the connection layer. But a named isolation model
that enforces nothing is a real trap, and the previous section says so
plainly rather than leaving the gap for the next reader to discover.

**Validate at startup only, by cross-referencing tenants against
DataSources.** Rejected as the *sole* mechanism: both registries refresh
independently at runtime, so a configuration that was safe at boot can stop
being safe an hour later. A per-request check has no such window. A startup
cross-scan would still be a useful addition — it would catch two tenants
sharing a DataSource that is merely *mislabelled* `Dedicated` — and is worth
adding, but it is not what closes this hole.
