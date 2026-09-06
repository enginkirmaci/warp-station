using System.Diagnostics;
using Serilog;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services.OpenCode;

/// <inheritdoc cref="IOpenCodeRunService"/>
public class OpenCodeRunService : IOpenCodeRunService
{
    /// <summary>
    /// Upper bound for one <c>opencode run</c> — a real model call is much slower than
    /// the other CLIs this app drives, so this is generous; a hung session must still
    /// not pin the wand button forever.
    /// </summary>
    private static readonly TimeSpan CliTimeout = TimeSpan.FromSeconds(120);

    /// <inheritdoc/>
    public async Task<string?> RunAsync(string? executable, string? model, string prompt, CancellationToken cancellationToken = default)
    {
        var exe = string.IsNullOrWhiteSpace(executable) ? "opencode" : executable;

        // Same lookup as the model service: the GUI process runs with a minimal PATH,
        // so a bare name needs the user-level install dirs to resolve.
        var resolved = ExecutableDefaults.Locate(exe);
        if (resolved is null)
        {
            Log.Logger.Debug("OpenCodeRunService: '{Exe}' not found on PATH or in the user install folders", exe);
            return null;
        }

        try
        {
            var psi = new ProcessStartInfo
            {
                FileName = resolved,
                UseShellExecute = false,
                CreateNoWindow = true,
                WindowStyle = ProcessWindowStyle.Hidden,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                // The child must never see the host's stdin: under a debugger (or any
                // launcher keeping a live pipe open) `opencode run` waits on that pipe
                // forever instead of using the argv prompt. A closed pipe is an instant
                // EOF, same as a terminal run.
                RedirectStandardInput = true,
            };
            // ArgumentList, not an Arguments string: the prompt (a diff + instructions)
            // carries quotes and newlines no shell-style escaping should have to survive.
            psi.ArgumentList.Add("run");
            if (!string.IsNullOrWhiteSpace(model))
            {
                psi.ArgumentList.Add("--model");
                psi.ArgumentList.Add(model);
            }
            psi.ArgumentList.Add(prompt);

            // Same Electron leak guard as the model service.
            psi.EnvironmentVariables.Remove("ELECTRON_RUN_AS_NODE");

            using var process = new Process { StartInfo = psi };
            process.Start();
            process.StandardInput.Close();

            // stderr must be drained even when only stdout is used: an undrained pipe
            // fills, the child blocks writing its progress, and the run "times out".
            var outputTask = process.StandardOutput.ReadToEndAsync(cancellationToken);
            var errorTask = process.StandardError.ReadToEndAsync(cancellationToken);
            var completed = await Task.WhenAny(outputTask, errorTask, Task.Delay(CliTimeout, cancellationToken));
            if (completed != outputTask && completed != errorTask)
            {
                try { process.Kill(entireProcessTree: true); } catch { /* best effort */ }
                Log.Logger.Warning("OpenCodeRunService: '{Exe} run' timed out after {Timeout}s", exe, CliTimeout.TotalSeconds);
                return null;
            }

            await process.WaitForExitAsync(cancellationToken);
            var output = (await outputTask).Trim();
            if (process.ExitCode != 0)
            {
                // Previously silent: a CLI refusal (bad model, auth, …) surfaced as a
                // bare null with no trace.
                var errorTail = (await errorTask).Trim();
                Log.Logger.Warning(
                    "OpenCodeRunService: '{Exe} run' exited {ExitCode}: {Stderr}",
                    exe, process.ExitCode, errorTail.Length > 500 ? errorTail[..500] : errorTail);
                return null;
            }
            return output;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Logger.Warning(ex, "OpenCodeRunService: '{Exe} run' failed", exe);
            return null;
        }
    }
}
