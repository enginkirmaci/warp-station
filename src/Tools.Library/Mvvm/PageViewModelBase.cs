namespace Tools.Library.Mvvm;

/// <summary>
/// Base class for page ViewModels with lifecycle management.
/// </summary>
public abstract class PageViewModelBase : ViewModelBase
{
    /// <summary>
    /// Called when the ViewModel is initialized for the first time.
    /// Override this method to perform initialization logic.
    /// </summary>
    public virtual Task OnInitializeAsync()
    {
        return Task.CompletedTask;
    }

    /// <summary>
    /// Called each time the page is navigated to.
    /// Override this method to perform logic that should run on every navigation.
    /// </summary>
    /// <param name="parameter">Navigation parameter.</param>
    public virtual Task OnNavigatedToAsync(object? parameter = null)
    {
        return Task.CompletedTask;
    }

    /// <summary>
    /// Called when the page is navigated away from.
    /// Override this method to perform cleanup logic.
    /// </summary>
    public virtual Task OnNavigatedFromAsync()
    {
        return Task.CompletedTask;
    }
}