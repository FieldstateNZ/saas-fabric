# 5. TLS trust roots come from the host, not from a compiled-in bundle

Date: 2026-08-13

## Status

Accepted.

## Context

`reqwest` is this platform's HTTP client, used by `fabric-connector-ndc` to
reach connector processes. Its default TLS feature, `rustls-tls`, is an alias
for `rustls-tls-webpki-roots`, which compiles Mozilla's CA certificate bundle
into the binary by way of the `webpki-roots` crate.

`webpki-roots` carries **CDLA-Permissive-2.0**. That licence is permissive —
no copyleft, no field-of-use restriction, no network clause — but it is a
Linux Foundation *data* licence and it is **not OSI-approved**. The licensing
constraint on this project names non-OSI licences specifically: they may not
be introduced without an explicit architectural decision.

So a decision was owed. The tempting one was to write it down as a narrow
`cargo-deny` exception — pinned to the crate and version, with a paragraph
explaining that a table of certificates is data rather than code. That
reasoning is sound as far as it goes, and it would have passed.

It was the wrong instinct. An exception is a permanent piece of policy
surface: it has to be re-verified on every upgrade, it teaches the next
reader that the allow list is negotiable, and it buys nothing if the
dependency was avoidable in the first place. The question worth asking is not
"can we justify this licence?" but "do we need this dependency at all?"

## Decision

Use `rustls-tls-native-roots` instead of `rustls-tls`. Trust roots come from
the host's certificate store at runtime, via `rustls-native-certs`.

The `cargo-deny` allow list stays as it was, and `exceptions` stays empty.

## Consequences

**The non-OSI dependency is gone rather than excused.** `webpki-roots` is no
longer in the resolved graph. What replaces it — `rustls-native-certs`,
`core-foundation`, `openssl-probe`, `security-framework`, `schannel` — is
uniformly MIT / Apache-2.0 / ISC, all already on the allow list. No exception
was needed, and none was written.

**It is also the better operational answer, which is what settles it.** A
compiled-in bundle is a fixed set of public CAs. This platform's HTTP calls
go to NDC connector processes, which in a real deployment sit behind a
private CA, a service-mesh CA, or plain in-cluster HTTP — none of which
Mozilla's bundle knows about. With `webpki-roots`, trusting an internal CA
means rebuilding the binary. With native roots, it means mounting a
certificate into the image, which is what an operator would expect and what
every other component in a cluster already does. The licence problem is what
prompted the look; the deployment story is why the change stands on its own.

**The image must carry a trust store.** This is the real cost, and it is
worth stating plainly rather than burying. A `scratch` or fully distroless
base with no `/etc/ssl/certs` will fail to verify *any* certificate, and it
will fail at request time rather than at boot. Deployments must either use a
base image with `ca-certificates` installed, or mount the trust store in.
Either is ordinary practice; neither is automatic.

**Per-target dependencies grew slightly.** Reading the platform trust store
means talking to Security.framework on macOS and `schannel` on Windows, so
the graph picked up six small crates. They are already-approved licences and
none of them are on the request path — the store is read once when the client
is built.

## Alternatives considered

**Write the exception.** Rejected, as above: it justifies a dependency
instead of removing one, and it leaves a recurring verification obligation
behind for no benefit.

**Vendor a trust bundle ourselves.** Rejected. It moves the same data into
this repository, where it becomes our job to keep current — the certificate
distrust events of the last few years are exactly the maintenance nobody here
should be signing up for.

**Both feature flags together.** `reqwest` permits it; the effect is that
native roots and the compiled bundle are both loaded. Rejected because it
reintroduces `webpki-roots` and therefore the licence question, while making
the trust set harder to reason about rather than easier.
