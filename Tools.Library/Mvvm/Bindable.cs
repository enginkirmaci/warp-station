using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Mvvm;

/// <summary>
/// Base class for all ViewModels providing observable property functionality.
/// </summary>
public abstract class ViewModelBase : ObservableObject
{
    private bool _isBusy;

    /// <summary>
    /// Gets or sets a value indicating whether this instance is busy.
    /// </summary>
    public bool IsBusy
    {
        get => _isBusy;
        set => SetProperty(ref _isBusy, value);
    }

    /// <summary>
    /// Gets a value indicating whether this instance is not busy.
    /// </summary>
    public bool IsNotBusy => !IsBusy;
}

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