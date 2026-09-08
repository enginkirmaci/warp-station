using Avalonia.Controls;
using Tools.Views.Components;

namespace Tools.Helpers;

/// <summary>
/// Resolves a fresh drawer component view for a tool key. Registered in DI by the
/// composition root (App) so the main window can host drawer components without
/// depending on the service container itself. Each call returns a new transient
/// view (with its transient ViewModel), one per drawer open.
/// </summary>
public delegate Control? ToolViewResolver(string key);

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

    /// <summary>Drawer key of the OpenCode settings component (opened by the row options icon and the tools dropdown).</summary>
    public const string OpenCodeKey = "OpenCodePage";

    /// <summary>Drawer key of the NuGet Local component (opened by the tools dropdown).</summary>
    public const string NugetLocalKey = "NugetLocalPage";

    /// <summary>Drawer key of the Formatters component (opened by the tools dropdown).</summary>
    public const string FormattersKey = "FormattersPage";

    /// <summary>Drawer key of the Clipboard Password component (opened by the tools dropdown).</summary>
    public const string ClipboardPasswordKey = "ClipboardPasswordPage";

    /// <summary>Drawer key of the Code Execute component (opened by the tools dropdown).</summary>
    public const string CodeExecuteKey = "CodeExecutePage";

    /// <summary>Drawer key of the SnapIt settings component (opened by the tools dropdown).</summary>
    public const string SnapItSettingsKey = "SnapItSettingsPage";

    private static readonly ToolDefinition[] _tools =
    [
        new ToolDefinition(NugetLocalKey, typeof(NugetLocalComponent), "NuGet Package Manager", "icon-package"),
        new ToolDefinition(FormattersKey, typeof(FormattersComponent), "Formatters", "icon-text-format"),
        new ToolDefinition(ClipboardPasswordKey, typeof(ClipboardPasswordComponent), "Clipboard Password", "icon-lock"),
        new ToolDefinition(CodeExecuteKey, typeof(CodeExecuteComponent), "Code Execute", "icon-terminal-alt"),
        new ToolDefinition(SnapItSettingsKey, typeof(SnapItSettingsComponent), "SnapIt", "icon-grid"),
        new ToolDefinition(OpenCodeKey, typeof(OpenCodeSettingsComponent), "OpenCode", "icon-opencode"),
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
