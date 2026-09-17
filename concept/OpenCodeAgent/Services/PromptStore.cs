using System.IO;
using System.Text.Json;

namespace OpenCodeAgent.Services;

/// <summary>Client-side saved prompts, one JSON list beside ui.json.</summary>
public sealed class PromptStore
{
    public List<string> Prompts { get; set; } = [];

    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "opencode-agent",
        "prompts.json");

    public static PromptStore Load()
    {
        try
        {
            if (File.Exists(FilePath))
                return JsonSerializer.Deserialize<PromptStore>(File.ReadAllText(FilePath)) ?? new PromptStore();
        }
        catch
        {
            // unreadable prompts must never block the app from starting
        }
        return new PromptStore();
    }

    public void Save()
    {
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
            File.WriteAllText(FilePath, JsonSerializer.Serialize(this, new JsonSerializerOptions { WriteIndented = true }));
        }
        catch
        {
            // a failed prompt write is not worth interrupting the chat
        }
    }
}
