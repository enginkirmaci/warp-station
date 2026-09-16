using System.IO;
using System.Text.Json;

namespace OpenCodeAgent.Services;

/// <summary>
/// Client-side UI state (pins, last selection) — the opencode server keeps
/// sessions but knows nothing about pins or which chat was open. One small
/// JSON file under the user's application-data folder.
/// </summary>
public sealed class UiState
{
    public Dictionary<string, bool> Pins { get; set; } = new();
    public string? Folder { get; set; }
    public string? Port { get; set; }
    public string? Model { get; set; }
    public string? Variant { get; set; }
    public string? ActiveSession { get; set; }

    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "opencode-agent",
        "ui.json");

    public static UiState Load()
    {
        try
        {
            if (File.Exists(FilePath))
                return JsonSerializer.Deserialize<UiState>(File.ReadAllText(FilePath)) ?? new UiState();
        }
        catch
        {
            // corrupt or unreadable state must never block the app from starting
        }
        return new UiState();
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
            // a failed state write is not worth interrupting the chat
        }
    }
}
