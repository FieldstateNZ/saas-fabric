using System.Security.Cryptography;
using System.Text;
using Aspire.Hosting.ApplicationModel;

namespace Aspire.Hosting;

/// <summary>Creates a reusable SaaS Fabric AppHost builder.</summary>
public static class SaaSFabricApplication
{
    public static SaaSFabricApplicationBuilder CreateBuilder(string[] args) => new(args);
}

/// <summary>Local identity gateway, OpenBao and per-client OpenTofu provisioning, configured together.</summary>
public sealed class SaaSFabricApplicationBuilder(string[] args) : DistributedApplicationBuilder(args)
{
    public IResourceBuilder<KeycloakResource>? Keycloak { get; private set; }
    public IResourceBuilder<ContainerResource>? OpenBao { get; private set; }
    public IResourceBuilder<ContainerResource>? Envoy { get; private set; }
    public IReadOnlyList<IResourceBuilder<ContainerResource>> ClientApplies { get; private set; } = [];

    /// <summary>Adds the local services and automatic applies from AppHost-relative YAML files.</summary>
    public SaaSFabricApplicationBuilder AddSaaSFabric(string clientDirectory, string audience = "saas-fabric")
    {
        if (ExecutionContext.IsPublishMode) throw new InvalidOperationException("This hosting integration is a local development demo only.");
        if (Keycloak is not null) throw new InvalidOperationException("SaaS Fabric has already been added.");
        if (string.IsNullOrWhiteSpace(audience)) throw new ArgumentException("An audience is required.", nameof(audience));
        var clients = ClientDirectory.Load(Path.GetFullPath(clientDirectory, AppHostDirectory));
        var templates = FabricTemplates.Stage(AppHostDirectory);
        // Stable within this worktree; distinct from primary checkout and other AppHosts.
        var suffix = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(AppHostDirectory)))[..12].ToLowerInvariant();
        var prefix = $"saas-fabric-{suffix}";
        var username = this.AddParameter("fabric-admin", "admin");
        Keycloak = this.AddKeycloak("fabric-keycloak", adminUsername: username)
            .WithDataVolume($"{prefix}-keycloak")
            .WithVolume($"{prefix}-brands", "/opt/keycloak/themes", isReadOnly: true);
        var password = this.CreateResourceBuilder(Keycloak.Resource.AdminPasswordParameter
            ?? throw new InvalidOperationException("Keycloak requires a generated admin password."));
        var bao = OpenBaoHosting.Add(this, templates, prefix);
        OpenBao = bao.Server;
        ClientApplies = OpenTofuHosting.Add(this, clients, templates, prefix, audience,
            Keycloak, username, password, bao.Server, bao.Initializer);
        Envoy = EnvoyHosting.Add(this, templates, clients, Keycloak, ClientApplies);
        var assets = this.AddContainer("fabric-brand-assets", "nginx", "1.29.6-alpine")
            .WithVolume($"{prefix}-brands", "/brands", isReadOnly: true)
            .WithBindMount(Path.Combine(templates, "brand-nginx.conf"), "/etc/nginx/conf.d/default.conf", isReadOnly: true)
            .WithContainerNetworkAlias("fabric-brand-assets")
            .WithHttpEndpoint(targetPort: 80, name: "http")
            .WithHttpHealthCheck("/healthz", 200, "http");
        foreach (var apply in ClientApplies) assets.WaitForCompletion(apply);
        Envoy.WaitFor(assets);
        return this;
    }
}
