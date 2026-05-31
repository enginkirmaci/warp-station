using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Formatters;
using Tools.Library.Mvvm;
using Tools.Library.Services.Abstractions;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the EF Tools page.
/// </summary>
public partial class EFToolsPageViewModel : PageViewModelBase
{
    private readonly ISettingsService _settingsService;
    private readonly IClipboardService _clipboardService;

    [ObservableProperty]
    private string _sqlInput = string.Empty;

    [ObservableProperty]
    private string _cSharpOutput = string.Empty;

    [ObservableProperty]
    private string _repositoryOutput = string.Empty;

    [ObservableProperty]
    private string _repositoryTemplate = string.Empty;

    /// <summary>
    /// Gets the command to convert SQL to C#.
    /// </summary>
    public IRelayCommand ConvertSqlCommand { get; }

    /// <summary>
    /// Gets the command to copy C# output to clipboard.
    /// </summary>
    public IRelayCommand CopyToClipboardCommand { get; }

    /// <summary>
    /// Gets the command to copy repository output to clipboard.
    /// </summary>
    public IRelayCommand CopyRepositoryToClipboardCommand { get; }

    public EFToolsPageViewModel(ISettingsService settingsService, IClipboardService clipboardService)
    {
        _settingsService = settingsService;
        _clipboardService = clipboardService;
        ConvertSqlCommand = new RelayCommand(OnConvertSql);
        CopyToClipboardCommand = new RelayCommand(OnCopyToClipboard);
        CopyRepositoryToClipboardCommand = new RelayCommand(OnCopyRepositoryToClipboard);
        _ = OnInitializeAsync();
    }

    /// <inheritdoc/>
    public override async Task OnInitializeAsync()
    {
        var settings = await _settingsService.GetSettingsAsync();
        RepositoryTemplate = settings.EFToolsPage?.RepositoryTemplate ?? string.Empty;
    }

    private void OnConvertSql()
    {
        try
        {
            CSharpOutput = SqlToCSharpFormatter.FormatCreateTableToClass(SqlInput);
            var tableName = SqlToCSharpFormatter.GetTableName(SqlInput);
            RepositoryOutput = GenerateRepositoryCode(tableName);
        }
        catch (Exception ex)
        {
            CSharpOutput = $"Error: {ex.Message}";
        }
    }

    private void OnCopyToClipboard()
    {
        if (!string.IsNullOrEmpty(CSharpOutput))
        {
            _clipboardService.CopyText(CSharpOutput);
        }
    }

    private void OnCopyRepositoryToClipboard()
    {
        if (!string.IsNullOrEmpty(RepositoryOutput))
        {
            _clipboardService.CopyText(RepositoryOutput);
        }
    }

    private string GenerateRepositoryCode(string className)
    {
        return RepositoryTemplate.Replace("{TABLENAME}", className);
    }
}