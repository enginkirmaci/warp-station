using System.Text;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Extensions;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Formatters page.
/// </summary>
public partial class FormattersViewModel : PageViewModelBase
{
    private readonly IClipboardService _clipboardService;
    private readonly INotificationService _notificationService;

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

    public FormattersViewModel(IClipboardService clipboardService, INotificationService notificationService)
    {
        _clipboardService = clipboardService;
        _notificationService = notificationService;
    }

    /// <summary>
    /// Converts each non-empty line of <see cref="InputText"/> under the selected
    /// transformation, prepending the results to <see cref="History"/> (newest first),
    /// then clears the input. The history is bounded to <see cref="MaxHistory"/> entries.
    /// </summary>
    [RelayCommand]
    private void Convert()
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
            return System.Convert.ToBase64String(Encoding.UTF8.GetBytes(line));
        }

        if (IsBase64DecodeSelected)
        {
            try
            {
                return Encoding.UTF8.GetString(System.Convert.FromBase64String(line));
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

    /// <summary>
    /// Copies <paramref name="text"/> to the clipboard and toasts a confirmation.
    /// </summary>
    [RelayCommand]
    private void CopyToClipboard(string? text)
    {
        if (!string.IsNullOrEmpty(text))
        {
            _clipboardService.CopyText(text);
            _notificationService.Show("Copied to clipboard", NotificationKind.Success);
        }
    }

    /// <summary>Clears the conversion history.</summary>
    [RelayCommand]
    private void ClearHistory()
    {
        History.Clear();
    }
}
