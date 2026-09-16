using System.ComponentModel;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using OpenCodeAgent.Models;
using OpenCodeAgent.ViewModels;
using SukiUI.Controls;

namespace OpenCodeAgent.Views;

public partial class MainWindow : SukiWindow
{
    private readonly MainViewModel _vm = new();
    private ScrollViewer? _chatScroll;
    private Button? _jumpLatest;
    private bool _atBottom = true;

    public MainWindow()
    {
        InitializeComponent();
        DataContext = _vm;
        _chatScroll = this.FindControl<ScrollViewer>("ChatScroll");
        _jumpLatest = this.FindControl<Button>("JumpLatest");
        // an AcceptsReturn TextBox consumes Enter before bubbling; tunnel to send first
        this.FindControl<TextBox>("InputBox")?
            .AddHandler(InputElement.KeyDownEvent, OnInputKeyDown, RoutingStrategies.Tunnel);
        if (_chatScroll is not null)
        {
            _chatScroll.ScrollChanged += (_, _) => UpdateAtBottom();
            _vm.ChatItems.CollectionChanged += (_, e) =>
            {
                if (e.NewItems is not null)
                    foreach (ChatItem item in e.NewItems)
                        item.PropertyChanged += OnItemChanged;
                Dispatcher.UIThread.Post(StickToBottom);
            };
        }
        _vm.PropertyChanged += (_, e) =>
        {
            if (e.PropertyName == nameof(MainViewModel.IsBusy))
                Dispatcher.UIThread.Post(StickToBottom);
        };
    }

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    // streaming text grows the transcript without CollectionChanged; follow it only at the bottom
    private void OnItemChanged(object? sender, PropertyChangedEventArgs e) => Dispatcher.UIThread.Post(StickToBottom);

    private void UpdateAtBottom()
    {
        if (_chatScroll is null)
            return;
        _atBottom = _chatScroll.Offset.Y + _chatScroll.Viewport.Height >= _chatScroll.Extent.Height - 32;
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = !_atBottom && _vm.HasMessages;
    }

    private void StickToBottom()
    {
        if (_atBottom)
            _chatScroll?.ScrollToEnd();
    }

    private void OnJumpLatestClicked(object? sender, RoutedEventArgs e)
    {
        _atBottom = true;
        _chatScroll?.ScrollToEnd();
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = false;
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key != Key.Enter || e.KeyModifiers.HasFlag(KeyModifiers.Shift))
            return;
        e.Handled = true;
        _vm.SendCommand.Execute(null);
    }

    private async void OnCopyMessage(object? sender, RoutedEventArgs e)
    {
        var text = (sender as Control)?.DataContext switch
        {
            UserMessageItem u => u.Text,
            AssistantTextItem a => a.Text,
            ToolItem t => string.Join(" ", new[] { t.Tool, t.Title }.Where(s => s.Length > 0)),
            PermissionItem p => $"{p.Title}\n{p.Detail}",
            _ => null,
        };
        if (string.IsNullOrEmpty(text) || TopLevel.GetTopLevel(this) is not { Clipboard: { } clipboard })
            return;
        await clipboard.SetTextAsync(text);
    }

    private void OnRenameClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _vm.BeginRename(session);
    }

    private void OnDeleteClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session)
            _ = _vm.DeleteSessionCommand.ExecuteAsync(session);
    }

    private void OnRenameKeyDown(object? sender, KeyEventArgs e)
    {
        if ((sender as Control)?.DataContext is not SessionItem session)
            return;
        if (e.Key == Key.Enter)
        {
            e.Handled = true;
            _ = _vm.CommitRenameCommand.ExecuteAsync(session);
        }
        else if (e.Key == Key.Escape)
        {
            e.Handled = true;
            _vm.CancelRename(session);
        }
    }

    private void OnRenameLostFocus(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is SessionItem session && session.IsEditing)
            _ = _vm.CommitRenameCommand.ExecuteAsync(session);
    }

    private async void OnBrowseClicked(object? sender, RoutedEventArgs e)
    {
        var folders = await StorageProvider.OpenFolderPickerAsync(new FolderPickerOpenOptions
        {
            Title = "Working folder for opencode",
            AllowMultiple = false,
        });
        if (folders.Count > 0 && folders[0].Path is { IsAbsoluteUri: true } path)
            _vm.Folder = path.LocalPath;
    }

    protected override void OnClosing(WindowClosingEventArgs e)
    {
        _vm.Shutdown();
        base.OnClosing(e);
    }
}
