using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

/// <inheritdoc/>
public class ToolDrawerService : IToolDrawerService
{
    public bool IsOpen { get; private set; }

    public string? SelectedToolKey { get; private set; }

    public object? Context { get; private set; }

    public event Action? Changed;

    /// <inheritdoc/>
    public void Open(string toolKey, object? context = null)
    {
        if (string.IsNullOrEmpty(toolKey))
        {
            return;
        }

        // Re-picking the tool that is already showing is a no-op: the drawer stays
        // open with its current component instead of rebuilding it and losing state.
        if (IsOpen && SelectedToolKey == toolKey)
        {
            return;
        }

        IsOpen = true;
        SelectedToolKey = toolKey;
        Context = context;
        RaiseChanged();
    }

    /// <inheritdoc/>
    public void Close()
    {
        if (!IsOpen)
        {
            return;
        }

        IsOpen = false;
        SelectedToolKey = null;
        Context = null;
        RaiseChanged();
    }

    private void RaiseChanged()
    {
        try
        {
            Changed?.Invoke();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "A ToolDrawerService.Changed subscriber threw");
        }
    }
}
