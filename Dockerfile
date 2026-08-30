# The two Rust services, from one compile.
#
# `fabric-api` and `fabric-control-plane-api` are separate images because they
# are separate deployments on separate networks (see the control-plane
# architecture). They share a builder stage because they share a workspace:
# compiling it twice would double the slowest part of the release for no
# benefit, and BuildKit reuses the stage across both `--target` builds.
#
#     docker build --target runtime-api      -t saas-fabric .
#     docker build --target control-plane-api -t saas-fabric-control-plane .

# The compiler this workspace is pinned to.
#
# Kept in step with `rust-toolchain.toml` by `scripts/check_toolchain_pin.py`,
# which fails the build if the two disagree. Without that check an image could
# be compiled by a different compiler than every gate checked it with — which
# is the failure the toolchain pin exists to prevent, reintroduced one layer
# down.
ARG RUST_VERSION=1.98.0

# Pinned by digest, not only by tag. A tag is a moving reference; a release
# should be reproducible from its own source.
FROM rust:${RUST_VERSION}-slim-bookworm@sha256:1469a27c125cb5a3aebfa4f4e4665d935b02fb72cc093b2c974b3d740e43f157 AS builder

WORKDIR /build

# `pkg-config` and the TLS trust store are needed at build time; `reqwest` is
# configured with `rustls-tls-native-roots` (ADR 0005), which links no OpenSSL
# but does read the host's certificates at runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY . .

# Cache mounts rather than the copy-manifests-first trick: this is a thirteen
# crate workspace, and a dependency-only pre-build would need every manifest
# listed here and would rot the first time one moved.
#
# The binaries are copied out inside the same `RUN`, because a cache mount is
# not part of the resulting layer — anything left in `target/` disappears when
# the instruction ends.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release --locked \
        --bin fabric-api \
        --bin fabric-control-plane-api \
        --bin fabric-fga-auth-api \
    && mkdir -p /out \
    && cp target/release/fabric-api /out/ \
    && cp target/release/fabric-control-plane-api /out/ \
    && cp target/release/fabric-fga-auth-api /out/

# ---------------------------------------------------------------------------
# The runtime plane
# ---------------------------------------------------------------------------
#
# Distroless: no shell, no package manager, and `nonroot` by default. There is
# nothing in the image to exec into, which is the point — this process is on
# the product edge and is reachable by every tenant's application.
#
# The `cc` variant rather than `static` because the binaries link glibc, and it
# carries `ca-certificates`, which the connector client needs to verify TLS.
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS runtime-api

COPY --from=builder /out/fabric-api /usr/local/bin/fabric-api

# The path the runtime host defaults to, and where a mounted ConfigMap
# conventionally lands. The platform supplies the file; the image only says
# where it looks.
ENV FABRIC_CONFIG=/etc/fabric/config.toml

EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/fabric-api"]

# ---------------------------------------------------------------------------
# The control plane
# ---------------------------------------------------------------------------
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS control-plane-api

COPY --from=builder /out/fabric-control-plane-api /usr/local/bin/fabric-control-plane-api

ENV FABRIC_CP_CONFIG=/etc/fabric/control-plane.toml

# A different port from the runtime API's by convention: the two run side by
# side in development, and on entirely different networks in production.
EXPOSE 8081
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/fabric-control-plane-api"]

# ---------------------------------------------------------------------------
# The authorization front, and the service it fronts
# ---------------------------------------------------------------------------
#
# Two processes, one image, and that is the point rather than a compromise.
# The authorization service runs with no authentication of its own, which is
# safe for exactly as long as nothing outside this container can address it —
# so the two are shipped as one unit and the only published port is the
# front's (ADR 0016).
#
# `fabric-fga-auth-api` is PID 1 and owns the child. If the service exits, the
# front exits too: a shell that started both and waited would leave the common
# failure looking healthy — a container whose liveness probe passes while every
# decision answers 503.
#
# Distroless like the other two, and the upstream authorization service is
# distributed that way as well — it ships one binary at `/openfga` on a base
# with no shell, which is why lifting it into this stage costs nothing. There
# is still nothing in the image to exec into.
FROM openfga/openfga:v1.19.0 AS authorization-source

FROM gcr.io/distroless/cc-debian12:nonroot@sha256:9dac0a79194e45a7da0158a9c6da57b217585af0786db3845d1f0ec1a0dd182f AS authorization-front

COPY --from=builder /out/fabric-fga-auth-api /usr/local/bin/fabric-fga-auth-api
COPY --from=authorization-source /openfga /usr/local/bin/openfga

ENV FABRIC_FGA_CONFIG=/etc/fabric/authorization.toml

# The front's port, and the only one. The authorization service's is bound to
# 127.0.0.1 by the host process and is deliberately not declared here: an
# EXPOSE for it would document an address nothing should ever connect to.
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/usr/local/bin/fabric-fga-auth-api"]
