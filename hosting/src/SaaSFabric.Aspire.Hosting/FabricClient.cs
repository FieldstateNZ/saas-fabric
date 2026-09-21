namespace Aspire.Hosting;

/// <summary>A local client declaration. Credentials never belong in this document.</summary>
public sealed class FabricClient
{
    public string Id { get; set; } = "";
    public string Name { get; set; } = "";
    public string Realm { get; set; } = "";
    public List<string> Roles { get; set; } = [];
    public List<FabricApplication> Applications { get; set; } = [];
    public FabricBrand? Brand { get; set; }
    public FabricBrand? LoginTemplate { get; set; }
}

/// <summary>Immutable OCI brand selection, consumed by the reusable OpenTofu module.</summary>
public sealed class FabricBrand
{
    public string Artifact { get; set; } = "";
    public bool PlainHttp { get; set; }
}

/// <summary>A public OIDC application using authorization code and S256 PKCE.</summary>
public sealed class FabricApplication
{
    public string Id { get; set; } = "";
    public List<string> RedirectUris { get; set; } = [];
}
