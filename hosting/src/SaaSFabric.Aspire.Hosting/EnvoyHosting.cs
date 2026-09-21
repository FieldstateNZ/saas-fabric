using Aspire.Hosting.ApplicationModel;

namespace Aspire.Hosting;

internal static class EnvoyHosting
{
    public static IResourceBuilder<ContainerResource> Add(
        IDistributedApplicationBuilder builder, string templates, IReadOnlyList<FabricClient> clients,
        IResourceBuilder<KeycloakResource> keycloak,
        IReadOnlyList<IResourceBuilder<ContainerResource>> applies)
    {
        // This alias lives on Aspire's isolated container network; no fixed host port is needed.
        keycloak.WithContainerNetworkAlias("fabric-keycloak-upstream")
            .WithEnvironment("KC_HTTP_ENABLED", "true");
        var routes = string.Join('\n', clients.Select(client =>
            new string(' ', 24) + $"- match: {{ prefix: \"/realms/{client.Realm}/\" }}\n" +
            new string(' ', 26) + "route: { cluster: keycloak, timeout: 30s }"));
        var configuration = File.ReadAllText(Path.Combine(templates, "envoy.template.yaml"))
            .Replace("# CLIENT_ROUTES", routes, StringComparison.Ordinal);
        var path = Path.Combine(templates, "envoy.yaml");
        File.WriteAllText(path, configuration);
        var gateway = builder.AddContainer("fabric-envoy", "envoyproxy/envoy", "v1.39.1")
            .WithBindMount(path, "/etc/envoy/envoy.yaml", isReadOnly: true)
            .WithArgs("-c", "/etc/envoy/envoy.yaml", "--service-cluster", "saas-fabric-edge")
            .WithHttpEndpoint(targetPort: 10000, name: "http")
            // Probe through the real route: a healthy Envoy alone does not prove Keycloak is reachable.
            .WithHttpHealthCheck($"/realms/{clients[0].Realm}/.well-known/openid-configuration", 200, "http")
            .WaitFor(keycloak);
        foreach (var apply in applies) gateway.WaitForCompletion(apply);
        return gateway;
    }
}
