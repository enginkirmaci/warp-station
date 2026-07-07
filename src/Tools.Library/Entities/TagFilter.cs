using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Entities;

/// <summary>
/// A checkable tag entry shown in the left filter panel. Backs a checkbox whose
/// <see cref="IsChecked"/> state the ViewModel watches to re-apply the OR filter.
/// </summary>
public partial class TagFilter : ObservableObject
{
    /// <summary>
    /// Gets the tag name this filter represents.
    /// </summary>
    public string Name { get; }

    [ObservableProperty]
    private bool _isChecked;

    public TagFilter(string name)
    {
        Name = name;
    }
}
