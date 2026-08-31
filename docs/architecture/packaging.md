# Packaging and release

- **Status:** Implemented
- **Applies to:** the three artifacts this repository publishes.
- **Built by:** [`Dockerfile`](../../Dockerfile),
  [`apps/control-plane-ui/Dockerfile`](../../apps/control-plane-ui/Dockerfile).
- **Published by:** [`.github/workflows/release.yml`](../../.github/workflows/release.yml).
- **Deployed by:** `saas-fabric-platform`, which is a different repository and
  a deliberate boundary — see below.

## What this repository publishes

| Image | Contains | Runs on |
|---|---|---|
| `ghcr.io/fieldstatenz/saas-fabric` | `fabric-api` — the runtime plane | the product edge |
| `ghcr.io/fieldstatenz/saas-fabric-control-plane` | `fabric-control-plane-api` | the operator plane |
| `ghcr.io/fieldstatenz/saas-fabric-control-plane-ui` | the operator console, as static files | the operator plane |

Three images because they are three deployments on two networks with different
availability requirements. The runtime plane must keep serving tenants while
the control plane is down; that is not a property you can have from one image.

The runtime plane's name is unchanged because `saas-fabric-platform` already
expects it.

## What this repository does not publish

**Anything that says where it runs.** No manifests, no namespaces, no
hostnames, no replica counts, no secrets. An image says what a process *is*;
`saas-fabric-platform` says where it runs and what it is given, and pins an
explicit version to do it.

## Two Rust services, one compile

`Dockerfile` has one builder stage and two final stages, selected with
`--target`. Compiling the workspace twice would double the slowest part of a
release for nothing, and BuildKit reuses the stage across both builds in a
release run.

The runtime images are **distroless** — no shell, no package manager, running
as `nonroot`. There is nothing in them to exec into, which matters most for the
runtime API, which is reachable by every tenant's application. The `cc` variant
rather than `static` because the binaries link glibc, and because it carries
the certificate store that `rustls-tls-native-roots` reads (ADR 0005).

About 62 MB each.

## The console is built with node and served without it

`npm run build` is `tsc -b && vite build`, so a type error fails the image
rather than shipping a bundle that never type-checked. The result is copied
into `nginx-unprivileged` — the stock nginx image starts as root to bind port
80 and then drops privileges, and a static file server on the operator plane
has no reason to begin life as root.

`nginx.conf` sets a strict `Content-Security-Policy` — `default-src 'self'`,
no framing, no inline anything — which the console satisfies without exception
because it loads only its own bundle and talks only to its own origin.

**It does not proxy `/api`.** The console calls the control-plane API on the
same origin, so the **operator ingress must route `/api` to the API service and
everything else to this container**. That routing belongs to the platform
(specification §8); putting it here would mean the console's container knowing
the API's address.

### One nginx behaviour worth knowing

`add_header` is inherited from an outer block *only if the inner block declares
no `add_header` of its own*. A `server`-level security header beside a
`location`-level `Cache-Control` therefore vanishes from every response that
location serves — including the document, which is where it matters most.

The first version of this config had exactly that shape. Running the image
showed `X-Frame-Options` missing from `GET /`; the config now sets its headers
once at server level and varies caching by content type through a `map`, so
there is no `add_header` in any location to suppress them.

## Versions

A **tag** publishes, and nothing else does. `workflow_dispatch` builds all three
images and pushes none, so packaging can be exercised without minting a version.

Each image gets two tags: the version, and `sha-<commit>`. **Never `latest`** —
the platform pins an explicit version (§25), and a floating tag is how a cluster
ends up running something nobody chose. The commit tag answers "what is actually
in this image" when the version does not.

The tag's **version core** must match `workspace.package.version`, and the
release fails if it does not. A tag that disagrees would publish an image whose
name says one thing and whose source says another.

## Previews

A tag carrying a SemVer prerelease part is a **preview**:

