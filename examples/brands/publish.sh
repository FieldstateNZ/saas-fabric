#!/bin/sh
# Independent artifact publishing; no Aspire dependency. Requires ORAS 1.3.4.
set -eu
registry="${1:?Usage: publish.sh REGISTRY [--plain-http]}"
transport="${2:-}"
case "$transport" in ''|--plain-http) ;; *) exit 2 ;; esac
root="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
for brand in demo contrast; do
  (cd "$root/$brand" && oras push ${transport:+"$transport"} \
    --artifact-type application/vnd.saas-fabric.brand.v1 \
    --annotation org.opencontainers.image.created=2026-09-21T00:00:00Z \
    "$registry/brands/$brand:v1" brand.json:application/json logo.png:image/png)
done
