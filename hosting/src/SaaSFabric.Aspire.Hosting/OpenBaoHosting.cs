using Aspire.Hosting.ApplicationModel;

namespace Aspire.Hosting;

internal static class OpenBaoHosting
{
    public static (IResourceBuilder<ContainerResource> Server, IResourceBuilder<ContainerResource> Initializer)
        Add(IDistributedApplicationBuilder builder, string templates, string prefix)
    {
        var server = builder.AddContainer("fabric-openbao", "openbao/openbao", "2.4.1")
            .WithBindMount(Path.Combine(templates, "openbao.hcl"), "/openbao/config/fabric.hcl", isReadOnly: true)
            .WithVolume($"{prefix}-bao-data", "/openbao/file")
            .WithArgs("server", "-config=/openbao/config/fabric.hcl")
            .WithHttpEndpoint(targetPort: 8200, name: "http")
            .WithHttpHealthCheck("/v1/sys/health?uninitcode=200&sealedcode=200", 200, "http");
        var initializer = builder.AddContainer("fabric-openbao-init", "openbao/openbao", "2.4.1")
            .WithEntrypoint("/bin/sh")
            .WithArgs("/init.sh")
            .WithBindMount(Path.Combine(templates, "openbao-init.sh"), "/init.sh", isReadOnly: true)
            .WithVolume($"{prefix}-bao-keys", "/keys")
            .WithEnvironment("BAO_ADDR", server.GetEndpoint("http"))
            .WaitFor(server);
        return (server, initializer);
    }
}
