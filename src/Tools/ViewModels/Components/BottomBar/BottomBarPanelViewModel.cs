using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using Serilog;
using Tools.Library.Entities;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Components.BottomBar;

/// <summary>
/// Shared mechanics of the bottom bar's per-tab view models. Each panel hangs off
/// the singleton <see cref="BottomBarViewModel"/> (the shell owns the selected repo,
/// the tab switching and the settings flags); the shell reference gives the panels
/// the repo snapshot and the services their loads need, so the bar's dependency
/// list stays injected once.
/// <para>
/// The loader helpers here are the "snapshot the repo, fetch, drop the result when
/// the user switched repos mid-load, swap the list under a busy flag, raise the
/// derived bindings" idiom every panel load repeats — factored out once.
/// </para>
/// </summary>
public abstract class BottomBarPanelViewModel : ObservableObject
{
    protected BottomBarPanelViewModel(BottomBarViewModel shell) => Shell = shell;

    /// <summary>The bar shell this panel belongs to (repo snapshot + services).</summary>
    public BottomBarViewModel Shell { get; }

    /// <summary>The repo the bar acts on right now (null until the user picks one).</summary>
    protected Repo? Repo => Shell.SelectedRepo;

    /// <summary>
    /// Fired whenever the panel's collections or load-state flags settle (apply or
    /// clear). The shell subscribes to re-raise its cross-panel aggregates — the tab
    /// header totals and the empty-state notes read both providers' state.
    /// </summary>
    public event Action? StateChanged;

    /// <summary>Raises <see cref="StateChanged"/> — call wherever an apply/clear lands.</summary>
    protected void RaiseStateChanged() => StateChanged?.Invoke();

    /// <summary>
    /// Same repo across a rescan re-resolve (fresh instance, same folder) counts as
    /// unchanged; only a genuine switch (different folder, or null) returns false.
    /// </summary>
    internal static bool IsSameRepo(Repo? a, Repo? b)
    {
        if (ReferenceEquals(a, b)) return true;
        if (a is null || b is null) return false;
        return RepoPath.SamePath(a.FolderPath ?? string.Empty, b.FolderPath ?? string.Empty);
    }

    /// <summary>Rebuilds a bound collection in place (Clear + Add) — the loaders' swap
    /// step, which keeps the collection instance (and its bindings) alive.</summary>
    protected static void ReplaceItems<T>(ObservableCollection<T> target, IEnumerable<T> items)
    {
        target.Clear();
        foreach (var item in items)
        {
            target.Add(item);
        }
    }

    /// <summary>
    /// Fetches a payload for the snapshotted repo and hands it back only when the user
    /// has not switched repos while the fetch ran — the "repo switched mid-load" guard
    /// every panel loader repeats. Returns null when the load was abandoned mid-flight
    /// (the caller then skips its state updates).
    /// </summary>
    protected async Task<T?> FetchIfCurrentAsync<T>(Repo repo, Func<Repo, Task<T>> fetch) where T : class
    {
        var payload = await fetch(repo);
        if (!ReferenceEquals(Shell.SelectedRepo, repo)) return default; // repo switched while loading
        return payload;
    }

    /// <summary>Runs a panel load under its busy flag: raises it up front, restores it
    /// in finally and pushes the flag-derived bindings — the try/finally shape every
    /// loader repeated. Exceptions are logged here (every call site discards the task
    /// or hands it to a relay command, so a propagating exception would vanish
    /// unobserved) and, when <paramref name="errorText"/> is given, toasted.</summary>
    protected async Task RunBusyAsync(Action<bool> setBusy, Func<Task> load, Action raiseSettled, Func<string>? errorText = null)
    {
        setBusy(true);
        try
        {
            await load();
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Panel load failed for {FolderPath}", Shell.SelectedRepo?.FolderPath);
            if (errorText?.Invoke() is { } error)
            {
                Shell.Notifications.Show(error, NotificationKind.Error);
            }
        }
        finally
        {
            setBusy(false);
            raiseSettled();
        }
    }

    /// <summary>
    /// The GitHub/Azure load twin: seed the panel from the service's cache so opening
    /// the tab is instant, then run the refresh that replaces the lists when it lands.
    /// The caller owns the null-repo branch (each panel clears its own collections).
    /// </summary>
    protected async Task LoadProviderAsync<TActivity>(
        Func<TActivity?> getCached,
        Action<TActivity> apply,
        Func<Task> refresh)
    {
        var cached = getCached();
        if (cached is not null)
        {
            apply(cached);
        }

        await refresh();
    }

    /// <summary>
    /// The GitHub/Azure refresh twin: fetches the provider's activity for the selected
    /// repo under the panel's busy flag, keeps the previous lists when the repo switched
    /// mid-load or the fetch failed (logged with the provider label — a failed fetch
    /// returns an empty activity, and applying it would flash a misleading all-clear),
    /// surfaces the provider's availability note and applies the activity. A no-op when
    /// there is no repo or <paramref name="canStart"/> refuses (the Azure panel never
    /// overlaps its own refresh; the GitHub one relies on its command's CanExecute).
    /// </summary>
    protected async Task RefreshProviderAsync<TActivity>(
        string providerLabel,
        Func<bool> canStart,
        Action<bool> setRefreshing,
        Func<Repo, Task<TActivity>> fetch,
        Action<Repo> setUnavailable,
        Action<TActivity> apply) where TActivity : class
    {
        var repo = Shell.SelectedRepo;
        if (repo is null || !canStart()) return;

        setRefreshing(true);
        try
        {
            var activity = await FetchIfCurrentAsync(repo, fetch);
            if (activity is null) return; // repo switched while loading
            setUnavailable(repo);
            apply(activity);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "{Provider} panel refresh failed for {FolderPath}", providerLabel, repo.FolderPath);
            Shell.Notifications.Show($"{providerLabel} refresh failed", NotificationKind.Error);
        }
        finally
        {
            setRefreshing(false);
        }
    }
}
