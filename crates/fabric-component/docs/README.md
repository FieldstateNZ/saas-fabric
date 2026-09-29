# fabric-component

What a SaaS Fabric component says it is: the **component descriptor**, its
renderer, and the configuration-field, resource and capability shapes it
shares with the catalogue. The contract of
[ADR 0026](../../../docs/decisions/0026-a-component-describes-itself-in-an-artifact-attached-to-its-image.md);
this crate is its slice 1, and nothing publishes or reads a component
descriptor from a registry yet.

## Why this crate exists

A component's publisher knows what the software is — its images, the
platform capabilities it needs, the configuration a client supplies, the Data
API resources it exposes — and until now an operator retyped all of it into
the console. ADR 0026 has the software say so itself, in a small JSON
document attached to its primary image as an OCI artifact.

Two readers need that document's shapes, on two sides of the graph: the
catalogue in `fabric-client-model` (control plane), which will select a
described component, and `fabric-platform-management` (neither plane), which
will decide from it whether a version is a release unit. A shape either one
owned would put the other on the wrong side of an edge. So the shapes live
here, in neither plane, on the same footing as `fabric-runtime-publication`:
its only internal edges are `fabric-core` and `fabric-runtime-publication`,
it has no transport, and anything outside the runtime plane may depend on it.
Its edge to `fabric-runtime-publication` puts it behind ADR 0018's publisher
fence: no runtime-plane crate may reach it.

**One declaration of each shape.** `ConfigurationField`, `FieldKind` and
`ApplicationResource` moved here from `fabric-client-model` with their
validators, byte for byte, and `fabric-client-model` re-exports them at their
old paths. A component descriptor's declared fields and resources are the
catalogue's own shapes checked by the catalogue's own rules — never a second
copy that could drift.

## Key concepts

| Concept | Type | What it is |
|---|---|---|
| Component descriptor | `ComponentDescriptor` | The published, valid v1 document. Read with `from_json` / `from_artifact`; written with `to_json`. |
| Spec | `ComponentSpec` | What it says: `name`, `title`, `description`, `version`, `images`, `capabilities`, `fields`, `resources`. |
| Authored source | `ComponentSource` | `component.yaml`: the same document without `version` and without digests. |
| Image | `ImageReference` | A `Repository` and a `Digest`, keyed by `Role`. |
| Capability | `PlatformCapability` | ADR 0021's closed list of seven, as a type. |
| Refusal | `ContractError` | `Invalid { detail }`; `OtherRegistry { detail }` for images on more than one registry; or `UnsupportedVersion { found }` for a version this build does not read. |

The validated strings — `ComponentName` (a DNS label), `Role` (an
identifier), `ComponentVersion` (SemVer without build metadata or a `v`),
`Digest` (lower-case `sha256`) and `Repository` (fully qualified, one
spelling) — each validate on construction and on deserialisation.

## How the pieces fit

```
component.yaml ──ComponentSource::from_yaml──▶ ComponentSource
                                                   │ render(version, digests by role)
                                                   ▼
                           ComponentDescriptor ──to_json──▶ canonical bytes (≤ 16 KiB)
                                   ▲                                 │ attached to the primary image
                                   └──from_json / from_artifact──────┘ (slice 5; read in slice 2)
```

Reading is strict and ordered, the first failure deciding: the byte bound,
UTF-8, no repeated key in any JSON object, the envelope (`apiVersion`,
`kind`) before the spec, the spec's shape with unknown fields refused, and
then `validate`, which applies the counts and bounds of ADR 0026 section 2,
the one-registry rule, the catalogue's field and resource validators, and —
for the descriptor only — a refusal of Unicode format characters in declared
text.

## Getting started

```rust
use fabric_component::{ComponentDescriptor, ComponentSource, Digest, Role};
use std::collections::BTreeMap;

let source = ComponentSource::from_yaml(&std::fs::read_to_string("component.yaml")?)?;
let digests: BTreeMap<Role, Digest> = /* the digest each role's image was pushed at */;
let descriptor = source.render("1.4.0", &digests)?;
let bytes = descriptor.to_json();
assert_eq!(ComponentDescriptor::from_json(&bytes)?, descriptor);
```

## Common tasks

- **Check this repository's `component.yaml`:**
  `cargo run -q -p fabric-component --example descriptor -- check component.yaml`
- **List its roles and repositories:**
  `cargo run -q -p fabric-component --example descriptor -- images component.yaml`
- **Render it for a release:**
  `cargo run -q -p fabric-component --example descriptor -- render component.yaml --version 0.3.0 --digest runtime=sha256:… --digest controlPlane=sha256:… --digest console=sha256:… --out component.json`
- **Read an attached artifact:** `ComponentDescriptor::from_artifact(artifact_type, layer_bytes)`;
  an `artifactType` of the family at another version is `UnsupportedVersion`,
  naming it as `v2` — the spelling `from_json` uses for a later `apiVersion`.
- **Find which role an image plays:** `descriptor.role_of(&repository, &digest)`.

## Gotchas

- **v1 is frozen.** Its fields and enumerations never change, and its rules
  only relax. A new field (even an optional one), a new capability, or a
  stricter rule is a new component descriptor version, stated by both the
  `artifactType` and the `apiVersion`.
- **The moved validators' messages are the catalogue's API.** The console
  shows them and tests in `fabric-client-model` and `fabric-control-plane`
  assert on them. Change a message and you change the catalogue.
- **Format characters are refused only by the descriptor's parser**, not by
  `validate_fields`, which also checks authored catalogue content and every
  frozen copy on each read; tightening it would make stored catalogues
  unreadable.
- **A repository has one spelling.** `docker.io/nginx`, `index.docker.io/…`,
  `:443`, an upper-case host and an IP-address host (anything a URL parser
  reads as IPv4: `10.0.0.1`, `127.1`, `0x7f000001`, `registry.123`) are all
  refused, not normalised. No repository is named by two roles.
- **`to_json` is canonical** — compact, fields in declared order, maps sorted,
  every field present — so equal descriptors have equal bytes. Do not add
  `skip_serializing_if` to anything a descriptor carries. Because every
  default is written, the canonical form can be larger than the document
  read; `validate` bounds the canonical form, so a descriptor that exists
  always writes bytes `from_json` reads.
- **`from_yaml` trusts its input.** It is the publisher's own file; what a
  server reads is only ever the JSON.
- The crate is `fabric-component` (hyphen); the Rust path is
  `fabric_component` (underscore).
