# Client brand deployment

This reusable OpenTofu module owns OCI retrieval, integrity/schema validation and
installation of the client brand. It has no Aspire or .NET dependency. The caller
uses `theme_name` as the realm's `login_theme`; that output depends on a successful
installation, so a failed pull cannot switch the realm to a missing theme.

```hcl
module "brand" {
  source      = "./modules/client-brand"
  client_id   = "demo"
  artifact    = var.brand_artifact # registry/repository@sha256:<64 hex digits>
  destination = "/brands"
}
# On the existing keycloak_realm resource:
# login_theme = module.brand.theme_name
```

This first deployment adapter targets shared persistent filesystems. The OpenTofu
runner needs Python 3, ORAS 1.3.4, registry access and a writable `destination`.
Keycloak mounts that directory as its themes directory, read-only. A static
server mounts only `destination/public` as its web root; expose it at `/brands/`.
The demo supplies those mounts and services with Aspire. A production deployment
must supply equivalent storage and services using its deployment provider; this
module does not claim to provision Kubernetes or cloud infrastructure.

The OCI artifact type is `application/vnd.saas-fabric.brand.v1`. It contains exactly
`brand.json` (`application/json`) and `logo.png` (`image/png`). Each layer is limited
to 2 MiB and checked against its OCI descriptor digest and size. PNG dimensions
are limited to 2048×2048. The manifest has exactly these fields:

```json
{"schemaVersion":1,"name":"Example","primaryColor":"#3156D3","backgroundColor":"#EDF2FF","foregroundColor":"#172452"}
```

Brand artifacts supply data only. Optional, separately approved login-template
artifacts can supply FreeMarker, CSS and message bundles (see below). Platform CSS extends
Keycloak's built-in `keycloak` login theme (tested against 26.6). Fonts and page
structure come from platform defaults or an approved template package. A theme name includes client identity, artifact
reference and renderer revision. New versions are installed before the realm
switches; previous versions remain available for rollback. Reusing a tag is not
supported. Registry TLS is the default; `plain_http=true` is explicit demo-only
configuration. Authenticate ORAS using the runner's registry credentials, never
client YAML or OpenTofu variables/state.

The module uses `terraform_data` with a local-exec provisioner because artifact
materialization has no provider in this stack. Changes to the digest, renderer,
destination or transport trigger installation. It is a filesystem adapter, not a
background synchronizer: losing or changing installed files must be detected by
running `materialize.py --verify` with the same BRAND_ARTIFACT, BRAND_DESTINATION
and BRAND_THEME values. The demo does this on every apply, before reporting a
no-change plan. Repair drift with `tofu apply -replace=module.brand.terraform_data.brand`
(adjust for module count/index in the caller). Do not treat a state-only plan as
proof that external files exist. Destroy forgets installation state and retains
assets to avoid disrupting active sessions; garbage collection is an explicit
operator task after old realm references are gone.

Outputs: `theme_name` and `asset_path` (versioned public brand manifest URL path).
The public manifest includes the relative `logo.png` path, allowing other clients
such as the React shell to consume the same assets. No browser login credentials
or tokens are included in branding assets.

## Separate login templates

Pass `login_template = { artifact = "registry/repository@sha256:..." }` to select
an OCI template independently of the brand. `approved_login_templates` must
contain the exact reference, supplied by platform deployment policy rather than
client YAML. `keycloak_version` declares the target major.minor compatibility line.
The package metadata must match it. Both references and all renderer files affect
the composed immutable theme name. Install verification also checks the selected
template and its installed file hashes.

Templates may override individual login FreeMarker files, CSS and locale message
bundles. The artifact contract, publishing example and review/test guidance are in
`examples/login-templates/README.md`. Template files never enter the public asset
root. This is trusted server-side code, not a sandbox or a signature verification
system; only reviewed exact references belong in the platform's approval list.
An omitted template preserves the standard inherited layout.
