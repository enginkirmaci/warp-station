using Avalonia.Controls;
using Serilog;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

public class NavigationService : INavigationService
{
    private ContentControl? _contentControl;
    private readonly Stack<(Control Page, Type PageType)> _backStack = new();

    public bool CanGoBack => _backStack.Count > 0;

    public event Action<Type?>? Navigated;
    public event Action? BackStackChanged;

    public void SetContentControl(ContentControl contentControl)
    {
        _contentControl = contentControl;
    }

    public bool Navigate(Type pageType, object? parameter = null)
    {
        if (_contentControl == null)
            return false;

        // Resolve the page from the DI container. All pages require DI, so there is no
        // Activator.CreateInstance fallback (it would throw for pages without a
        // parameterless constructor).
        Control? newPage = App.Services.GetService(pageType) as Control;
        if (newPage == null)
        {
            Log.Logger.Warning("Navigation target page '{PageType}' could not be resolved", pageType.Name);
            return false;
        }

        // Notify the outgoing page's ViewModel that it is being navigated away from.
        if (_contentControl.Content is Control currentPage
            && currentPage.GetType() != pageType
            && currentPage.DataContext is PageViewModelBase outgoingVm)
        {
            FireLifecycle(() => outgoingVm.OnNavigatedFromAsync(), nameof(PageViewModelBase.OnNavigatedFromAsync));
        }

        // Push the current page onto the back stack when navigating to a different page.
        if (_contentControl.Content is Control current && current.GetType() != pageType)
        {
            _backStack.Push((current, current.GetType()));
            BackStackChanged?.Invoke();
        }

        _contentControl.Content = newPage;

        // Notify the incoming page's ViewModel that it was navigated to.
        if (newPage.DataContext is PageViewModelBase incomingVm)
        {
            FireLifecycle(() => incomingVm.OnNavigatedToAsync(parameter), nameof(PageViewModelBase.OnNavigatedToAsync));
        }

        try
        {
            Navigated?.Invoke(pageType);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "A Navigated event subscriber threw");
        }
        return true;
    }

    public bool GoBack()
    {
        if (_contentControl == null || _backStack.Count == 0)
            return false;

        var (previousPage, previousType) = _backStack.Pop();

        // Notify the outgoing page's ViewModel that it is being navigated away from.
        if (_contentControl.Content is Control currentPage
            && currentPage.DataContext is PageViewModelBase outgoingVm)
        {
            FireLifecycle(() => outgoingVm.OnNavigatedFromAsync(), nameof(PageViewModelBase.OnNavigatedFromAsync));
        }

        _contentControl.Content = previousPage;

        // Notify the restored page's ViewModel that it was navigated to.
        if (previousPage.DataContext is PageViewModelBase incomingVm)
        {
            FireLifecycle(() => incomingVm.OnNavigatedToAsync(), nameof(PageViewModelBase.OnNavigatedToAsync));
        }

        try
        {
            Navigated?.Invoke(previousType);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "A Navigated event subscriber threw");
        }
        BackStackChanged?.Invoke();
        return true;
    }

    /// <summary>
    /// Invokes an asynchronous page-lifecycle hook, surfacing failures via the logger
    /// instead of silently swallowing them. Fire-and-forget mirrors the existing VM
    /// self-initialization pattern while keeping navigation itself synchronous.
    /// </summary>
    private static void FireLifecycle(Func<Task> hook, string hookName)
    {
        _ = Task.Run(async () =>
        {
            try
            {
                await hook();
            }
            catch (Exception ex)
            {
                Log.Logger.Error(ex, "Page lifecycle hook {HookName} threw", hookName);
            }
        });
    }
}
