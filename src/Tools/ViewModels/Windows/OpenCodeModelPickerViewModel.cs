using System.Collections.ObjectModel;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using Tools.Helpers;

namespace Tools.ViewModels.Windows;

/// <summary>
/// The editable OpenCode model ComboBox's state, extracted so a view can host several
/// pickers over one catalog (the Repo Settings drawer's default and commit model fields).
/// Owns the searchable dropdown projection, the committed selection, and the syncing
/// guard; the host loads the catalog (via <see cref="BeginCatalogRefresh"/>/
/// <see cref="EndCatalogRefresh"/> around its cached-then-fresh applies) and seeds each
/// picker with the field value it edits.
/// <para>
/// Same contract as the OpenCode launch drawer's picker: the ComboBox commits its
/// selection through the hosting component's code-behind (<see cref="CommitPick"/>) instead
/// of a TwoWay SelectedItem binding, so the in-place ItemsSource rebuilds never write a
/// transient null back in. Unlike the launch picker, the box TEXT is the value the host
/// saves (<see cref="ResolveCommittedValue"/>) — a settings field keeps custom ids the
/// catalog doesn't list, and clearing the box clears the value.
/// </para>
/// </summary>
public partial class OpenCodeModelPickerViewModel : ObservableObject
{
    /// <summary>The full model catalog the dropdown projects from.</summary>
    [ObservableProperty]
    private ObservableCollection<string> _models = new();

    /// <summary>
    /// The committed dropdown selection, bound OneWay so the box genuinely highlights the
    /// entry; user picks are committed by the component's SelectionChanged handler (see
    /// <see cref="CommitPick"/>). Purely UI state — the host saves
    /// <see cref="ResolveCommittedValue"/>, not this.
    /// </summary>
    [ObservableProperty]
    private string _selectedModel = string.Empty;

    /// <summary>The editable ComboBox's live text: the search while open, the value when saved.</summary>
    [ObservableProperty]
    private string _filter = string.Empty;

    /// <summary>The dropdown list: <see cref="Models"/> filtered by <see cref="Filter"/>.</summary>
    [ObservableProperty]
    private ObservableCollection<string> _filteredModels = new();

    /// <summary>Whether the catalog has any entries (drives the "no models loaded" hint).</summary>
    public bool HasModels => Models.Count > 0;

    /// <summary>
    /// True while the host refreshes the catalog: the ItemsSource swap transiently
    /// re-selects entries and re-fires the commit handler, which must not treat those
    /// phantom picks as user selections. Spans the host's whole cached-then-fresh apply —
    /// including the await between them and the deferred SelectionChanged each ItemsSource
    /// swap raises — and clears on a posted Background pass (a synchronous reset would miss
    /// that deferred phantom).
    /// </summary>
    private bool _syncing;

    /// <summary>Begins an atomic catalog refresh; see <see cref="_syncing"/>.</summary>
    public void BeginCatalogRefresh() => _syncing = true;

    /// <summary>Ends an atomic catalog refresh on the next Background dispatcher pass.</summary>
    public void EndCatalogRefresh()
        => Dispatcher.UIThread.Post(() => _syncing = false, DispatcherPriority.Background);

    /// <summary>
    /// Swaps the catalog in, shows the field's value in the box, and selects it when the
    /// catalog lists it (matched case-insensitively, resolved to the list's casing — a
    /// custom id the catalog doesn't list stays verbatim in the box, unselected). A value
    /// the user already picked survives the refresh when still listed. Callers run this
    /// inside a <see cref="BeginCatalogRefresh"/> scope.
    /// </summary>
    public void ApplyCatalog(IReadOnlyList<string> models, string? fieldValue)
    {
        Models = new ObservableCollection<string>(models);

        var previous = SelectedModel;
        var previousStillListed = !string.IsNullOrWhiteSpace(previous) && Contains(previous);
        SelectedModel = previousStillListed
            ? previous
            : ResolveMember(fieldValue) ?? string.Empty;
        Filter = previousStillListed
            ? previous
            : SelectedModel.Length > 0
                ? SelectedModel
                : fieldValue ?? string.Empty;
        RefreshFilteredModels();

        // Re-raise so the OneWay SelectedItem binding re-resolves after the in-place list
        // rebuild — including when the value did not change and ObservableProperty raised
        // nothing. Safe from text clobbering: the filter was just mirrored to the same
        // value, and the commit handler re-commits equal values (no loop).
        OnPropertyChanged(nameof(SelectedModel));

        OnPropertyChanged(nameof(HasModels));
    }

