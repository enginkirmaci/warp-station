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
            try
            {
                var pipeServer = new NamedPipeServerStream(
                    PipeName,
                    PipeDirection.In,
                    MaxConnections,
                    PipeTransmissionMode.Message,
                    PipeOptions.Asynchronous);

                await pipeServer.WaitForConnectionAsync(_cts.Token);
                Log.Information("[NamedPipeServer] Client connected");

                var task = Task.Run(() => HandleClientAsync(pipeServer, _cts.Token));
                _connectionTasks.Add(task);
            }
            catch (OperationCanceledException)
            {
                break;
            }
            catch (Exception ex)
            {
                Log.Error(ex, "[NamedPipeServer] Error accepting connection");
            }
        }

        // Wait for all connection tasks to complete
        await Task.WhenAll(_connectionTasks.Where(t => !t.IsCompleted));
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