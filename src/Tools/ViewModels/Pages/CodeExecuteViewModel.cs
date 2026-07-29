using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Microsoft.CodeAnalysis.CSharp.Scripting;
using Microsoft.CodeAnalysis.Scripting;
using Tools.Library.Mvvm;

namespace Tools.ViewModels.Pages;

/// <summary>
/// ViewModel for the Code Execute page.
/// </summary>
public partial class CodeExecuteViewModel : PageViewModelBase
{
    [ObservableProperty]
    private string _immediateInput = string.Empty;

    [ObservableProperty]
    private string _immediateOutput = string.Empty;

    /// <summary>
    /// Evaluates the current <see cref="ImmediateInput"/> as a C# script and writes the
    /// result (or the error message) to <see cref="ImmediateOutput"/>.
    /// </summary>
    [RelayCommand]
    private async Task ExecuteAsync()
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