    /// <summary>
    /// Commits a model picked from the dropdown (called by the hosting component's
    /// code-behind): updates the selection and snaps the box text to the full id. Dropped
    /// while a catalog refresh is in flight — those are phantom picks from the list
    /// rebuild, not user selections.
    /// </summary>
    public void CommitPick(string model)
    {
        if (_syncing) return;
        if (string.IsNullOrWhiteSpace(model)) return;

        SelectedModel = model;
        Filter = model;
    }

    /// <summary>
    /// The value the host saves: the box text with an exact catalog match resolved to the
    /// list's casing; anything else verbatim (a custom id, or empty = the field's
    /// "unset" meaning). The text wins over the selection so a cleared box clears the
    /// value even while a stale selection remains.
    /// </summary>
    public string ResolveCommittedValue()
    {
        var typed = Filter?.Trim() ?? string.Empty;
        if (typed.Length == 0)
            return string.Empty;

        return ResolveMember(typed) ?? typed;
    }

    private bool Contains(string model)
        => ResolveMember(model) is not null;

    /// <summary>The catalog's own entry for <paramref name="model"/> (case-insensitive), or null.</summary>
    private string? ResolveMember(string? model)
    {
        if (string.IsNullOrWhiteSpace(model))
            return null;

        return Models.FirstOrDefault(m => string.Equals(m, model, StringComparison.OrdinalIgnoreCase));
    }

    /// <summary>
    /// Rebuilds <see cref="FilteredModels"/> from <see cref="Models"/> using the current
    /// filter. Must not run synchronously from a filter writeback that originates inside
    /// the ComboBox's own selection update — see <see cref="ScheduleFilteredModelsRefresh"/>.
    /// </summary>
    private void RefreshFilteredModels()
    {
        var filter = Filter ?? string.Empty;
        bool isFullSelection = string.IsNullOrEmpty(filter)
            || string.Equals(filter, SelectedModel, StringComparison.Ordinal);
        var source = (isFullSelection
            ? Models
            : Models.Where(m => m.Contains(filter, StringComparison.OrdinalIgnoreCase))).ToList();

        // Rebuild in place rather than swapping in a new instance: the ComboBox's Text
        // binding raises the filter change from inside the control's own selection
        // update, and re-sourcing ItemsSource there throws "Cannot change source while
        // update is in progress". Skip the rebuild entirely when the projection already
        // matches — Clear() raises a Reset which drops the control-side selection even
        // when the content is identical.
        if (source.Count == FilteredModels.Count && source.SequenceEqual(FilteredModels))
            return;

        FilteredModels.Clear();
        foreach (var model in source)
            FilteredModels.Add(model);
    }

    /// <summary>Coalesces the deferred <see cref="RefreshFilteredModels"/> passes —
    /// one queued dispatcher pass per burst.</summary>
    private readonly UiPostOnce _filteredModelsRefreshPost = new();

    partial void OnFilterChanged(string value) => ScheduleFilteredModelsRefresh();

    /// <summary>
    /// Schedules <see cref="RefreshFilteredModels"/> on the next dispatcher pass,
    /// coalescing bursts into one rebuild. The deferral is load-bearing: mutating the
    /// filtered list synchronously from the Text writeback raises CollectionChanged
    /// re-entrantly inside the ComboBox's selection update and the selection model throws.
    /// </summary>
    private void ScheduleFilteredModelsRefresh()
    {
        _filteredModelsRefreshPost.Post(() =>
        {
            // Capture whether the box is supposed to be showing the committed selection
            // before rebuilding — while a user search is in flight the filter differs.
            bool boxShowsSelection = string.Equals(Filter, SelectedModel, StringComparison.Ordinal);
            RefreshFilteredModels();

            // A rebuild that actually runs drops the ComboBox's control-side selection;
            // when the box was showing the committed selection, re-push it so the OneWay
            // SelectedItem binding re-resolves and reselects the entry.
            if (boxShowsSelection && !string.IsNullOrEmpty(SelectedModel))
                OnPropertyChanged(nameof(SelectedModel));
        });
    }

    partial void OnModelsChanged(ObservableCollection<string> value)
        => ScheduleFilteredModelsRefresh();
}
