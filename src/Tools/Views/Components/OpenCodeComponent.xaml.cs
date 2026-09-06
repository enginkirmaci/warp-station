using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components;

namespace Tools.Views.Components;

/// <summary>
/// The OpenCode launch surface (settings + launch) shown inside the
/// <see cref="OpenCodePanel"/>. Its DataContext is the singleton
/// <see cref="BottomBarViewModel"/>, inherited from the panel. The editable model
/// ComboBox commits its selection through code-behind handlers instead of a TwoWay
/// binding so the in-place ItemsSource rebuilds (model list refresh) never write a
/// transient null back into the view model.
/// </summary>
public partial class OpenCodeComponent : UserControl
{
    /// <summary>
    /// Set while the editable model ComboBox is committing a selection so the auto-open-on-type
    /// handler doesn't re-pop the dropdown right after the user picks an item.
    /// </summary>
    private bool _suppressAutoOpenModelDropdown;

    public BottomBarViewModel? ViewModel => DataContext as BottomBarViewModel;

    public OpenCodeComponent()
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

    /// <summary>
    /// Captures a pick from the plain commit-model ComboBox (the "(use default model)"
    /// sentinel clears the dedicated setting; anything else persists the model id). The
    /// selection is committed through the view model like the default model's, not a
    /// TwoWay binding, so option-list rebuilds never write transients back.
    /// </summary>
    private void OnOpenCodeCommitModelSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string model } && ViewModel is { } vm)
        {
            _ = vm.CommitCommitModelAsync(model);
        }
    }
}
