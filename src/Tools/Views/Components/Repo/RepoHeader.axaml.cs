using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Tools.ViewModels.Components.BottomBar;

namespace Tools.Views.Components.Repo;

/// <summary>
/// Repo detail header at the top of the Repositories page (back link, repo name + star,
/// branch / GitHub URL line, tag chip row, tab row, Open in GitHub + kebab). Its
/// DataContext is the singleton <see cref="BottomBarViewModel"/> attached by ReposPage —
/// the same VM as the bottom bar, so the tabs here drive the bar's panel.
/// <para>
/// The add-tag flyout is wired here (Enter / Add button share one path): read the box,
/// let the shell VM add the tag (its duplicate check toasts a warning), clear the box
/// only when the tag landed so a typo survives to be fixed, then refocus for the next
/// entry; Escape or the ✕ hides the flyout. Below the input sits the picker — every
/// tag known across the repos that this repo does not carry yet, one clickable chip
/// each (a click goes through the same add path, then removes the chip from the
/// picker); rebuilt on every open and after each landed tag.
/// </para>
/// </summary>
public partial class RepoHeader : UserControl
{
    public RepoHeader()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    /// <summary>Adds the input box's text as a tag on the selected repo (Enter and the
    /// Add button both land here). Keeps the flyout open for back-to-back entries.</summary>
    private async void AddTagFromInput()
    {
        if (DataContext is not BottomBarViewModel vm) return;
        if (this.FindControl<TextBox>("NewTagInput") is not { } input) return;

        var added = await vm.AddRepoTagAsync(input.Text);
        if (added)
        {
            input.Text = string.Empty;
            RebuildAvailableTags();
        }

        input.Focus();
    }

    /// <summary>Fills the picker with the tags this repo does not carry yet (sorted like
    /// the Repos page's tag filters); hides the whole section when nothing remains.
    /// Runs on every flyout open and after each tag that lands. Reads the repo
    /// entity's own tag list — the SelectedRepoTags mirror is refilled from a posted
    /// TagsChanged, so it can still lag one add behind at this point.</summary>
    private void RebuildAvailableTags()
    {
        if (DataContext is not BottomBarViewModel vm) return;
        if (this.FindControl<StackPanel>("AvailableTagsSection") is not { } section) return;
        if (this.FindControl<ItemsControl>("AvailableTagsList") is not { } list) return;

        var current = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        if (vm.SelectedRepo is { } repo)
        {
            foreach (var tag in repo.Tags)
            {
                current.Add(tag.Name);
            }
        }
        var tags = vm.AvailableTags
            .Where(t => !current.Contains(t))
            .OrderBy(t => t, StringComparer.OrdinalIgnoreCase)
            .ToList();

        section.IsVisible = tags.Count > 0;
        list.ItemsSource = tags;
    }

    /// <summary>A picker chip click adds that existing tag through the same VM path as
    /// a typed name (dedupe check, TagsChanged) and drops the chip from the picker.</summary>
    private async void AvailableTag_OnTapped(object? sender, TappedEventArgs e)
    {
        if (DataContext is not BottomBarViewModel vm) return;
        if (sender is not Control { DataContext: string tag }) return;

        if (await vm.AddRepoTagAsync(tag))
        {
            RebuildAvailableTags();
        }
    }

    private void NewTagInput_OnKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            AddTagFromInput();
        }
        else if (e.Key == Key.Escape)
        {
            e.Handled = true;
            HideAddTagFlyout();
        }
    }

    private void AddTagConfirm_OnClick(object? sender, RoutedEventArgs e)
    {
        AddTagFromInput();
    }

    private void AddTagClose_OnClick(object? sender, RoutedEventArgs e)
    {
        HideAddTagFlyout();
    }

    private void HideAddTagFlyout()
    {
        (this.FindControl<Button>("AddTagButton")?.Flyout as Flyout)?.Hide();
    }

    /// <summary>Each open starts from a clean, focused input (a fresh repo must not
    /// inherit the previous one's half-typed name) and a fresh picker.</summary>
    private void AddTagFlyout_OnOpened(object? sender, System.EventArgs e)
    {
        if (this.FindControl<TextBox>("NewTagInput") is { } input)
        {
            input.Text = string.Empty;
            input.Focus();
        }

        RebuildAvailableTags();
    }
}
