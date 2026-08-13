using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Pages;

namespace Tools.Views.Pages;

public partial class ReposPage : UserControl
{
    /// <summary>
    /// Set while the editable model ComboBox is committing a selection so the auto-open-on-type
    /// handler doesn't re-pop the dropdown right after the user picks an item.
    /// </summary>
    private bool _suppressAutoOpenModelDropdown;

    public ReposViewModel ViewModel { get; }

    public ReposPage()
    {
        InitializeComponent();
    }

    public ReposPage(ReposViewModel viewModel)
    {
        ViewModel = viewModel;
        DataContext = viewModel;
        InitializeComponent();
        Loaded += OnLoaded;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    private void OnLoaded(object? sender, RoutedEventArgs e)
    {
        if (this.FindControl<ComboBox>("OpenCodeModelPicker") is not { } picker)
        {
            return;
        }

        // The editable ComboBox's inner TextBox is a template part created after ApplyTemplate.
        // Its TextChangedEvent bubbles up to the ComboBox, so hook it there to open the dropdown
        // automatically while typing/deleting.
        picker.AddHandler(TextBox.TextChangedEvent, OnOpenCodeModelFilterChanged);
    }

    /// <summary>
    /// Captures a model picked from the editable ComboBox's dropdown. The editable box is bound
    /// two-way to <see cref="ReposViewModel.OpenCodeModelFilter"/> (the live search text), so the
    /// actual selection is committed here and the filter text is snapped back to the chosen
    /// model's full name — otherwise the box would keep showing the partial search term.
    /// </summary>
    /// <summary>
    /// Swallows pointer presses on the card background before they bubble up to the
    /// ListBoxItem, which would otherwise select the item and flash the theme's
    /// selected-state indicator. Buttons inside the template sit deeper in the visual
    /// tree and handle their own presses first, so their clicks are unaffected.
    /// </summary>
    private void OnCardPointerPressed(object? sender, PointerPressedEventArgs e)
    {
        e.Handled = true;
    }

    /// <summary>
    /// The repo cards are non-interactive containers — every action is an explicit button
    /// inside the template — so ListBox selection carries no meaning. Clear it immediately
    /// to suppress the theme's selected-state indicator (the pill shown on a clicked card).
    /// </summary>
    private void OnRepoSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ListBox { SelectedItem: not null } listBox)
        {
            listBox.SelectedItem = null;
        }
    }

    private void OnOpenCodeModelSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (sender is ComboBox { SelectedItem: string model } && ViewModel is { } vm)
        {
            // The filter update below changes the box text and would otherwise re-open the
            // dropdown that the selection just closed; suppress that for this cycle.
            _suppressAutoOpenModelDropdown = true;
            try
            {
                vm.OpenCodeSelectedModel = model;
                vm.OpenCodeModelFilter = model;
            }
            finally
            {
                _suppressAutoOpenModelDropdown = false;
            }
        }
    }

    /// <summary>
    /// Opens the dropdown as the user types into (or deletes from) the editable ComboBox. The
    /// filter narrows the list in the view model; this just ensures the popup is visible while
    /// editing so matches are shown without a separate arrow click. Only fires for user-initiated
    /// edits (the box has keyboard focus) so programmatic text changes — e.g. the view model
    /// resetting the filter when models load — don't pop the dropdown open.
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
