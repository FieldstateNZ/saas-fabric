# ADR 0015 — A subject is named by its realm

- **Status:** Accepted
- **Date:** 2026-08-30
- **Applies to:** `fabric-core`, the OpenFGA adapter when it is built, and the runtime plane's enforcement path
- **Related:** [ADR 0013](0013-authorization-is-declared-in-the-platforms-words.md); [ADR 0014](0014-fabric-calls-openfga-as-the-operator.md); [ADR 0009](0009-operator-identity-is-not-tenant-identity.md)

## Context

[ADR 0014](0014-fabric-calls-openfga-as-the-operator.md) established that the
identity which *calls* the authorization service and the identity a check is
*about* are independent — a caller authenticated from one realm ran a check
about a subject from another and it answered `allowed: true`. That makes naming
subjects a decision this platform gets to make rather than one the
authorization service's authentication forces.

It also has to make it, because the service will not. Measured against OpenFGA
v1.19: `acme/`, `/subject` and `acme//doubled` are all accepted as ordinary
identifiers. Each is a *distinct* subject that will never match the one
intended, so a malformed value fails as a silently denied request rather than
as an error anybody sees.

## Decision

**A subject is `<realm>/<subject>`,** carried by a validated `SubjectId` in
`fabric-core`.

```text
acme/cb606ddc-f148-4193-8875-a84ea6a85e6c
```

The realm takes the same DNS-label rule a realm name takes everywhere else in
the platform. The subject is whatever the provider minted, bounded at 255
bytes, with whitespace and four characters refused: `:`, `#` and `*` are the
authorization service's — a type separator, a userset introducer, and the
wildcard — and `/` is this platform's own separator.

### Why qualify at all, when the store already implies the realm

Store-per-client means a decision is usually made in a store belonging to
exactly one realm, so the qualification looks redundant. It is not, and the
reason is what happens when something goes wrong.

A tuple written into the wrong client's store carrying a **bare** subject is a
grant that silently applies to whoever holds that subject *there*. The same
tuple carrying its realm matches nothing. Qualification turns a misrouted grant
from a security failure into an inert row — and misrouting is exactly the
mistake a platform with one store per client is positioned to make.

It is also simply true: a provider's subject is unique within its realm and
nowhere else. Reading one outside that namespace is a category error even when
the string happens to be unique in practice.

### Why `/` and not something else

Measured, not chosen by taste. OpenFGA accepts `/`, `|` and `.` in an
identifier and refuses `:` and `#`, which are structural to its own syntax. `/`
reads as a path, which is what this is, and it is the one separator that cannot
be confused with the service's own punctuation.

### Why it lives in `fabric-core`

The same reason `OperationKind` and `LogicalResourceName` do (ADR 0013): both
planes must mean the same thing by a subject, and neither may depend on the
other. A subject constructed one way in the runtime and another way by whatever
writes tuples would produce two identifiers for one person, and nothing would
fail until somebody was refused access they had been granted.

## Consequences

`SubjectId` cannot be constructed from an unchecked string, so a subject
carrying a separator, a reserved character, or whitespace is refused where it
enters rather than written into a tuple that can never match.

**This does not decide how the runtime obtains a subject.** It says what a
subject *is*, not which claim of which token it comes from; a provider's `sub`
is the obvious source and this ADR does not require it.

Operators and tenant users are named the same way, in different realms — which
is ADR 0009's separation expressed in one string rather than defended by
convention.
