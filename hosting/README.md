# SaaS Fabric Aspire hosting demo

A separate C# hosting library and a small consuming AppHost. The structure follows
the NGCM platform reference: application builder, embedded provisioning templates,
client configuration files, and an OpenTofu apply with separate state per client.
The local identity/secrets slice includes an Envoy identity gateway.

## Run

Requires .NET 10 SDK, Aspire CLI 13.5.4 and a running Docker-compatible runtime.
The AppHost SDK and hosting packages are pinned to 13.5.4; the matching
Keycloak integration is 13.5.4-preview.1.26464.4. Independent provider and
container versions retain their own pins.
The demo intentionally uses NuGet-locked dashboard/orchestration packages instead
of the CLI bundle, keeping those runtime components at exactly 13.5.4 too.
OpenTofu runs in an image built by Aspire; no host `tofu` installation is needed.

From the repository root:

```sh
aspire start --isolated --apphost hosting/samples/SaaSFabric.Demo.AppHost/SaaSFabric.Demo.AppHost.csproj
aspire describe --apphost hosting/samples/SaaSFabric.Demo.AppHost/SaaSFabric.Demo.AppHost.csproj
aspire logs fabric-tofu-demo --apphost hosting/samples/SaaSFabric.Demo.AppHost/SaaSFabric.Demo.AppHost.csproj
```

The dashboard reports the allocated local service URLs. Wait for
both `fabric-tofu-demo` and `fabric-tofu-contrast` to **exit with code 0**: a running container is still applying,
not a completion signal. Each last message confirms the read-back checks and a
second plan with no changes. A nonzero exit is a provisioning/verification failure.
Run the same `aspire start --isolated` command to reconcile after editing config.
Stop only this stack with:

```sh
aspire stop --apphost hosting/samples/SaaSFabric.Demo.AppHost/SaaSFabric.Demo.AppHost.csproj
```

The independently running React preview on port 5186 is not managed or modified
by this AppHost. Its callback URL is registered as an example; this change does
not implement browser sign-in or connect the Rust control plane.

## Library use

Reference `src/SaaSFabric.Aspire.Hosting/SaaSFabric.Aspire.Hosting.csproj` from
an AppHost with `IsAspireProjectResource="false"` (this is a library, not a launched
application). Its HCL, Envoy configuration and scripts travel as embedded resources.

```cs
using Aspire.Hosting;

var builder = SaaSFabricApplication.CreateBuilder(args);
builder.AddSaaSFabric("config/clients", audience: "saas-fabric");
// Additional local workloads can use builder.Keycloak / builder.OpenBao / builder.Envoy,
// and WaitForCompletion on builder.ClientApplies before accessing provisioned data.
builder.Build().Run();
```

Configuration path is relative to the consuming AppHost. Each `.yaml` file
contains one client; see `samples/SaaSFabric.Demo.AppHost/config/clients/demo.yaml`.
IDs/realms must be unique, application IDs unique per client. Keycloak built-in
application IDs are reserved. Redirects must be
explicit HTTPS or loopback HTTP URLs. Unknown YAML fields and duplicate keys
fail before services are added. Do not put credentials in these files.

For each client, OpenTofu creates:

- A Keycloak realm and its declared public OIDC applications, with authorization
  code flow, S256 PKCE, explicit callbacks/origins and the Fabric audience mapper.
- The two required SaaS Fabric realm roles plus optional configured roles.
- An OpenBao namespace named for the client, a `secret` KV v2 mount inside it,
  and a `client` policy limited to that mount.

No users or application secrets are invented. Workload authentication to OpenBao
is outside this demo: the policy is created but no workload is given a root token.
The root token is used only by the local provisioner. Do not let the Rust
reconciler and this demo manage the same realms concurrently.

## Local Envoy gateway

`AddSaaSFabric` automatically adds `fabric-envoy` and exposes `builder.Envoy`.
Its HTTP URL is allocated by Aspire, so isolated worktrees do not share ports.
For each declared realm, `/realms/<realm>/` forwards unchanged to Keycloak;
`/resources/` serves Keycloak's login assets. For the demo, request
`/realms/fabric-demo/.well-known/openid-configuration`, then its `jwks_uri`.
The original Host header is retained so discovery uses the gateway origin.
The Aspire health check requests discovery through Envoy, testing the upstream
as well as the listener. Envoy starts after all client applies succeed.

`/healthz` forwards only to Envoy's own `/ready`. Its admin listener binds to
container loopback, allows only `/ready`, and has no published endpoint.
`/brands/` serves only the public brand assets installed by OpenTofu.
Other paths (including `/admin/`, `/config_dump`, and undeclared realms) return
404. OpenBao is not routed. The internal Keycloak HTTP listener on 8080 is
explicitly enabled and reached by an alias on Aspire's isolated container network.

This is a local HTTP identity gateway, not the production trusted-ingress JWT
boundary or the NGCM OAuth/BFF implementation. It does not authenticate upstream
application requests, proxy the React preview, or change registered callbacks.
TLS termination, sessions, authorization and product API routing are outside
this demo. The packaged template is rendered afresh from validated client YAML.

## OCI client branding (OpenTofu-owned)

The reusable module lives at `infrastructure/opentofu/modules/client-brand`,
outside this library. Aspire packages an unchanged copy into the local runner;
it contains no OCI pulling, theme rendering or brand selection logic in C#.
The existing realm resource sets `login_theme` from the module's dependent output.

