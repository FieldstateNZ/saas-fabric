# Dependency licence policy

- **Status:** Accepted
- **Applies to:** every crate in this workspace, and every dependency —
  direct or transitive — that enters `Cargo.lock`.
- **Related:** [ADR 0001 — NDC as the internal connector boundary](../decisions/0001-ndc-as-connector-boundary.md)
  §"Licence verification". Read that section first — it is the incident this
  policy generalises from, and this document does not repeat its detail, only
  its conclusion.

## The policy

SaaS Fabric is licensed **Apache-2.0** (see [`LICENSE`](../../LICENSE) and
`workspace.package.license` in the root `Cargo.toml`). Everything this
platform ships under that licence must be free to combine with it.

1. **Every dependency requires an approved OSS licence.** "Approved" means
   present on the allow list enforced by [`deny.toml`](../../deny.toml) at
   the repository root — not "looks open source", not "the project has a
   public GitHub repo".
2. **Apache-2.0 is preferred** when a choice exists between otherwise
   equivalent crates. It is the licence this project ships under, it grants
   an explicit patent licence (MIT and BSD do not), and it is compatible
   with everything else on the allow list.
3. **Verification is per crate, per version — never per ecosystem.** "This
   dependency comes from a well-known open-source project" is not a licence
   check. A repository, an org, or a language ecosystem being open source
   says nothing about whether one specific published crate carries a licence
   grant at all, let alone a compatible one. See the `ndc-spec` incident
   below: it happened inside an ecosystem (Hasura's NDC tooling) where every
   sibling repository genuinely is Apache-2.0.
4. **Unlicensed code must never enter the dependency graph.** No `license`
   field, no `LICENSE` file, no licence statement in the README means *all
   rights reserved* by default — there is no implicit grant to fall back on,
   no matter how small the code or how convenient the dependency. `deny.toml`
   fails the build on any crate cargo-deny cannot attribute to an allowed
   licence; there is no wildcard fallback that would let one through
   silently (see "How this is enforced" below).
5. **Protocol interoperability does not justify copying implementation
   code.** Implementing a published wire format or specification so this
   platform can talk to a third-party system is a different act from
   depending on that third party's reference implementation, and the first
   does not license the second. If a specification is open (a document, an
   RFC, a published API contract) but its reference implementation's licence
   is missing, incompatible, or unverified, implement the wire format from
   the specification instead of importing the implementation. This is
   exactly what `fabric-connector-ndc` does for Hasura's NDC protocol — see
   ADR 0001.
6. **Exceptions require an ADR.** If a genuinely-needed dependency carries a
   licence outside the allow list — or a licence cargo-deny's classifier
   cannot cleanly attribute (a non-SPDX `license-file`, a hand-written
   licence blend) — the fix is never to widen `deny.toml`'s allow list
   silently. Either:
   - add a narrowly-scoped, version-pinned `[[licenses.clarify]]` or
     `[[licenses.exceptions]]` entry in `deny.toml` naming the exact crate
     and version, with a comment explaining what was verified and why it is
     safe; or
   - write a new ADR recording what licence was found, at what version, and
     why it is (or is not) acceptable — the same shape as the "Licence
     verification" table in ADR 0001.

   Either way, the exception is visible in a diff and in git blame. It is
   never just a wider `allow` list that quietly admits the next unrelated
   crate too.

## The incident this policy comes from

While adopting Hasura's Native Data Connector (NDC) protocol as the internal
connector boundary (ADR 0001), every candidate dependency was checked
individually, at a specific version, rather than assumed acceptable because
it came from an open-source ecosystem:

| Component | Version checked | Licence | Verdict |
|---|---|---|---|
| `hasura/ndc-spec` (publishes the `ndc-models` crate) | `v0.2.13` | **None** | ❌ Rejected |
| `hasura/ndc-sdk-rs` | `v0.9.0` | Apache-2.0 | ✅ Acceptable |
| `hasura/ndc-postgres` | `v3.1.0` | Apache-2.0 | ✅ Acceptable — consumed over HTTP, never linked |

`hasura/ndc-spec` — the repository that defines the exact wire types this
platform needed — has no `LICENSE` file anywhere in the repository (root,
`main`, or the `v0.2.13` tag), no `license` field in its workspace or
`ndc-models` manifest, and no licence statement in its README. A GitHub code
search for `filename:LICENSE` in that repository returns zero results.
Absent an explicit grant, the default is *all rights reserved*, so
`ndc-models` was rejected as a dependency regardless of how convenient it
would have been to import the reference types directly.

The reason this is worth restating here, outside ADR 0001: the surrounding
ecosystem reads as uniformly open source. `ndc-sdk-rs` and `ndc-postgres` are
genuinely, verifiably Apache-2.0, published by the same organisation, in
sibling repositories, for the same protocol family. Only the middle one —
the protocol-definitions crate, which is exactly the one a naive integration
would have depended on — carries no licence at all. Checking "is this
ecosystem open source" would have said yes at every step. Only checking the
specific repository, crate, and version caught it.

The response was not to abandon NDC. The specification itself is a published
protocol, not the unlicensed crate. `fabric-connector-ndc` hand-writes the
subset of NDC's wire types this platform actually uses, implementing the
protocol for interoperability without importing `ndc-models`. That is rule 5
above, applied to the case that produced it.

## Verifications since

Each new dependency has been checked the same way — the specific crate, at the
specific version in `Cargo.lock`, not the ecosystem it came from.

| Component | Version checked | Licence | Verdict |
|---|---|---|---|
| `serde_norway` | `0.9.42` | MIT OR Apache-2.0 | ✅ Acceptable — Apache-2.0 is taken (rule 2) |
| `unsafe-libyaml-norway` | `0.2.15` | MIT | ✅ Acceptable |

Both arrived with the control plane, which reads and writes client desired
state as YAML because the repository holding it is edited by humans. The
obvious choice was `serde_yaml`, whose author has archived it; `serde_norway` is
the maintained fork. The dependency is confined to `fabric-client-model` —
nothing in the runtime plane parses YAML.

`unsafe-libyaml-norway` is libyaml transpiled to Rust, and it does contain
`unsafe`. That is worth noting and is not a policy violation: `unsafe_code =
"forbid"` in `[workspace.lints.rust]` applies to this workspace's own crates,
and every non-trivial parser in the Rust ecosystem depends on something like
it. The relevant question this policy asks is about the *licence*, and both
crates answer it.

## How this is enforced

[`deny.toml`](../../deny.toml) at the repository root is the mechanical
enforcement of this policy, run via `cargo deny check` (see
[`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)). It fails the
build on:

- any licence not explicitly on the `[licenses] allow` list;
- any crate cargo-deny cannot attribute a licence to at all (the unlicensed
  case above);
- copyleft licences with network or field-of-use effects incompatible with
  an Apache-2.0 platform — BUSL/BSL, SSPL, and every AGPL variant by name;
- source-available licences that are not OSI-approved open source;
- known-vulnerable or explicitly banned crate versions (`[advisories]`,
  `[bans]`);
- dependencies pulled from anywhere other than the crates.io registry
  without an explicit exception (`[sources]`) — a git or path dependency is
  itself an unpublishable, mutable-tag risk of the same shape ADR 0001
  rejected `ndc-models` over.

`deny.toml`'s allow list and the rationale for each entry on it are
maintained alongside the config, not duplicated here, so the two cannot
drift apart.
