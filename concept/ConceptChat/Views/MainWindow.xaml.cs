using Avalonia.Controls;
using Avalonia.Interactivity;
using Avalonia.Markup.Xaml;
using Avalonia.Platform.Storage;
using Avalonia.Threading;
using ConceptChat.Models;
using ConceptChat.ViewModels;
using SukiUI.Controls;

namespace ConceptChat.Views;

public partial class MainWindow : SukiWindow
{
    private readonly MainViewModel _vm = new();
    private ScrollViewer? _chatScroll;

    public MainWindow()
    {
        InitializeComponent();
        DataContext = _vm;
        _chatScroll = this.FindControl<ScrollViewer>("ChatScroll");
        _vm.ChatItems.CollectionChanged += (_, _) =>
            Dispatcher.UIThread.Post(() => _chatScroll?.ScrollToEnd());
    }

    private void InitializeComponent() => AvaloniaXamlLoader.Load(this);

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
