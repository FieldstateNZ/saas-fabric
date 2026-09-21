namespace Aspire.Hosting;

internal static class FabricTemplates
{
    public static string Stage(string directory)
    {
        var assembly = typeof(FabricTemplates).Assembly;
        var target = Path.Combine(directory, ".fabric", "templates");
        Directory.CreateDirectory(target);
        const string prefix = "Aspire.Hosting.Templates.";
        foreach (var resource in assembly.GetManifestResourceNames().Where(n => n.StartsWith(prefix, StringComparison.Ordinal)))
        {
            using var input = assembly.GetManifestResourceStream(resource)!;
            var path = Path.Combine(target, resource[prefix.Length..]);
            Directory.CreateDirectory(Path.GetDirectoryName(path)!);
            using var output = File.Create(path);
            input.CopyTo(output);
        }
        return target;
    }
}
