using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.CodeAnalysis.CSharp.Scripting;
using Microsoft.CodeAnalysis.Scripting;
using Tools.Library.Mvvm;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Code Execute page.
/// </summary>
public partial class CodeExecutePageViewModel : PageViewModelBase
{
    [ObservableProperty]
    private string _immediateInput = string.Empty;

    [ObservableProperty]
    private string _immediateOutput = string.Empty;

    /// <summary>
    /// Gets the command to execute code.
    /// </summary>
    public IAsyncRelayCommand ExecuteCommand { get; }

    public CodeExecutePageViewModel()
    {
        ExecuteCommand = new AsyncRelayCommand(OnExecuteAsync);
    }

    private async Task OnExecuteAsync()
    {
        try
        {
            var result = await CSharpScript.EvaluateAsync(ImmediateInput, ScriptOptions.Default);
            ImmediateOutput = result?.ToString() ?? "Executed successfully with no result.";
        }
        catch (Exception ex)
        {
            ImmediateOutput = $"Error: {ex.Message}";
        }
    }
}