using System.Diagnostics;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services.OpenCode;

/// <inheritdoc cref="IOpenCodeModelService"/>
public class OpenCodeModelService : IOpenCodeModelService
{
    /// <summary>Upper bound for the <c>opencode models</c> call; a hung CLI must not stall the UI.</summary>
    private static readonly TimeSpan CliTimeout = TimeSpan.FromSeconds(15);

    /// <inheritdoc/>
    public async Task<IReadOnlyList<string>> GetModelsAsync(string? executable, CancellationToken cancellationToken = default)
    {
        var exe = string.IsNullOrWhiteSpace(executable) ? "opencode" : executable;
        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = exe,
                Arguments = "models",
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };

            using var process = new Process { StartInfo = psi };
            process.Start();

            var outputTask = process.StandardOutput.ReadToEndAsync(cancellationToken);
            var completed = await Task.WhenAny(outputTask, Task.Delay(CliTimeout, cancellationToken));
            if (completed != outputTask)
            {
                try { process.Kill(entireProcessTree: true); } catch { /* best effort */ }
                Log.Logger.Warning("OpenCodeModelService: '{Exe} models' timed out after {Timeout}s", exe, CliTimeout.TotalSeconds);
                return Array.Empty<string>();
            }

            // Model ids are printed one per line as provider/model-id; the '/' guard drops any
            // stray non-model lines (banners, warnings leaked to stdout).
            return outputTask.Result
                .Split('\n', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                .Where(line => line.Contains('/'))
                .Distinct(StringComparer.Ordinal)
                .ToList();
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeModelService: failed to list models via '{Exe} models'", exe);
            return Array.Empty<string>();
        }
    }
}
