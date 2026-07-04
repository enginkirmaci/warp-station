using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Extensions;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Formatters page.
/// </summary>
public partial class FormattersPageViewModel : PageViewModelBase
{
    private readonly IClipboardService _clipboardService;

    [ObservableProperty]
    private string _inputText = string.Empty;

    [ObservableProperty]
    private bool _isBase64EncodeSelected = true;

    [ObservableProperty]
    private bool _isBase64DecodeSelected;

    [ObservableProperty]
    private bool _isSnakeCaseSelected;

    [ObservableProperty]
    private bool _isPascalCaseSelected;

    [ObservableProperty]
    private ObservableCollection<string> _history = new();

    /// <summary>Maximum number of items retained in <see cref="History"/>.</summary>
    private const int MaxHistory = 100;

    /// <summary>
    /// Gets the command to convert text.
    /// </summary>
    public IRelayCommand ConvertCommand { get; }

    /// <summary>
    /// Gets the command to copy text to clipboard.
    /// </summary>
    public IRelayCommand<string> CopyToClipboardCommand { get; }

    /// <summary>
    /// Gets the command to clear the history.
    /// </summary>
    public IRelayCommand ClearHistoryCommand { get; }

    public FormattersPageViewModel(IClipboardService clipboardService)
    {
        _clipboardService = clipboardService;
        ConvertCommand = new RelayCommand(OnConvert);
        CopyToClipboardCommand = new RelayCommand<string>(OnCopyToClipboard);
        ClearHistoryCommand = new RelayCommand(OnClearHistory);
    }

    private void OnConvert()
    {
        if (string.IsNullOrEmpty(InputText))
        {
            return;
        }

        var lines = InputText.Split(Environment.NewLine, StringSplitOptions.RemoveEmptyEntries);
        // Insert newest-first so the latest result stays on top, while
        // preserving the original order of the converted batch.
        for (var i = lines.Length - 1; i >= 0; i--)
        {
            History.Insert(0, ConvertLine(lines[i]));
        }

        // Bound the history so it can't grow unbounded across a long session.
        while (History.Count > MaxHistory)
        {
            History.RemoveAt(History.Count - 1);
        }

        InputText = string.Empty;
    }

    private string ConvertLine(string line)
    {
        if (IsBase64EncodeSelected)
        {
            return Convert.ToBase64String(Encoding.UTF8.GetBytes(line));
        }

        if (IsBase64DecodeSelected)
        {
            try
            {
                return Encoding.UTF8.GetString(Convert.FromBase64String(line));
            }
            catch
            {
                return $"[Invalid Base64: {line}]";
            }
        }

        if (IsSnakeCaseSelected)
        {
            return line.ToSnakeCase().ToUpperInvariant();
        }

        if (IsPascalCaseSelected)
        {
            return line.ToPascalCase();
        }

        return line;
    }

    private void OnCopyToClipboard(string? text)
    {
        if (!string.IsNullOrEmpty(text))
        {
            _clipboardService.CopyText(text);
        }
    }

    private void OnClearHistory()
    {
        History.Clear();
    }
}
