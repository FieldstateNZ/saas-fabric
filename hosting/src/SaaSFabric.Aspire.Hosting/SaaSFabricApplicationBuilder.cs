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
        Envoy = SaaSFabricHostingExtensions.AddSaaSFabric(this, clientDirectory, audience);
        Keycloak = this.CreateResourceBuilder(Resources.OfType<KeycloakResource>().Single(r => r.Name == "fabric-keycloak"));
        OpenBao = this.CreateResourceBuilder(Resources.OfType<ContainerResource>().Single(r => r.Name == "fabric-openbao"));
        ClientApplies = Resources.OfType<ContainerResource>().Where(r => r.Name.StartsWith("fabric-tofu-", StringComparison.Ordinal))
            .Select(this.CreateResourceBuilder).ToArray();
        return this;
    }
}
