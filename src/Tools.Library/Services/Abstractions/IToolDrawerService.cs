namespace Tools.Library.Services.Abstractions;

/// <summary>
/// State of the main window's floating tool drawer (the right sidebar that hosts a
/// tool component over the open page). The title-bar tools dropdown funnels its tool
/// selections through here, so the drawer state has a single owner; the window
/// mirrors the state into its visuals on <see cref="Changed"/>.
/// </summary>
public interface IToolDrawerService
{
    /// <summary>Gets a value indicating whether the drawer is currently shown.</summary>
    bool IsOpen { get; }

    /// <summary>
    /// Gets the tool key of the component currently shown (or just closed) in the
    /// drawer. Cleared on close so every open resolves a fresh component instance.
    /// </summary>
    string? SelectedToolKey { get; }

    /// <summary>
    /// Gets the context payload the current open was made with, or <c>null</c>. Tools
    /// open without one; drawer-hosted dialogs (Add Repositories, Repo Settings) pass
    /// their request state and completion source here. Cleared on close.
    /// </summary>
    object? Context { get; }

    /// <summary>Raised whenever <see cref="IsOpen"/> or <see cref="SelectedToolKey"/> changes.</summary>
    event Action? Changed;

    /// <summary>
    /// Opens the drawer on the given tool key. Re-picking the tool that is already
    /// showing is a no-op, so the component keeps its state.
    /// </summary>
    /// <param name="toolKey">The key of the tool to show (see ToolComponentMapper).</param>
    /// <param name="context">
    /// Optional payload for this open; delivered to the component's ViewModel when it
    /// implements <see cref="IToolDrawerContextReceiver"/>.
    /// </param>
    void Open(string toolKey, object? context = null);

    /// <summary>Closes the drawer if it is open.</summary>
    void Close();
}