```text
v0.3.0                     a stable release
v0.3.0-preview.20260831.42 a preview of it
```

Both are built and published identically — same gates, same context, same
labels. A preview is not a lower standard of artifact. It is the same artifact,
minted more often, and stated not to be a release.

"Core" rather than "the whole tag" is what admits one. `v0.3.0` and
`v0.3.0-preview.42` share the core `0.3.0`, so both are publishable while the
workspace is on `0.3.0`, and neither is while it is on `0.2.2`. The guarantee is
unchanged; there is simply more than one tag that can satisfy it.

**So you bump the workspace version when you start working towards a release,
not when you cut it.** Every preview on the way to 0.3.0 is a prerelease of
0.3.0, and the eventual `v0.3.0` matches with no second bump.

Build metadata (`+something`) is rejected outright: `+` is not a legal character
in an OCI tag, so such a version could never name its own image.

### Publishing a preview is the whole of this repository's part in it

Nothing here deploys a preview, and nothing here tells anything else that one
exists. The platform's desired state names the version an environment runs, and
whatever maintains that desired state discovers a new preview by looking at the
registry.

That is deliberate. This repository holds no credential for
`saas-fabric-platform` and needs none: publishing to GHCR is the entire
contract.

The `org.opencontainers.image.revision` label matters more under that
arrangement than it looks. It is what lets anything holding a digest prove which
commit the artifact was built from, without asking this repository.

## The compiler is pinned, and the pin is checked

The builder image's `RUST_VERSION` must equal the channel in
`rust-toolchain.toml`, enforced by
[`scripts/check_toolchain_pin.py`](../../scripts/check_toolchain_pin.py) in CI.

Without that check, an image could be compiled by a different compiler than
every gate checked the workspace with — which is the failure
[the toolchain policy](toolchain-policy.md) exists to prevent, reintroduced one
layer down and much harder to see. Base images are pinned by digest as well as
tag, because a tag is a moving reference and a release should be reproducible
from its own source.

## Nothing is published from a commit whose gates did not pass

The release workflow re-runs the gates before it builds. That duplicates
`ci.yml`, and the duplication is the point: CI runs on every push, so a tagged
commit has almost certainly been checked — and "almost certainly" is doing the
work in that sentence. What it admits is an artifact built from a red tree,
deployed, behaving in a way no gate reproduces.

Releases are rare. Paying twice is the cheaper side of that trade.

### The condition has to name what it wants

A GitHub job result is one of **four** values, and that is the trap. The build
job's condition first read:

```yaml
if: always() && needs.verify.result != 'failure'
```

which reads as "only when the gates passed" and is not. A `cancelled` gate — a
killed run, a lost runner, a cancelled workflow — is not a `failure`, so that
condition **would have published from a release whose gates never finished**.
Excluding the result you fear is not the same as requiring the one you want.

The condition now requires success outright for a tag, and permits the
deliberate skip only when the ref is not one:

| ref | `verify` | builds? | publishes? |
|---|---|---|---|
| tag | success | yes | yes |
| tag | failure | no | no |
| tag | cancelled | no | no |
| tag | skipped | no | no |
| branch | skipped | yes | no |

Publishing is gated a second time inside the job, on the ref being a tag — so a
pull-request build cannot push even if the first condition were ever wrong
again.

## Building locally

```bash
docker build --target runtime-api       -t saas-fabric .
docker build --target control-plane-api -t saas-fabric-control-plane .
docker build -f apps/control-plane-ui/Dockerfile --target console \
                                        -t saas-fabric-control-plane-ui .
```

All three build from the repository root: the console's build needs only its
own directory, but a release builds all three from one context and a second
context would be a second thing to get wrong.

Note that a local build produces your machine's architecture. LucentRoot's node
is `amd64`, and the release builds there — so an image built on an Apple
machine is for testing the packaging, not for running on the cluster.