Client YAML optionally includes `brand.artifact` (an OCI reference pinned by
SHA-256 digest) and `brand.plainHttp` (default false). The two demo clients select
blue and copper artifacts from the local registry. See `examples/brands/README.md`
for initial publishing on a fresh machine; first applies fail until the artifacts exist.
The sample AppHost supplies a persistent registry; the library supplies empty
shared brand storage and a static asset server. Neither publishes brand bundles.

On apply, OpenTofu retrieves and validates the manifest and logo, installs a
versioned platform-owned Keycloak theme, and updates the realm only after that
succeeds. It exports `brand_theme` and `brand_asset_path`. Each apply verifies
installed file hashes and reads back the realm theme before its no-change plan.
A failed artifact cannot replace the selected realm theme. Storage drift fails
verification; the module README documents explicit repair and retained-version
cleanup. Mounts are read-only for Keycloak and the static server.

The public manifest and PNG are reusable by the React shell; automatic shell
brand selection is not wired into the existing independent preview. No frontend
login flow or demo users were added. The copper client additionally selects an
approved OCI login template, while blue retains the default layout. See
`examples/login-templates/README.md` for the separate artifact contract and
platform-owned approval policy. Login/reset-password pages inherit the
Keycloak theme. Account/admin/email customization is not included.

## Local state and lifecycle

Keycloak data, OpenBao file storage, OpenBao bootstrap keys and each client's
OpenTofu state have separate persistent Docker volumes. Names include a hash of
the AppHost's absolute path, so another worktree has different volumes. Aspire's
isolated mode keeps endpoints/secrets separate from other running AppHosts.
The generated `.fabric/templates` directory is ignored and recreated from the
library; edit the embedded source templates, not generated files.

The initializer uses a single unseal share, stored with the initial root token
in a private local Docker volume. It does not log them. Plain HTTP for OpenBao, development
Keycloak, file storage and automatic unseal are explicitly local-only;
`aspire publish` is rejected. This is not a production secret-management setup.
Retain the keys and data together; missing keys or mismatched data fail closed.
State/keys are local sensitive data and are not committed.

Removing a YAML file does not destroy that client's resources. Realm and namespace
destruction/replacement is prevented by HCL. Renaming an ID or realm is not a
migration. Resetting requires deliberately removing this AppHost's own volumes,
including the matching Keycloak credentials in its isolated secret store. Do not
use global Docker prune or remove another stack's volumes.

Providers are pinned to Keycloak 5.9.0 and Vault 5.11.0 (Vault API compatibility
against OpenBao); OpenTofu is pinned to 1.12.0. The provider lock file created by
`init` persists in each client's state volume. Keycloak uses the pinned Aspire
integration's image default (26.6); OpenBao is 2.4.1 and Envoy is v1.39.1.
This demo uses no remote environments or deployment pipeline.

## Checks

```sh
dotnet build hosting/samples/SaaSFabric.Demo.AppHost/SaaSFabric.Demo.AppHost.csproj
dotnet test hosting/tests/SaaSFabric.Aspire.Hosting.Tests/SaaSFabric.Aspire.Hosting.Tests.csproj
```

Each apply runs `tofu validate`, `apply`, a second `plan -detailed-exitcode`, and
API read-back checks for public PKCE clients, the namespace KV v2 mount and policy.
Restarting the AppHost tests persisted reconciliation and automatic unseal.

References: [Aspire Keycloak](https://aspire.dev/integrations/security/keycloak/),
[OpenTofu container build guidance](https://opentofu.org/docs/intro/install/docker/),
[Keycloak provider](https://registry.terraform.io/providers/keycloak/keycloak/latest/docs).

Envoy references: [official release](https://github.com/envoyproxy/envoy/releases/tag/v1.39.1),
[configuration and validation](https://www.envoyproxy.io/docs/envoy/latest/start/quick-start/run-envoy),
[admin access restrictions](https://www.envoyproxy.io/docs/envoy/latest/start/quick-start/admin).

## GitHub package releases

The `Hosting package` workflow tests and packs on pull requests. A
`hosting-v0.1.0-preview.2`-style tag on a commit already merged into `main`
publishes the exact tested NuGet artifact to the FieldstateNZ GitHub Packages
feed. Manual workflow runs validate packaging without publishing. Hosting tags
are separate from the Rust/container `v*` releases. Use preview versions while
the Keycloak dependency is a preview. Never move an existing release tag.

After the first publication, a package administrator must set package visibility
to **Public** in GitHub's package settings; public repository visibility alone
does not do this. GitHub's NuGet registry still requires consumer authentication
with a classic token granting `read:packages`, even for public packages. Keep
credentials in a local credential provider or CI secrets, never source control.
See [GitHub's NuGet registry documentation](https://docs.github.com/en/packages/working-with-a-github-packages-registry/working-with-the-nuget-registry).

Configure `https://nuget.pkg.github.com/FieldstateNZ/index.json` as an authenticated
NuGet source and map only `SaaSFabric.Aspire.Hosting` to it; leave other packages
mapped to nuget.org. Then replace the project reference with:

```xml
<PackageReference Include="SaaSFabric.Aspire.Hosting" Version="0.1.0-preview.2" />
```

This package remains a local development harness. Production uses the independent
OpenTofu module and production infrastructure, not Aspire's demo bootstrap.
