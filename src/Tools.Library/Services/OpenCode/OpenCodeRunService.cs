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

    /// <summary>The in-flight runs' cancellation scopes, for <see cref="Stop"/>.</summary>
    private readonly object _gate = new();
    private readonly List<CancellationTokenSource> _activeRuns = new();

    /// <inheritdoc/>
    public void Stop()
    {
        lock (_gate)
        {
            foreach (var run in _activeRuns)
            {
                try { run.Cancel(); }
                catch (ObjectDisposedException) { /* the run just completed on its own */ }
            }

            _activeRuns.Clear();
        }
    }

    /// <inheritdoc/>
    public async Task<string?> RunAsync(string? executable, string? model, string prompt, CancellationToken cancellationToken = default)
    {
        // Not on the shared ProcessRunner (yet): completion is the first pipe to close
        // rather than process exit plus a full drain, and the drains ride
        // CancellationToken.None to survive teardown. The timeout now kills the tree
        // exactly like the runner does — a timed-out run's output is discarded, and a
        // left-alive Electron tree would outlive the app (the wand's generation scope
        // disposes its CTS without cancelling, so nothing else would reap it).
        var (exe, resolved) = ProcessRunner.LocateCli(executable, ReposSettings.DefaultOpenCodeExecutable, "OpenCodeRunService", warnWhenMissing: false);
        if (resolved is null)
        {
            return null;
        }

        // Every run rides a scope linked to the caller's token, and scopes register in
        // _activeRuns: Stop() can then kill runs whose owning ViewModel is a transient
        // drawer host nobody can reach at shutdown. The kill registration below fires
        // on the scope, so both a caller cancel and Stop() reap the process tree.
        using var runScope = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        lock (_gate) _activeRuns.Add(runScope);

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
            // Headless: nobody can answer a permission prompt, so any tool request
            // would be auto-rejected and the model reduced to plain text. Verified
            // against opencode 1.18.30: --dangerously-skip-permissions is a hidden
            // (undocumented) alias of --auto — the binary resolves
            // `auto || yolo || dangerously-skip-permissions` into the same switch.
            psi.ArgumentList.Add("--dangerously-skip-permissions");
            if (!string.IsNullOrWhiteSpace(model))
            {
                psi.ArgumentList.Add("--model");
                psi.ArgumentList.Add(model);
            }
            psi.ArgumentList.Add(prompt);

            // Same Electron leak guard as the model service (constant hoisted on the runner).
            psi.EnvironmentVariables.Remove(ProcessRunner.ElectronRunAsNodeVariable);

            using var process = new Process { StartInfo = psi };
            process.Start();
            process.StandardInput.Close();

            // Killing on cancel must not depend on an async continuation noticing the
            // token: the Register callback fires synchronously on whichever thread
            // cancels (app shutdown, repo switch), so the Electron child — and the
            // workers it spawns — dies even if this await never resumes again.
            using var killOnCancel = runScope.Token.Register(
                static p => { try { ((Process)p!).Kill(entireProcessTree: true); } catch { /* already exited */ } },
                process);

            // stderr must be drained even when only stdout is used: an undrained pipe
            // fills, the child blocks writing its progress, and the run "times out".
            // No token here: the drain has to survive teardown until the kill closes
            // the pipes.
            var outputTask = process.StandardOutput.ReadToEndAsync(CancellationToken.None);
            var errorTask = process.StandardError.ReadToEndAsync(CancellationToken.None);
            var completed = await Task.WhenAny(outputTask, errorTask, Task.Delay(CliTimeout, runScope.Token));
            if (completed != outputTask && completed != errorTask)
            {
                // A cancelled delay also lands here; surface it as cancellation (the
                // kill already happened via killOnCancel) instead of a timeout.
                await completed;
                // Genuine timeout: kill the whole tree, mirroring ProcessRunner. The
                // output is discarded either way, and a left-alive Electron tree would
                // pin system RAM past app exit.
                try { process.Kill(entireProcessTree: true); } catch { /* already exited */ }
                // Error, not Warning: the file sink persists Error only, and a timed-out
                // wand is a user-visible failure that needs the diagnosis trail.
                Log.Logger.Error("OpenCodeRunService: '{Exe} run' timed out after {Timeout}s", exe, CliTimeout.TotalSeconds);
                return null;
            }

            await process.WaitForExitAsync(runScope.Token);
            var output = (await outputTask).Trim();
            if (process.ExitCode != 0)
            {
                // Previously silent: a CLI refusal (bad model, auth, …) surfaced as a
                // bare null with no trace. Error level so the file sink keeps it.
                var errorTail = (await errorTask).Trim();
                Log.Logger.Error(
                    "OpenCodeRunService: '{Exe} run' exited {ExitCode}: {Stderr}",
                    exe, process.ExitCode, errorTail.Length > 500 ? errorTail[..500] : errorTail);
                return null;
            }
            return output;
        }
        catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested || runScope.IsCancellationRequested)
        {
            throw;
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "OpenCodeRunService: '{Exe} run' failed", exe);
            return null;
        }
        finally
        {
            lock (_gate) _activeRuns.Remove(runScope);
        }
    }
}
