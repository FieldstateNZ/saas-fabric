using Aspire.Hosting;
using Aspire.Hosting.ApplicationModel;
using Xunit;

public sealed class BuilderTests : IDisposable
{
    private readonly string directory = Path.Combine(Path.GetTempPath(), $"fabric-builder-test-{Guid.NewGuid():N}");
    public BuilderTests() => Directory.CreateDirectory(directory);
    public void Dispose() => Directory.Delete(directory, recursive: true);

    [Fact]
    public void StandardBuilderSupportsTheExportedIntegration()
    {
        File.WriteAllText(Path.Combine(directory, "client.yaml"), "id: client\nname: Client\nrealm: client\napplications: [{ id: shell, redirectUris: ['http://localhost:6002/callback'] }]\n");
        var builder = DistributedApplication.CreateBuilder([]);
        File.WriteAllText(Path.Combine(directory, "policy.json"), "[]");
        var gateway = builder.AddSaaSFabric(directory, templatePolicy: Path.Combine(directory, "policy.json"));
        Assert.Equal("[]", File.ReadAllText(Path.Combine(builder.AppHostDirectory, ".fabric/templates/template-policy.json")));
        Assert.Equal("fabric-envoy", gateway.Resource.Name);
        Assert.Contains(builder.Resources, resource => resource.Name == "fabric-tofu-client");
        Assert.Throws<InvalidOperationException>(() => builder.AddSaaSFabric(directory));
    }

    [Fact]
    public void DemoUsesSeparateResourcesAndAnApplyForEachClient()
    {
        foreach (var id in new[] { "one", "two" })
            File.WriteAllText(Path.Combine(directory, $"{id}.yaml"), $$"""
                id: {{id}}
                name: {{id}}
                realm: realm-{{id}}
                applications:
                  - id: shell
                    redirectUris: ['http://localhost:5186/callback']
                """);
        var builder = SaaSFabricApplication.CreateBuilder(["--appHostDirectory", directory]);
        builder.AddSaaSFabric(directory);
        Assert.NotNull(builder.Keycloak);
        Assert.NotNull(builder.OpenBao);
        Assert.NotNull(builder.Envoy);
        Assert.Single(builder.Envoy.Resource.Annotations.OfType<EndpointAnnotation>());
        var yaml = new YamlDotNet.RepresentationModel.YamlStream();
        yaml.Load(new StringReader(File.ReadAllText(Path.Combine(builder.AppHostDirectory, ".fabric/templates/envoy.yaml"))));
        var root = (YamlDotNet.RepresentationModel.YamlMappingNode)yaml.Documents[0].RootNode;
        var routes = (YamlDotNet.RepresentationModel.YamlSequenceNode)root["static_resources"]["listeners"][0]
            ["filter_chains"][0]["filters"][0]["typed_config"]["route_config"]["virtual_hosts"][0]["routes"];
        Assert.Equal("/realms/realm-one/", routes[0]["match"]["prefix"].ToString());
        Assert.Equal("/realms/realm-two/", routes[1]["match"]["prefix"].ToString());
        Assert.Equal("404", routes.Children.Last()["direct_response"]["status"].ToString());
        Assert.Equal("127.0.0.1", root["admin"]["address"]["socket_address"]["address"].ToString());
        Assert.Equal(new[] { "fabric-tofu-one", "fabric-tofu-two" }, builder.ClientApplies.Select(r => r.Resource.Name));
        Assert.Contains(builder.Resources, r => r.Name == "fabric-openbao-init");
        Assert.All(builder.ClientApplies, apply => Assert.Contains(apply.Resource.Annotations,
            annotation => annotation is ContainerMountAnnotation mount && mount.Target == "/state"));
        Assert.Throws<InvalidOperationException>(() => builder.AddSaaSFabric(directory));
    }
}
