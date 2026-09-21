#!/bin/sh
# Publishing remains independent of the deployment/demo harness.
set -eu
registry="${1:?Usage: publish.sh REGISTRY [--plain-http]}"
transport="${2:-}"
case "$transport" in ''|--plain-http) ;; *) exit 2 ;; esac
root="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cd "$root/welcome-panel"
oras push ${transport:+"$transport"} --artifact-type application/vnd.saas-fabric.login-template.v1 \
  --annotation org.opencontainers.image.created=2026-09-21T00:00:00Z \
  "$registry/login-templates/welcome-panel:v1" \
  template.json:application/json login/footer.ftl:text/plain \
  login/resources/css/template.css:text/css login/messages/messages_en.properties:text/plain
