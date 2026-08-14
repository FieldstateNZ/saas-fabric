# 7. Isolation is checked against an observed fact, not a declared label

Date: 2026-08-14

## Status

Accepted. Supersedes the rule in ADR 0006, which it keeps as one of two
conditions.

## Context

ADR 0006 closed a cross-tenant leak: `Database` and `Schema` isolation
contribute no predicate, so on a DataSource many tenants share they isolate
nothing. The rule it introduced was **a `Shared` DataSource may only serve
`Discriminator` isolation**, enforced at resolution.

`PlacementClass` has six variants. That rule inspected one.

The other five are `Dedicated`, `HighAvailability`, `Regulated`,
`Development` and `Ephemeral`. Only the first makes any claim about
tenancy. A clustered Postgres shared by fifty tenants is honestly labelled
`HighAvailability`; a sandbox shared by every developer is honestly
labelled `Development`. Both took structural isolation without complaint.

The shipped example already had it. `initech-dedicated` — `placement:
"regulated"`, `accepts_new_tenants: true` — served `initech` under
`Database` isolation. Structurally identical to the configuration ADR 0006
was written about, wearing a label the guard did not read. Two tenants there
produce byte-identical NDC request bodies.

ADR 0006 predicted this and did not act on it. Its own "Alternatives
considered" says a cross-scan "would catch two tenants sharing a DataSource
that is merely *mislabelled* `Dedicated` … and is worth adding, but it is not
what closes this hole." It was never added, and "mislabelled" turned out to
understate the problem: no mislabelling was required, only a label that says
nothing about tenancy.

The deeper fault is the kind of fact the rule consulted. **A placement class
is a claim by an operator.** The runtime cannot verify it, so any rule keyed
on one has this shape of hole permanently — close `HighAvailability` and the
next deployment writes `Regulated`.

## Decision

Structural isolation additionally requires that **no other tenant reaches
the same destination**, which is a fact the runtime observes rather than a
label it trusts.

"Destination" is the connector plus the connection selector, not the
DataSource id — two ids naming one connector and one connection are one
physical database, which is precisely what a label-based rule misses.

The label rule from ADR 0006 stays. A `Shared` DataSource is refused
structural isolation even when only one tenant currently occupies it,
because `Shared` is an operator saying more tenants are coming.

`ConnectionSelector` is also no longer serde-defaulted. Omitting `connection`
used to yield `Default`, whose own documentation says it is "only valid where
one connector serves exactly one physical database" — a precondition nothing
checked, reachable by saying nothing at all. Omission is now a
deserialisation error.

## Consequences

**The check needs no cross-registry knowledge when it is derived.** That was
the objection to a cross-scan, and it dissolves once the question is
decomposed: which tenants occupy a DataSource comes from the tenant snapshot
alone, and which DataSources select one connector-and-connection comes from
the DataSource snapshot alone. Each registry derives its own half when it
installs a snapshot, so the facts always describe the map that was actually
installed — the same invariant `MergedSnapshot` already carries. They are
combined only per request, which is O(1).

**It survives a refresh**, unlike a startup-only scan. That matters because
both registries reconcile independently and continuously; a configuration
safe at boot can stop being safe an hour later.

**Reuse alone is not a refusal.** One tenant holding a writable and a
read-only DataSource over one database is legitimate and common. So the
destination fact names its peers rather than issuing a verdict, and
occupancy is checked per peer.

**It catches configuration equality only, and that limit is real.** Two
differently-named connections reaching one database, or two `SecretRef`s
resolving to one credential, still read as two destinations. Detecting that
needs a connector round trip on the request path, which §6 forbids.

**Four existing test fixtures were wrong and now fail.** One —
`two_tenants_can_share_one_data_source` — was asserting the leak as a
feature. Two more hardcoded a single connection name for every DataSource
id, so any test registering two DataSources was quietly building one
physical database. That the new rule found them is the argument for it, and
it is the second time on this branch that a correctness rule has exposed
fixtures encoding the bug.

**The shipped example changed again.** `initech-dedicated` no longer
advertises `accepts_new_tenants`. It was safe as shipped — one tenant, a
distinct secret reference — but advertising capacity on a DataSource whose
only tenant depends on structural isolation means the second tenant to
arrive breaks both of them. The build-time guard over the examples now
checks co-tenancy and destination reuse rather than the label, and refuses
that advertisement outright.

## Alternatives considered

**Widen the allowlist to the placements that assert single tenancy** —
`Dedicated` and `Regulated`, say. Rejected: it is the same rule with a
longer list, and it fails the same way the moment someone picks the honest
label for a shared cluster.

**A cross-scan at startup only.** Rejected as sufficient, for the reason
ADR 0006 already gave: both registries refresh independently, so it leaves a
window that widens with uptime. It is subsumed here — the per-request check
sees everything a startup scan would, and keeps seeing it.

**Verify the destination by asking the connector.** This is the only thing
that would close the configuration-equality gap. Rejected: it puts a network
call on the request path for a fact that changes only at reconciliation, and
§6 exists to keep that path free of exactly this.
