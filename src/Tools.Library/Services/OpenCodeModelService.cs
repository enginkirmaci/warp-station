using System.Text.Json;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>
/// File-backed implementation of <see cref="IOpenCodeModelService"/>. Reads the model
/// list JSON located next to the executable under <c>settings/opencode/</c>.
/// </summary>
public class OpenCodeModelService : IOpenCodeModelService
{
    private static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };

    /// <summary>
    /// Returned when the file is missing, unreadable, or empty so the selector is never
    /// blank. Mirrors the seeded <c>models.json</c> shipped with the app.
    /// </summary>
    private static readonly OpenCodeModelsConfig Fallback = new()
    {
        DefaultModel = "Model-Garden/minimax-2.5-230b-awq-garden",
        Models = new()
        {
            "Model-Garden/minimax-2.5-230b-awq-garden",
            "Model-Garden/qwen3.5-397b-a17b-awq",
            "Model-Garden/qwen3.6-35b-a3b",
        },
    };

    private readonly string _modelsFilePath;

    public OpenCodeModelService()
    {
        // User data under %USERPROFILE%\.devtools so it survives reinstalls. Seeded from
        // the shipped default on first run; the in-memory Fallback is the last resort.
        _modelsFilePath = UserPaths.GetUserDataFile("settings", "opencode", "models.json");
    }

    /// <inheritdoc/>
    public async Task<OpenCodeModelsConfig> LoadAsync()
    {
        try
        {
            // One-time seed/migration from the shipped default on first run.
            UserPaths.SeedFromDefault(_modelsFilePath, "opencode\\models.json");

            if (!File.Exists(_modelsFilePath))
                return CloneFallback();

            var json = await File.ReadAllTextAsync(_modelsFilePath);
            var config = JsonSerializer.Deserialize<OpenCodeModelsConfig>(json, ReadOptions);
            if (config is null || config.Models.Count == 0)
                return CloneFallback();

            // Ensure a non-empty default: fall back to the first model in the list.
            if (string.IsNullOrWhiteSpace(config.DefaultModel))
                config.DefaultModel = config.Models[0];

            return config;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Error loading OpenCode models");
            return CloneFallback();
        }
    }

    private static OpenCodeModelsConfig CloneFallback()
        => new()
        {
            DefaultModel = Fallback.DefaultModel,
            Models = new List<string>(Fallback.Models),
        };
}
