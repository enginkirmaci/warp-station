using System.Diagnostics;
using System.Text;
using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

#if WINDOWS
public class DevToolsClient : IDevToolsClient, IDisposable
{
    private const string PipeName = "devtools-pipe";
    private const int ConnectTimeout = 3000;
    
    private System.IO.Pipes.NamedPipeClientStream? _pipeClient;
    private bool _isConnected;
    private readonly object _lock = new();

    public bool IsConnected
    {
        get
        {
            lock (_lock)
            {
                return _isConnected && _pipeClient?.IsConnected == true;
            }
        }
    }

    public event EventHandler<string>? ConnectionStatusChanged;

    public async Task ConnectAsync()
    {
        lock (_lock)
        {
            if (_isConnected && _pipeClient?.IsConnected == true)
                return;

            _pipeClient?.Dispose();
            _pipeClient = new System.IO.Pipes.NamedPipeClientStream(
                ".",
                PipeName,
                System.IO.Pipes.PipeDirection.Out,
                System.IO.Pipes.PipeOptions.Asynchronous);
        }

        try
        {
            await _pipeClient.ConnectAsync(ConnectTimeout);
            
            lock (_lock)
            {
                _isConnected = true;
            }
            
            ConnectionStatusChanged?.Invoke(this, "Connected");
            Log.Logger.Information("Connected to pipe '{PipeName}'", PipeName);
        }
        catch (TimeoutException)
        {
            lock (_lock)
            {
                _isConnected = false;
            }
            ConnectionStatusChanged?.Invoke(this, "Connection timeout");
            Log.Logger.Warning("Connection timeout to pipe '{PipeName}'", PipeName);
            throw;
        }
        catch (Exception ex)
        {
            lock (_lock)
            {
                _isConnected = false;
            }
            ConnectionStatusChanged?.Invoke(this, $"Connection failed: {ex.Message}");
            Log.Logger.Error(ex, "Connection failed");
            throw;
        }
    }

    public async Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null)
    {
        await SendProcessLaunchRequestAsync(fileName, arguments, false);
    }

    public async Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null, bool hidden = false)
    {
        if (string.IsNullOrWhiteSpace(fileName))
            return;

        // Ensure connected
        if (!IsConnected)
        {
            await ConnectAsync();
        }

        // Format: fileName|arguments|hidden
        var message = $"{fileName}|{arguments ?? ""}|{hidden}";
        var messageBytes = Encoding.UTF8.GetBytes(message);

        try
        {
            lock (_lock)
            {
                if (_pipeClient?.IsConnected != true)
                {
                    Log.Logger.Warning("Pipe not connected, cannot send message");
                    return;
                }

                _pipeClient.Write(messageBytes, 0, messageBytes.Length);
                _pipeClient.Flush();
            }
            
            Log.Logger.Debug("Sent: {Message}", message);
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Failed to send message");
            lock (_lock)
            {
                _isConnected = false;
            }
            ConnectionStatusChanged?.Invoke(this, "Disconnected");
        }
    }

    public void Dispose()
    {
        lock (_lock)
        {
            _pipeClient?.Dispose();
            _pipeClient = null;
            _isConnected = false;
        }
    }
}
#else
/// <summary>
/// Stub implementation for non-Windows platforms where named pipes are not available.
/// </summary>
public class DevToolsClient : IDevToolsClient, IDisposable
{
    public bool IsConnected => false;

    public event EventHandler<string>? ConnectionStatusChanged;

    public Task ConnectAsync()
    {
        ConnectionStatusChanged?.Invoke(this, "Named pipes not supported on this platform");
        return Task.CompletedTask;
    }

    public Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null)
    {
        Log.Logger.Warning("Process launch not supported on this platform");
        return Task.CompletedTask;
    }

    public Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null, bool hidden = false)
    {
        Log.Logger.Warning("Process launch not supported on this platform");
        return Task.CompletedTask;
    }

    public void Dispose() { }
}
#endif