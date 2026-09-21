using System.Security.Cryptography;
using System.Text;
using Aspire.Hosting.ApplicationModel;

namespace Aspire.Hosting;

/// <summary>Adds Fabric services to existing C# and TypeScript AppHosts.</summary>
public static class SaaSFabricHostingExtensions
{
    /// <summary>Adds local identity, secrets and client provisioning; returns the gateway.</summary>
    /// <param name="builder">The existing application builder.</param>
    /// <param name="clientDirectory">AppHost-relative directory containing client YAML.</param>
    /// <param name="audience">Audience for the clients' access tokens.</param>
    [AspireExport]
    public static IResourceBuilder<ContainerResource> AddSaaSFabric(
        this IDistributedApplicationBuilder builder, string clientDirectory, string audience = "saas-fabric")
    {
        if (builder.ExecutionContext.IsPublishMode) throw new InvalidOperationException("This hosting integration is a local development demo only.");
        if (builder.Resources.Any(resource => resource.Name == "fabric-keycloak")) throw new InvalidOperationException("SaaS Fabric has already been added.");
        if (string.IsNullOrWhiteSpace(audience)) throw new ArgumentException("An audience is required.", nameof(audience));
        var clients = ClientDirectory.Load(Path.GetFullPath(clientDirectory, builder.AppHostDirectory));
        var templates = FabricTemplates.Stage(builder.AppHostDirectory);
        // Stable within this worktree; distinct from primary checkout and other AppHosts.
        var suffix = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(builder.AppHostDirectory)))[..12].ToLowerInvariant();
        var prefix = $"saas-fabric-{suffix}";
        var username = builder.AddParameter("fabric-admin", "admin");
        var keycloak = builder.AddKeycloak("fabric-keycloak", adminUsername: username)
            .WithDataVolume($"{prefix}-keycloak")
            .WithVolume($"{prefix}-brands", "/opt/keycloak/themes", isReadOnly: true);
        var password = builder.CreateResourceBuilder(keycloak.Resource.AdminPasswordParameter
            ?? throw new InvalidOperationException("keycloak requires a generated admin password."));
        var bao = OpenBaoHosting.Add(builder, templates, prefix);
        var clientApplies = OpenTofuHosting.Add(builder, clients, templates, prefix, audience,
            keycloak, username, password, bao.Server, bao.Initializer);
        var envoy = EnvoyHosting.Add(builder, templates, clients, keycloak, clientApplies);
        var assets = builder.AddContainer("fabric-brand-assets", "nginx", "1.29.6-alpine")
            .WithVolume($"{prefix}-brands", "/brands", isReadOnly: true)
            .WithBindMount(Path.Combine(templates, "brand-nginx.conf"), "/etc/nginx/conf.d/default.conf", isReadOnly: true)
            .WithContainerNetworkAlias("fabric-brand-assets")
            .WithHttpEndpoint(targetPort: 80, name: "http")
            .WithHttpHealthCheck("/healthz", 200, "http");
        foreach (var apply in clientApplies) assets.WaitForCompletion(apply);
        envoy.WaitFor(assets);
        return envoy;
    }
}
