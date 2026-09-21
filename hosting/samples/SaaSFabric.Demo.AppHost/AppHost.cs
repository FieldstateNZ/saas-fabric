using Aspire.Hosting;
using System.Security.Cryptography;
using System.Text;

var builder = SaaSFabricApplication.CreateBuilder(args);
var isolation = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(builder.AppHostDirectory)))[..12].ToLowerInvariant();
// Local infrastructure only: OCI publishing and brand deployment live outside Aspire.
var registry = builder.AddContainer("fabric-brand-registry", "registry", "3.0.0")
    .WithVolume($"saas-fabric-{isolation}-brand-registry", "/var/lib/registry")
    .WithContainerNetworkAlias("fabric-brand-registry")
    .WithHttpEndpoint(targetPort: 5000, name: "http")
    .WithHttpHealthCheck("/v2/", 200, "http");
builder.AddSaaSFabric("config/clients");
foreach (var apply in builder.ClientApplies) apply.WaitFor(registry);
builder.Build().Run();
