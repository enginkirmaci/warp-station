using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Windows;

/// <summary>
/// Shared plumbing for the drawer-hosted dialog ViewModels (Add Repositories, Repo
/// Settings). <see cref="Tools.Services.DialogService"/> opens these as tool-drawer
/// components, handing the ViewModel a context that carries the request state and a
/// completion source, and awaits the completion: the dialog's confirm command resolves
/// it with the result, and every other close path — ✕, Cancel, the backdrop, Escape,
/// or switching tools — resolves it with null. The base owns the completion source,
/// the context-received delivery (null deliveries are ignored), the Cancel command,
/// and the confirm plumbing; the derived dialog supplies the typed completion
/// extraction, the per-open state seed, and the result its confirm command resolves
/// with.
/// </summary>
/// <typeparam name="TContext">The drawer open payload type this dialog receives.</typeparam>
/// <typeparam name="TResult">The dialog result type the completion source resolves with.</typeparam>
public abstract partial class DrawerDialogViewModelBase<TContext, TResult> :
    ObservableObject, IToolDrawerContextReceiver<TContext>
    where TContext : class
{
    private readonly IToolDrawerService _toolDrawer;

    /// <summary>
    /// The context completion source while this instance is the drawer's open component;
    /// resolved with the dialog result on confirm, null on every other close path.
    /// </summary>
    protected TaskCompletionSource<TResult?>? Completion { get; private set; }

    protected DrawerDialogViewModelBase(IToolDrawerService toolDrawer)
    {
        _toolDrawer = toolDrawer;
    }

    /// <summary>
    /// Drawer open payload: stores the context's completion source for the confirm path
    /// and seeds the dialog state so every open starts from a clean sheet. Null
    /// deliveries (an open without this payload) are ignored. Virtual so a dialog can
    /// extend the delivery with its own async seeding (the Repo Settings drawer loads
    /// the OpenCode model catalog after the synchronous base seed) — the override still
    /// calls base first, and the synchronous prefix runs inline ahead of it.
    /// </summary>
    public virtual Task OnDrawerContextAsync(TContext? context)
    {
        if (context is null)
        {
            return Task.CompletedTask;
        }

        Completion = GetCompletion(context);
        OnDrawerContext(context);
        return Task.CompletedTask;
    }

    /// <summary>The open context's completion source the confirm path resolves.</summary>
    protected abstract TaskCompletionSource<TResult?> GetCompletion(TContext context);

    /// <summary>Per-dialog seed for a fresh open, after <see cref="Completion"/> is stored.</summary>
    protected abstract void OnDrawerContext(TContext context);

    /// <summary>Cancel/close: resolves the context with null (DialogService semantics).</summary>
    [RelayCommand]
    private void Cancel() => _toolDrawer.Close();

    /// <summary>
    /// Confirm plumbing: resolves the drawer context with the dialog result and closes
    /// the drawer. Closing raises the drawer's Changed event, which DialogService treats
    /// as a cancel — a no-op here because the completion is already set.
    /// </summary>
    protected void ConfirmWith(TResult? result)
    {
        Completion?.TrySetResult(result);
        _toolDrawer.Close();
    }
}
