# When a slice is done

One rule, and the evidence for it.

> **A vertical slice is not complete until its primary operator workflow is
> exercised through the real HTTP surface. Component, adapter, and handler
> tests are necessary but do not establish product completion.**

## Why this is a rule and not a preference

The Secrets slice had, at the point it looked finished:

- an adapter suite against a **real OpenBao**, proving namespace isolation,
  KV v2 versions, check-and-set and delete
- unit tests over paths, values, redaction and the service's boundary
  resolution
- a console that rendered, built, linted and passed its own tests

Three of its five operations returned `400` to every call.

`ClientPath` read its path parameter positionally, so on a route carrying a
second parameter it failed with *"the request path names no client"*. Metadata,
write and delete were dead. The adapter tests passed because they call the
adapter; the handler tests passed because there weren't any that composed a
two-parameter route; the console passed because nothing had clicked past
Reveal.

Every layer was green over a feature that mostly did not work.

## What the rule asks for

For each slice, one test that goes through the surface an operator actually
uses, doing the thing the slice exists to let them do. For Secrets that is:

```text
create → list finds it → metadata without values → reveal → update with the
version read → stale update refused → delete → gone
```

It is not a replacement for the layer tests. It catches a different class of
defect: composition. Layer tests are designed *not* to cross the seams where
these bugs live, which is why they cannot find them however many there are.

## What it does not ask for

Not a browser. The Secrets slice's HTTP tests drive the router in-process,
which is enough to catch every composition defect above — a route that does not
match, an extractor that cannot read its parameter, a status that collapses two
different failures, a header that was never set.

Not a running dependency for every test. The real-store and real-provider
suites are gated on explicit activation and say so loudly when skipped; the
in-process HTTP tests use fakes and run on every commit.

## The related habit

A test that cannot fail is worse than no test, because it reports success. This
repository has produced several, and each was found only by breaking the thing
it was meant to protect:

- a mixed-case fixture, absent, made a `to_lowercase()` bug invisible to both
  the socket test and the real engine
- an oversized-body test asserted only "not 200", so removing the limit
  entirely still passed
- a probe that restored a file with `git checkout` restored nothing, because
  the file was untracked, and the "restored" run was still mutated

So: mutate the thing, watch the test fail, then keep the test. The fixtures are
part of the proof, not scaffolding around it.
