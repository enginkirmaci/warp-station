using System.IO.Pipes;
using System.Text;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace DevTools.Services;

public class NamedPipeServer : IDisposable
{
    private const string PipeName = "devtools-pipe";
    private const int MaxConnections = 5;
    
    private readonly IProcessLauncher _processLauncher;
    private readonly CancellationTokenSource _cts = new();
    private readonly List<Task> _connectionTasks = new();
    private bool _isRunning;

    public NamedPipeServer(IProcessLauncher processLauncher)
    {
        _processLauncher = processLauncher;
    }

    public async Task StartAsync()
    {
        if (_isRunning)
            return;

        _isRunning = true;
        Log.Information("[NamedPipeServer] Starting server on pipe '{PipeName}'", PipeName);

        _ = AcceptConnectionsAsync();
    }

    public void Stop()
    {
        _isRunning = false;
        _cts.Cancel();
        Log.Information("[NamedPipeServer] Server stopped");
    }

    private async Task AcceptConnectionsAsync()
    {
        while (_isRunning && !_cts.Token.IsCancellationRequested)
        {
            var pipeServer = CreateServerStream();

            try
            {
                await pipeServer.WaitForConnectionAsync(_cts.Token);
            }
            catch (OperationCanceledException)
            {
                pipeServer.Dispose();
                break;
            }
            catch (Exception ex)
            {
                // The stream belongs to this loop until a client connects; an accept
                // failure must not leak it.
                pipeServer.Dispose();
                Log.Error(ex, "[NamedPipeServer] Error accepting connection");
                continue;
            }

            Log.Information("[NamedPipeServer] Client connected");

            var task = Task.Run(() => HandleClientAsync(pipeServer, _cts.Token));
            _connectionTasks.Add(task);

            // Prune finished entries: the list is only joined on shutdown, and a
            // long-lived host would otherwise accumulate one dead task per client.
            _connectionTasks.RemoveAll(t => t.IsCompleted);
        }

        // Wait for all connection tasks to complete
        await Task.WhenAll(_connectionTasks.Where(t => !t.IsCompleted));
    }

    /// <summary>
    /// Creates the accept loop's server stream. On Windows the pipe is restricted to
    /// the current user's SID: the server launches whatever fileName|arguments a
    /// client submits, so an open pipe would be a local launch oracle for any other
    /// process running in the session. Tools.exe (elevated) still connects — an
    /// elevated admin token carries the launching user's SID.
    /// </summary>
    private static NamedPipeServerStream CreateServerStream()
    {
        if (OperatingSystem.IsWindows())
        {
            var security = new PipeSecurity();
            security.AddAccessRule(new PipeAccessRule(
                System.Security.Principal.WindowsIdentity.GetCurrent().User!,
                PipeAccessRights.ReadWrite,
                System.Security.AccessControl.AccessControlType.Allow));

            // The PipeSecurity-taking constructor is Windows-runtime-only;
            // NamedPipeServerStreamAcl.Create is its supported cross-target entry.
            return System.IO.Pipes.NamedPipeServerStreamAcl.Create(
                PipeName,
                PipeDirection.In,
                MaxConnections,
                PipeTransmissionMode.Message,
                PipeOptions.Asynchronous,
                inBufferSize: 0,
                outBufferSize: 0,
                security);
        }

        return new NamedPipeServerStream(
            PipeName,
            PipeDirection.In,
            MaxConnections,
            PipeTransmissionMode.Message,
            PipeOptions.Asynchronous);
    }

    private async Task HandleClientAsync(NamedPipeServerStream pipeServer, CancellationToken cancellationToken)
    {
        try
        {
            var buffer = new byte[1024];
            
            while (pipeServer.IsConnected && !cancellationToken.IsCancellationRequested)
            {
                var bytesRead = await pipeServer.ReadAsync(buffer, 0, buffer.Length, cancellationToken);
                
                if (bytesRead == 0)
                    break;

                var message = Encoding.UTF8.GetString(buffer, 0, bytesRead);
                Log.Information("[NamedPipeServer] Received: {Message}", message);

                // Parse message: fileName|arguments|hidden
                var parts = message.Split('|');
                if (parts.Length >= 1 && !string.IsNullOrWhiteSpace(parts[0]))
                {
                    var fileName = parts[0];
                    var arguments = parts.Length > 1 ? parts[1] : null;
                    var hidden = parts.Length > 2 && bool.TryParse(parts[2], out var h) && h;

                    Log.Information("[NamedPipeServer] Launching: {FileName} with args: {Arguments}, hidden: {Hidden}",
                        fileName, arguments, hidden);

                    _processLauncher.StartProcess(fileName, arguments, hidden);
                }
            }
        }
        catch (OperationCanceledException)
        {
            // Expected when cancellation is requested
        }
        catch (Exception ex)
        {
            Log.Error(ex, "[NamedPipeServer] Error handling client");
        }
        finally
        {
            pipeServer.Dispose();
            Log.Information("[NamedPipeServer] Client disconnected");
        }
    }

    public void Dispose()
    {
        Stop();
        _cts.Dispose();
    }
}