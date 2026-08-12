# Scope and non-goals

The platform specification lists non-goals in §32. This records the two that are
easiest to overstate when describing what SaaS Fabric does, so the claim stays
honest as the documentation grows.

## What the abstraction actually is

**SaaS Fabric abstracts resource *resolution*, not datastore *semantics*.**

An application asks for `customers` and never learns which database answered.
That is real and it is the point. What it does **not** mean is that a tenant can
be moved between fundamentally different datastores and have everything keep
working.

## Portability is not transparent

It would be easy to read "logical data source" and conclude that a tenant on
PostgreSQL could be migrated to SQL Server, MySQL or MongoDB with no application
change. The architecture does not promise that, and the documentation must not
imply it.

What genuinely is portable:

- **Which physical instance** serves a tenant — a different server, a different
  region, a dedicated database instead of a shared schema. This is the migration
  story in §19 and it works.
- **The isolation model** — a tenant can move between a dedicated database, a
  per-tenant schema and a shared table with a discriminator without an
  application change.

What is not:

- **Datastore semantics.** Type systems, null handling, collation and ordering,
  transaction isolation, and what a "row" even is all differ. The Data API's
  operation model is deliberately small — equality, ordering, projection,
  paging — precisely because that is the subset every backend performs the same
  way. It is not a universal query abstraction and adding one would either
  refuse common queries or translate them unfaithfully.
- **Schema.** §32 is explicit that the platform does not remove the need for
  data schemas. A logical resource maps to a physical collection; that mapping
  has to exist and someone has to maintain it.
- **Connector capability parity.** Backends differ in what predicates they can
  express, and the platform refuses operations a backend cannot express rather
  than approximating them. Two tenants on different backends may therefore have
  genuinely different capability surfaces, and the Data API reports that rather
  than hiding it.

The honest framing: **the abstraction boundary is logical resource resolution,
not pretending all infrastructure behaves identically.**

## Not in the first release

Deliberately deferred, each for its own reason rather than for lack of time:

| Not built | Why |
|---|---|
| Configuration, Feature, Storage, Events, Secrets APIs (§27) | Same architecture, separate slices. The binding format already carries their state, so adding them changes no tenant model. |
| The Experience API | Belongs in SaaS Fabric, but it composes identity, tenant enablement, permissions, feature state and application UX declaration — it needs the foundation merged underneath it first. |
| Reconciliation itself | The runtime reads what a controller writes. `ResourceSource` is the contract between them, and the file-backed adapter is one implementation of it. |
| A JWKS refresher | Only relevant in the opt-in defence-in-depth identity mode, where key rotation currently means rebuilding the reader. |
| A migration engine | §19's cut-over is expressible today (provision, migrate, validate, rebind, drain). Automating it is separate work, and the binding model deliberately does not assume a tenant's DataSource is immutable. |
