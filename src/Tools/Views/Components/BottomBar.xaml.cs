using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// Bottom bar of the Repos page: tab strip (always visible once a repo is selected from
/// the table) and the active tab's expandable panel. Its DataContext is the singleton
/// <see cref="BottomBarViewModel"/> (attached by ReposPage). The two ComboBoxes commit
/// their selections through code-behind handlers instead of TwoWay bindings so the
/// in-place ItemsSource rebuilds (model list refresh, branch reload) never write a
/// transient null back into the view model.
/// </summary>
public partial class BottomBar : UserControl
{
    /// <summary>
    /// Set while the editable model ComboBox is committing a selection so the auto-open-on-type
    /// handler doesn't re-pop the dropdown right after the user picks an item.
    /// </summary>
    private bool _suppressAutoOpenModelDropdown;

    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public BottomBar()
    {
        InitializeComponent();
        WireModelPicker();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// The editable ComboBox's inner TextBox is a template part created after ApplyTemplate.
    /// Its TextChangedEvent bubbles up to the ComboBox, so hook it there to open the dropdown
    /// automatically while typing/deleting.
    /// </summary>
    private void WireModelPicker()
    {
        if (this.FindControl<ComboBox>("OpenCodeModelPicker") is { } picker)
        {
            picker.AddHandler(TextBox.TextChangedEvent, OnOpenCodeModelFilterChanged);
        }
    }

    /// <summary>
    /// Commits a branch pick: the ViewModel checks out the branch (see
    /// <see cref="BottomBarViewModel.OnSelectedBranchChanged"/>). Programmatic syncs
    /// re-commit the current branch and are ignored there.
    /// </summary>
    private void OnBranchSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (e.AddedItems.Count > 0 && e.AddedItems[0] is string branch && ViewModel is { } vm)
        {
            vm.SelectedBranch = branch;
        }
    }

    /// <summary>
    /// Captures a model picked from the editable ComboBox's dropdown and persists it as
    /// the configured default model. The editable box is bound two-way to
    /// <see cref="BottomBarViewModel.OpenCodeModelFilter"/> (the live search text), so
    /// the actual selection is committed here — the filter text is snapped back to the
    /// chosen model's full name inside the commit.
    /// </summary>
    private void OnOpenCodeModelSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string model } && ViewModel is { } vm)
        {
            // The filter update inside the commit changes the box text and would otherwise
            // re-open the dropdown that the selection just closed; suppress that for this
            // cycle.
            _suppressAutoOpenModelDropdown = true;
            try
            {
                vm.OpenCodeModelFilter = model;
                _ = vm.CommitOpenCodeModelAsync(model);
            }
            finally
            {
                _suppressAutoOpenModelDropdown = false;
            }
        }
    }

    /// <summary>
    /// Opens the dropdown as the user types into (or deletes from) the editable ComboBox.
    /// Only fires for user-initiated edits (the box has keyboard focus) so programmatic
    /// text changes don't pop the dropdown open.
    /// </summary>
    private void OnOpenCodeModelFilterChanged(object? sender, TextChangedEventArgs e)
    {
        if (_suppressAutoOpenModelDropdown)
        {
            return;
        }

        if (sender is ComboBox box
            && box.IsEnabled
            && !box.IsDropDownOpen
            && box.IsKeyboardFocusWithin)
        {
            box.IsDropDownOpen = true;
        }
    }
}
