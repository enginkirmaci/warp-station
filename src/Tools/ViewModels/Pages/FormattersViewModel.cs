using System.Diagnostics;
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
    private ObservableCollection<string> _history = new();

    /// <summary>The conversion strategies offered by the page.</summary>
    private enum FormatterMode
    {
        Base64Encode,
        Base64Decode,
        SnakeCase,
        PascalCase,
    }

    /// <summary>
    /// The active conversion mode. The four radio-bound booleans below are a
    /// projection onto this single value, so exactly one strategy can ever be
    /// active (the radio group in the view keeps them mutually exclusive, and
    /// the last radio checked wins here).
    /// </summary>
    private FormatterMode _mode = FormatterMode.Base64Encode;

    public bool IsBase64EncodeSelected
    {
        get => _mode == FormatterMode.Base64Encode;
        set => SetMode(FormatterMode.Base64Encode, value);
    }

    public bool IsBase64DecodeSelected
    {
        get => _mode == FormatterMode.Base64Decode;
        set => SetMode(FormatterMode.Base64Decode, value);
    }

    public bool IsSnakeCaseSelected
    {
        get => _mode == FormatterMode.SnakeCase;
        set => SetMode(FormatterMode.SnakeCase, value);
    }

    public bool IsPascalCaseSelected
    {
        get => _mode == FormatterMode.PascalCase;
        set => SetMode(FormatterMode.PascalCase, value);
    }

    /// <summary>
    /// Selects <paramref name="mode"/> when the radio reports being checked.
    /// Unchecking (a <see langword="false"/> write) never changes the mode, which
    /// keeps the setter order-safe regardless of whether the radio group pushes
    /// the new selection before or after unchecking the previous one. Raises
    /// change notifications for all four booleans so the radio bindings stay
    /// mutually in sync when the mode is switched programmatically.
    /// </summary>
    private void SetMode(FormatterMode mode, bool isChecked)
    {
        if (!isChecked || _mode == mode)
        {
            return;
        }

        _mode = mode;
        OnPropertyChanged(nameof(IsBase64EncodeSelected));
        OnPropertyChanged(nameof(IsBase64DecodeSelected));
        OnPropertyChanged(nameof(IsSnakeCaseSelected));
        OnPropertyChanged(nameof(IsPascalCaseSelected));
    }

    /// <summary>Maximum number of items retained in <see cref="History"/>.</summary>
    private const int MaxHistory = 100;

    /// <summary>True when the history list has entries (drives its empty state).</summary>
    public bool HasHistory => History.Count > 0;

    public FormattersViewModel(IClipboardService clipboardService, INotificationService notificationService)
    {
        _clipboardService = clipboardService;
        _notificationService = notificationService;

        History.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasHistory));
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

    private string ConvertLine(string line) => _mode switch
    {
        FormatterMode.Base64Encode => Base64EncodeLine(line),
        FormatterMode.Base64Decode => Base64DecodeLine(line),
        FormatterMode.SnakeCase => ToUpperSnakeCase(line),
        FormatterMode.PascalCase => ToPascalCase(line),
        // Unreachable: _mode is only ever assigned one of the four members above
        // (field initializer plus SetMode). Previously an unrecognized flag
        // combination silently fell through to identity; that is now impossible.
        _ => throw new UnreachableException($"Undefined formatter mode: {_mode}"),
    };

    private static string Base64EncodeLine(string line) =>
        System.Convert.ToBase64String(Encoding.UTF8.GetBytes(line));

    private static string Base64DecodeLine(string line)
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

    private static string ToUpperSnakeCase(string line) => line.ToSnakeCase().ToUpperInvariant();

    private static string ToPascalCase(string line) => line.ToPascalCase();

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
