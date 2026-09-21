using System.Text.Json;
using Aspire.Hosting.ApplicationModel;

namespace Aspire.Hosting;

internal static class OpenTofuHosting
{
    public static IReadOnlyList<IResourceBuilder<ContainerResource>> Add(IDistributedApplicationBuilder builder,
        IReadOnlyList<FabricClient> clients, string templates, string prefix, string audience,
        IResourceBuilder<KeycloakResource> keycloak, IResourceBuilder<ParameterResource> username,
        IResourceBuilder<ParameterResource> password, IResourceBuilder<ContainerResource> bao,
        IResourceBuilder<ContainerResource> initializer)
    {
        var applies = new List<IResourceBuilder<ContainerResource>>();
        foreach (var client in clients)
        {
            var applications = client.Applications.ToDictionary(a => a.Id, a => new {
                redirect_uris = a.RedirectUris,
                web_origins = a.RedirectUris.Select(u => new Uri(u).GetLeftPart(UriPartial.Authority)).Distinct(),
            });
            var apply = builder.AddContainer($"fabric-tofu-{client.Id}", "saas-fabric-tofu")
                .WithDockerfile(templates)
                .WithVolume($"{prefix}-tofu-{client.Id}", "/state")
                .WithVolume($"{prefix}-bao-keys", "/keys", isReadOnly: true)
                .WithVolume($"{prefix}-brands", "/brands")
                .WithEnvironment("KEYCLOAK_URL", keycloak.GetEndpoint("http"))
                .WithEnvironment("KEYCLOAK_CLIENT_ID", "admin-cli")
                .WithEnvironment("KEYCLOAK_USER", username)
                .WithEnvironment("KEYCLOAK_PASSWORD", password)
                .WithEnvironment("VAULT_ADDR", bao.GetEndpoint("http"))
                .WithEnvironment("TF_VAR_client_id", client.Id)
                .WithEnvironment("TF_VAR_realm", client.Realm)
                .WithEnvironment("TF_VAR_display_name", client.Name)
                .WithEnvironment("TF_VAR_audience", audience)
                .WithEnvironment("TF_VAR_applications", JsonSerializer.Serialize(applications))
                .WithEnvironment("TF_VAR_roles", JsonSerializer.Serialize(client.Roles))
                .WithEnvironment("TF_VAR_brand", JsonSerializer.Serialize(client.Brand is null ? null : new {
                    artifact = client.Brand.Artifact, plain_http = client.Brand.PlainHttp,
                }))
                .WithEnvironment("TF_VAR_login_template", JsonSerializer.Serialize(client.LoginTemplate is null ? null : new {
                    artifact = client.LoginTemplate.Artifact, plain_http = client.LoginTemplate.PlainHttp,
                }))
                .WaitFor(keycloak).WaitForCompletion(initializer);
            if (applies.Count > 0) apply.WaitForCompletion(applies[^1]);
            applies.Add(apply);
        }
        return applies.AsReadOnly();
    }
}
