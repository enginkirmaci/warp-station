using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Windows;

namespace Tools.Views.Components.Repo;

/// <summary>
/// Repo Settings component, hosted in the main window's floating tool drawer (the
/// former modal dialog). A thin view: all editing state and the array/string
/// translation live in <see cref="ReposSettingsViewModel"/>, which receives the
/// settings to edit through the drawer context and resolves the edited settings on Save.
/// <para>
/// The two OpenCode model ComboBoxes commit their selections through code-behind
/// handlers instead of TwoWay bindings so the in-place ItemsSource rebuilds (model
/// catalog refresh) never write a transient null back into the pickers — same pattern
/// as the OpenCode launch drawer's picker.
/// </para>
/// </summary>
public partial class ReposSettingsComponent : UserControl
{
    /// <summary>
    /// Set while a model ComboBox is committing a selection so the auto-open-on-type
    /// handler doesn't re-pop that dropdown right after the user picks an item.
    /// </summary>
    private bool _suppressDefaultModelAutoOpen;
    private bool _suppressCommitModelAutoOpen;

    public ReposSettingsViewModel ViewModel { get; }

    public ReposSettingsComponent()
    {
        InitializeComponent();
        WireModelPickers();
    }

    public ReposSettingsComponent(ReposSettingsViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
        WireModelPickers();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>
    /// Each editable model ComboBox's inner TextBox is a template part created after
    /// ApplyTemplate. Its TextChangedEvent bubbles up to the ComboBox, so hook it there
    /// to open that dropdown automatically while typing/deleting.
    /// </summary>
    private void WireModelPickers()
    {
        if (this.FindControl<ComboBox>("DefaultModelPicker") is { } defaultPicker)
        {
            defaultPicker.AddHandler(TextBox.TextChangedEvent, OnDefaultModelTextChanged);
        }

        if (this.FindControl<ComboBox>("CommitModelPicker") is { } commitPicker)
        {
            commitPicker.AddHandler(TextBox.TextChangedEvent, OnCommitModelTextChanged);
        }
    }

    private void OnDefaultModelSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string model } && DataContext is ReposSettingsViewModel vm)
        {
            // The filter update inside the commit changes the box text and would otherwise
            // re-open the dropdown that the selection just closed; suppress that for this
            // cycle.
            _suppressDefaultModelAutoOpen = true;
            try
            {
                vm.DefaultModelPicker.CommitPick(model);
            }
            finally
            {
                _suppressDefaultModelAutoOpen = false;
            }
        }
    }

    private void OnCommitModelSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string model } && DataContext is ReposSettingsViewModel vm)
        {
            _suppressCommitModelAutoOpen = true;
            try
            {
                vm.CommitModelPicker.CommitPick(model);
            }
            finally
            {
                _suppressCommitModelAutoOpen = false;
            }
        }
    }

    /// <summary>
    /// Opens the dropdown as the user types into (or deletes from) the editable ComboBox.
    /// Only fires for user-initiated edits (the box has keyboard focus) so programmatic
    /// text changes don't pop the dropdown open.
    /// </summary>
    private void OnDefaultModelTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_suppressDefaultModelAutoOpen)
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

    private void OnCommitModelTextChanged(object? sender, TextChangedEventArgs e)
    {
        if (_suppressCommitModelAutoOpen)
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
