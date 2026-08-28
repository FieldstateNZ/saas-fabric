# The operator console: built with node, served without it.
#
#     docker build -f apps/control-plane-ui/Dockerfile -t saas-fabric-control-plane-ui .
#
# Built from the repository root, because the build needs this directory and
# nothing above it — but the release builds all three images from one context
# and a second context would be a second thing to get wrong.

FROM node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5 AS builder

WORKDIR /build/apps/control-plane-ui

# Manifests first, so a change to the console's source does not re-resolve its
# dependencies. Unlike the Rust workspace there are only two files to name, so
# the trick is worth doing here.
COPY apps/control-plane-ui/package.json apps/control-plane-ui/package-lock.json ./

# `ci`, not `install`: it installs exactly what the lockfile pins and fails if
# the lockfile and `package.json` disagree — the same guarantee `--locked`
# gives the Rust build above.
RUN npm ci

COPY apps/control-plane-ui/ ./

# `npm run build` is `tsc -b && vite build`, so a type error fails the image
# rather than shipping a bundle that never type-checked.
RUN npm run build

# ---------------------------------------------------------------------------
# The console
# ---------------------------------------------------------------------------
#
# `nginx-unprivileged` rather than `nginx`: the stock image starts as root to
# bind port 80 and then drops privileges, and there is no reason for a static
# file server on the operator plane to begin life as root.
FROM nginxinc/nginx-unprivileged:1.29-alpine@sha256:0c79d56aee561a1d81c63f00eee5fb5fe29279560cdc55e91425133104c7fbe6 AS console

COPY apps/control-plane-ui/nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /build/apps/control-plane-ui/dist /usr/share/nginx/html

EXPOSE 8080
USER 101
