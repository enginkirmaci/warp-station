using Tools.Views.Components;

namespace Tools.Helpers;

/// <summary>
/// Single source of truth for the tool components hosted in the main window's
/// floating tool drawer. Maps the tool keys (used by the title-bar tools dropdown
/// and by DialogService for the drawer-hosted dialogs) to their component view,
/// display title and icon asset. The keys are stable identifiers; some still carry
/// their former "Page" names from before the components conversion.
/// </summary>
public static class ToolComponentMapper
{
    /// <summary>One drawer-hostable tool: its key, component view and presentation.</summary>
    public sealed record ToolDefinition(string Key, Type ViewType, string Title, string IconAsset);

    /// <summary>Drawer key of the Add Repositories component (opened by DialogService).</summary>
    public const string AddRepositoriesKey = "AddRepositoryDialog";

    /// <summary>Drawer key of the Repo Settings component (opened by DialogService).</summary>
    public const string RepoSettingsKey = "ReposSettingsDialog";

    /// <summary>Drawer key of the History commit-detail component (opened by a History row click).</summary>
    public const string CommitHistoryKey = "CommitHistoryDialog";

    private static readonly ToolDefinition[] _tools =
    [
        new ToolDefinition("NugetLocalPage", typeof(NugetLocalComponent), "NuGet Package Manager", "icon-package"),
        new ToolDefinition("FormattersPage", typeof(FormattersComponent), "Formatters", "icon-text-format"),
        new ToolDefinition("ClipboardPasswordPage", typeof(ClipboardPasswordComponent), "Clipboard Password", "icon-lock"),
        new ToolDefinition("CodeExecutePage", typeof(CodeExecuteComponent), "Code Execute", "icon-terminal-alt"),
        new ToolDefinition("SnapItSettingsPage", typeof(SnapItSettingsComponent), "SnapIt", "icon-grid"),
        // Drawer-hosted dialogs: not in the tools dropdown, opened programmatically by
        // DialogService in place of the former modal windows.
        new ToolDefinition(AddRepositoriesKey, typeof(AddRepositoryComponent), "Add Repositories", "icon-plus"),
        new ToolDefinition(RepoSettingsKey, typeof(ReposSettingsComponent), "Repo Settings", "icon-cog"),
        new ToolDefinition(CommitHistoryKey, typeof(CommitHistoryComponent), "History", "icon-clock"),
    ];

    /// <summary>All drawer-hostable tools in menu order.</summary>
    public static IReadOnlyList<ToolDefinition> Tools => _tools;

    /// <summary>Finds the tool registered under <paramref name="key"/>, or null.</summary>
    public static ToolDefinition? Find(string? key)
    {
        if (string.IsNullOrEmpty(key))
        {
            return null;
        }

        return Array.Find(_tools, t => t.Key == key);
    }
}
