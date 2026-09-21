using System.Text.RegularExpressions;
using YamlDotNet.Serialization;
using YamlDotNet.Serialization.NamingConventions;

namespace Aspire.Hosting;

/// <summary>Loads a small, strict client configuration before any resources are created.</summary>
public static class ClientDirectory
{
    private static readonly HashSet<string> ReservedApplications =
        ["account", "account-console", "admin-cli", "broker", "realm-management", "security-admin-console"];

    public static IReadOnlyList<FabricClient> Load(string path)
    {
        var parser = new DeserializerBuilder().WithNamingConvention(CamelCaseNamingConvention.Instance)
            .WithDuplicateKeyChecking().Build();
        var clients = new List<FabricClient>();
        foreach (var file in Directory.GetFiles(path, "*.yaml").Order(StringComparer.Ordinal))
        {
            try
            {
                var client = parser.Deserialize<FabricClient>(File.ReadAllText(file));
                Validate(client);
                clients.Add(client);
            }
            catch (Exception error) when (error is YamlDotNet.Core.YamlException or ArgumentException)
            {
                throw new ArgumentException($"Invalid client file {Path.GetFileName(file)}: {error.Message}", error);
            }
        }
        if (clients.Count == 0) throw new ArgumentException("Client directory must contain at least one .yaml file.");
        if (clients.Select(c => c.Id).Distinct().Count() != clients.Count ||
            clients.Select(c => c.Realm).Distinct().Count() != clients.Count)
            throw new ArgumentException("Client IDs and realm names must be unique across the directory.");
        return clients.AsReadOnly();
    }

    public static void Validate(FabricClient? client)
    {
        ArgumentNullException.ThrowIfNull(client);
        if (!SafeId(client.Id) || !SafeId(client.Realm) || client.Realm == "master")
            throw new ArgumentException("Client id and realm must be lowercase IDs; master is reserved.");
        if (string.IsNullOrWhiteSpace(client.Name) || client.Name.Length > 120)
            throw new ArgumentException("Client name must contain 1–120 characters.");
        if (client.Brand is not null && !Regex.IsMatch(client.Brand.Artifact,
            "^[a-z0-9][a-z0-9.:-]*/[a-z0-9/_-]+@sha256:[a-f0-9]{64}$"))
            throw new ArgumentException("Brand artifact must be an OCI repository pinned by sha256 digest.");
        if (client.LoginTemplate is not null && (client.Brand is null || !Regex.IsMatch(client.LoginTemplate.Artifact,
            "^[a-z0-9][a-z0-9.:-]*/[a-z0-9/_-]+@sha256:[a-f0-9]{64}$")))
            throw new ArgumentException("Login template requires a brand and a digest-pinned OCI reference.");
        if (client.Applications is null || client.Applications.Count == 0)
            throw new ArgumentException("A client needs at least one application.");
        if (client.Roles is null || client.Roles.Any(r => string.IsNullOrWhiteSpace(r) || r.Length > 120))
            throw new ArgumentException("Role names must contain 1–120 characters.");
        if (client.Applications.Any(a => a is null || !SafeId(a.Id) || ReservedApplications.Contains(a.Id)) ||
            client.Applications.Select(a => a.Id).Distinct().Count() != client.Applications.Count)
            throw new ArgumentException("Application IDs must be valid, non-reserved and unique within a client.");
        foreach (var app in client.Applications)
        {
            if (app.RedirectUris is null || app.RedirectUris.Count == 0 || app.RedirectUris.Any(uri => !ValidRedirect(uri)))
                throw new ArgumentException($"{app.Id}: use explicit HTTPS or loopback HTTP redirect URIs without wildcards, fragments or credentials.");
        }
    }

    private static bool SafeId(string? id) => id is not null && Regex.IsMatch(id, "^[a-z][a-z0-9-]{0,47}$");

    private static bool ValidRedirect(string value) => Uri.TryCreate(value, UriKind.Absolute, out var uri)
        && !value.Contains('*') && uri.Fragment.Length == 0 && uri.UserInfo.Length == 0
        && (uri.Scheme == "https" || (uri.Scheme == "http" && uri.IsLoopback));
}
