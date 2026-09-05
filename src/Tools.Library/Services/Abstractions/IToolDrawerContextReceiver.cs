namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Implemented by ViewModels of tool-drawer components that need the payload their open
/// was made with. The main window delivers the drawer service's current context once per
/// open, right after the component view is resolved and hosted. Tools opened from the
/// dropdown have no context; drawer-hosted dialogs (Add Repositories, Repo Settings)
/// receive their request state and completion source through it.
/// </summary>
public interface IToolDrawerContextReceiver
{
    /// <summary>Called once per drawer open with the payload passed to <c>Open</c>.</summary>
    /// <param name="context">The open's context payload; the receiver type-checks it.</param>
    void OnDrawerContext(object context);
}
