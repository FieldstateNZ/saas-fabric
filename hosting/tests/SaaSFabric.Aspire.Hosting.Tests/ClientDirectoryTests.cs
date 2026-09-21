using Aspire.Hosting;
using Xunit;

public sealed class ClientDirectoryTests : IDisposable
{
    private readonly string directory = Path.Combine(Path.GetTempPath(), $"fabric-config-test-{Guid.NewGuid():N}");
    private const string Valid = """
        id: demo
        name: Demo
        realm: demo-realm
        applications:
          - id: shell
            redirectUris: ['http://127.0.0.1:5186/callback']
        """;

    public ClientDirectoryTests() => Directory.CreateDirectory(directory);
    public void Dispose() => Directory.Delete(directory, recursive: true);

    [Fact]
    public void LoadsClientWithoutInventingCredentials()
    {
        File.WriteAllText(Path.Combine(directory, "demo.yaml"), Valid);
        var client = Assert.Single(ClientDirectory.Load(directory));
        Assert.Equal("demo-realm", client.Realm);
        Assert.Single(client.Applications);
        Assert.Empty(client.Roles);
    }

    [Theory]
    [InlineData("realm: demo-realm", "realm: master")]
    [InlineData("id: demo", "id: ../demo")]
    [InlineData("id: shell", "id: admin-cli")]
    [InlineData("realm: demo-realm", "realm: demo-realm\nrealm: duplicate")]
    [InlineData("name: Demo", "name: Demo\npassword: should-not-be-here")]
    [InlineData("http://127.0.0.1:5186/callback", "http://remote.example/callback")]
    [InlineData("http://127.0.0.1:5186/callback", "https://example.test/*")]
    [InlineData("http://127.0.0.1:5186/callback", "https://example.test/#callback")]
    public void RefusesInvalidDeclarationBeforeCreatingResources(string before, string after)
    {
        File.WriteAllText(Path.Combine(directory, "demo.yaml"), Valid.Replace(before, after));
        Assert.Throws<ArgumentException>(() => ClientDirectory.Load(directory));
    }

    [Fact]
    public void RefusesDuplicateRealmAcrossClients()
    {
        File.WriteAllText(Path.Combine(directory, "one.yaml"), Valid);
        File.WriteAllText(Path.Combine(directory, "two.yaml"), Valid.Replace("id: demo", "id: other"));
        Assert.Throws<ArgumentException>(() => ClientDirectory.Load(directory));
    }

    [Theory]
    [InlineData("registry.example/brand:latest")]
    [InlineData("registry.example/brand@sha256:short")]
    [InlineData("https://user:secret@registry.example/brand")]
    public void RefusesMutableOrCredentialBearingBrands(string artifact)
    {
        File.WriteAllText(Path.Combine(directory, "demo.yaml"), Valid + $"\nbrand:\n  artifact: {artifact}\n");
        Assert.Throws<ArgumentException>(() => ClientDirectory.Load(directory));
    }

    [Fact]
    public void LoadsPinnedBrandWithTlsByDefault()
    {
        var artifact = "registry.example/brand@sha256:" + new string('a', 64);
        File.WriteAllText(Path.Combine(directory, "demo.yaml"), Valid + $"\nbrand:\n  artifact: {artifact}\n");
        var client = Assert.Single(ClientDirectory.Load(directory));
        Assert.Equal(artifact, client.Brand!.Artifact);
        Assert.False(client.Brand.PlainHttp);
    }

    [Fact]
    public void TemplateRequiresBrandAndImmutableReference()
    {
        var template = "\nloginTemplate:\n  artifact: registry.example/template@sha256:" + new string('b', 64) + "\n";
        var file = Path.Combine(directory, "demo.yaml");
        File.WriteAllText(file, Valid + template);
        Assert.Throws<ArgumentException>(() => ClientDirectory.Load(directory));
        var branded = Valid + "\nbrand:\n  artifact: registry.example/brand@sha256:" + new string('a', 64) + "\n";
        File.WriteAllText(file, branded + template);
        Assert.NotNull(Assert.Single(ClientDirectory.Load(directory)).LoginTemplate);
        File.WriteAllText(file, branded + "\nloginTemplate:\n  artifact: registry.example/template:latest\n");
        Assert.Throws<ArgumentException>(() => ClientDirectory.Load(directory));
    }
}
