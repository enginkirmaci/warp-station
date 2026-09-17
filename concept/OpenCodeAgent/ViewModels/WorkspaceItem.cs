using System.IO;
using CommunityToolkit.Mvvm.ComponentModel;
using OpenCodeAgent.Services;

namespace OpenCodeAgent.ViewModels;

/// <summary>One added workspace folder; owns its own server instance and chat list.</summary>
public sealed class WorkspaceItem : ObservableObject
{
    private bool _isRunning;

    public WorkspaceItem(WorkspaceState state)
    {
        State = state;
        Name = Path.GetFileName(state.Path.TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar)) is { Length: > 0 } name
            ? name
            : state.Path;
    }

    public WorkspaceState State { get; }

    /// <summary>Normalized absolute path; also the runtime dictionary key.</summary>
    public string FolderPath => State.Path;

    public string Name { get; }

    public bool IsRunning { get => _isRunning; set => SetProperty(ref _isRunning, value); }
}
