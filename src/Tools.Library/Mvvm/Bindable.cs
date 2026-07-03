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
