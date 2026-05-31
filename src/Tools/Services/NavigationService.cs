using Avalonia.Controls;
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
        Control? newPage = null;
        try
        {
            newPage = App.Services.GetService(pageType) as Control;
        }
        catch { }
        newPage ??= Activator.CreateInstance(pageType) as Control;
        if (newPage == null)
            return false;
        if (_contentControl.Content is Control currentPage && currentPage.GetType() != pageType)
        {
            _backStack.Push((currentPage, currentPage.GetType()));
            BackStackChanged?.Invoke();
        }
        _contentControl.Content = newPage;
        try { Navigated?.Invoke(pageType); } catch { }
        return true;
    }

    public bool GoBack()
    {
        if (_contentControl == null || _backStack.Count == 0)
            return false;
        var (previousPage, previousType) = _backStack.Pop();
        _contentControl.Content = previousPage;
        try { Navigated?.Invoke(previousType); } catch { }
        BackStackChanged?.Invoke();
        return true;
    }
}
