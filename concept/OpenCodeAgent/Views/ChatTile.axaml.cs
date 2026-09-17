using System.Diagnostics;
using System.IO;
using Avalonia.Controls;
using Avalonia.Controls.Primitives;
using Avalonia.Input;
using Avalonia.Input.Platform;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Media.Imaging;
using Avalonia.Platform.Storage;
using Avalonia.VisualTree;
using OpenCodeAgent.Models;
using OpenCodeAgent.Services;
using OpenCodeAgent.ViewModels;
using SukiUI.Controls;

namespace OpenCodeAgent.Views;

public partial class ChatTile : UserControl
{
    private ScrollViewer? _chatScroll;
    private Button? _jumpLatest;
    private bool _atBottom = true;

    public ChatTile()
    {
        InitializeComponent();
        _chatScroll = this.FindControl<ScrollViewer>("ChatScroll");
        _jumpLatest = this.FindControl<Button>("JumpLatest");
        var input = this.FindControl<TextBox>("InputBox");
        // an AcceptsReturn TextBox consumes Enter before bubbling; tunnel to send first
        input?.AddHandler(InputElement.KeyDownEvent, OnInputKeyDown, RoutingStrategies.Tunnel);
        input?.GotFocus += (_, _) => Vm?.Focus();
        if (_chatScroll is not null)
            _chatScroll.ScrollChanged += OnChatScrollChanged;
    }

    private ChatTileViewModel? Vm => DataContext as ChatTileViewModel;

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

    // ScrollChanged carries the post-layout extent, so pinning is decided entirely here:
    // extent growth under a pinned view is streaming content arriving, never a scroll-up.
    private void OnChatScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (_chatScroll is null || Vm is not { } vm)
            return;
        if (Math.Abs(e.OffsetDelta.Y) > 0.5)
        {
            // real offset moves: user scrolling, or the re-stick below landing at the bottom
            _atBottom = _chatScroll.Offset.Y + _chatScroll.Viewport.Height >= _chatScroll.Extent.Height - 1;
        }
        else if (_atBottom && e.ExtentDelta.Y > 0.5)
        {
            _chatScroll.ScrollToEnd();
        }
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = !_atBottom && vm.HasMessages;
    }

    private void OnJumpLatestClicked(object? sender, RoutedEventArgs e)
    {
        _atBottom = true;
        _chatScroll?.ScrollToEnd();
        if (_jumpLatest is not null)
            _jumpLatest.IsVisible = false;
    }

    private void OnTileTapped(object? sender, TappedEventArgs e) => Vm?.Focus();

    private void OnExampleClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { Content: string prompt })
            return;
        Vm?.LoadIntoInput(prompt);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    private void OnInputKeyDown(object? sender, KeyEventArgs e)
    {
        if (e.Key == Key.V && e.KeyModifiers.HasFlag(KeyModifiers.Control))
        {
            // let the TextBox's own text paste run; an image clipboard no-ops in it
            _ = PasteFromClipboardAsync();
            return;
        }
        if (e.Key != Key.Enter || e.KeyModifiers.HasFlag(KeyModifiers.Shift))
            return;
        e.Handled = true;
        Vm?.SendCommand.Execute(null);
    }

    /// <summary>Ctrl+V: attach a clipboard image; plain text was already pasted by the TextBox itself.</summary>
    private async Task PasteFromClipboardAsync()
    {
        if (TopLevel.GetTopLevel(this)?.Clipboard is not { } clipboard)
            return;
        try
        {
            var bitmap = await clipboard.TryGetBitmapAsync();
            if (bitmap is not null)
            {
                using var ms = new MemoryStream();
                bitmap.Save(ms, new PngBitmapEncoderOptions());
                Vm?.AttachImage(ms.ToArray());
                return;
            }
            var file = await clipboard.TryGetFileAsync();
            if (file is IStorageFile storageFile && storageFile.Name is { } name &&
                Path.GetExtension(name) is { Length: > 0 } ext &&
                new[] { ".png", ".jpg", ".jpeg", ".gif", ".webp", ".bmp" }.Contains(ext, StringComparer.OrdinalIgnoreCase))
            {
                await using var stream = await storageFile.OpenReadAsync();
                using var decoded = new Bitmap(stream);
                using var png = new MemoryStream();
                decoded.Save(png, new PngBitmapEncoderOptions());
                Vm?.AttachImage(png.ToArray());
            }
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[paste] {ex.Message}");
        }
    }

    private async void OnCopyMessage(object? sender, RoutedEventArgs e)
    {
        var text = (sender as Control)?.DataContext switch
        {
            UserMessageItem u => u.Text,
            AssistantTextItem a => a.Text,
            ReasoningItem r => r.Text,
            SystemNoteItem note => note.Text,
            ToolItem t => string.Join(" ", new[] { t.Tool, t.Title }.Where(s => s.Length > 0)),
            PermissionItem p => $"{p.Title}\n{p.Detail}",
            _ => null,
        };
        if (sender is Control { DataContext: ImageItem image } && TopLevel.GetTopLevel(this) is { Clipboard: { } cb2 })
        {
            await cb2.SetBitmapAsync(image.Preview);
            return;
        }
        if (string.IsNullOrEmpty(text) || TopLevel.GetTopLevel(this) is not { Clipboard: { } clipboard })
            return;
        await clipboard.SetTextAsync(text);
    }

    // ---- file drag & drop onto the composer ----

    private void OnFileDragOver(object? sender, DragEventArgs e)
    {
        if (!e.DataTransfer.Contains(DataFormat.File))
            return;
        e.DragEffects = DragDropEffects.Copy;
        e.Handled = true;
    }

    private async void OnFileDrop(object? sender, DragEventArgs e)
    {
        if (!e.DataTransfer.Contains(DataFormat.File))
            return;
        var files = e.DataTransfer.TryGetFiles();
        if (files is null || files.Length == 0)
            return;
        e.Handled = true;
        foreach (var file in files)
            await AttachDroppedFileAsync(file);
    }

    private async Task AttachDroppedFileAsync(IStorageItem item)
    {
        if (item is not IStorageFile file || Vm is not { } vm)
            return;
        try
        {
            await using var stream = await file.OpenReadAsync();
            using var ms = new MemoryStream();
            await stream.CopyToAsync(ms);
            var data = ms.ToArray();
            if (data.Length > 512 * 1024)
            {
                vm.Announce($"{file.Name} is over 512 KB — skipped.", error: true);
                return;
            }
            vm.AttachFile(file.Name, MimeFor(Path.GetExtension(file.Name)), data);
        }
        catch (Exception ex)
        {
            vm.Announce($"Could not attach {file.Name}: {ex.Message}", error: true);
        }
    }

    private static string MimeFor(string ext) => ext.ToLowerInvariant() switch
    {
        ".png" => "image/png",
        ".jpg" or ".jpeg" => "image/jpeg",
        ".gif" => "image/gif",
        ".webp" => "image/webp",
        ".bmp" => "image/bmp",
        ".pdf" => "application/pdf",
        _ => "text/plain",
    };

    // ---- transcript + prompts flyout helpers ----

    private void OnEditMessageClick(object? sender, RoutedEventArgs e)
    {
        if ((sender as Control)?.DataContext is not UserMessageItem message)
            return;
        Vm?.LoadIntoInput(message.Text);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    private async void OnCommandClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: CommandInfo command })
            return;
        HideHostFlyout(sender as Control);
        if (Vm is { } vm)
            await vm.RunCommandAsync(command);
    }

    private void OnPromptUseClick(object? sender, RoutedEventArgs e)
    {
        if (sender is not Button { DataContext: string prompt })
            return;
        HideHostFlyout(sender as Control);
        Vm?.LoadIntoInput(prompt);
        this.FindControl<TextBox>("InputBox")?.Focus();
    }

    // x:Name fields are never assigned by the hand-written InitializeComponent; walk to the host popup instead
    private static void HideHostFlyout(Control? control)
    {
        if (control?.GetVisualAncestors().OfType<Popup>().FirstOrDefault() is { } popup)
            popup.IsOpen = false;
    }
}
