using System.Diagnostics;
using System.Text;
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
            Debug.WriteLine($"[DevToolsClient] Connected to pipe '{PipeName}'");
        }
        catch (TimeoutException)
        {
            lock (_lock)
            {
                _isConnected = false;
            }
            ConnectionStatusChanged?.Invoke(this, "Connection timeout");
            Debug.WriteLine($"[DevToolsClient] Connection timeout to pipe '{PipeName}'");
            throw;
        }
        catch (Exception ex)
        {
            lock (_lock)
            {
                _isConnected = false;
            }
            ConnectionStatusChanged?.Invoke(this, $"Connection failed: {ex.Message}");
            Debug.WriteLine($"[DevToolsClient] Connection failed: {ex.Message}");
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
                    Debug.WriteLine("[DevToolsClient] Pipe not connected, cannot send message");
                    return;
                }

                _pipeClient.Write(messageBytes, 0, messageBytes.Length);
                _pipeClient.Flush();
            }
            
            Debug.WriteLine($"[DevToolsClient] Sent: {message}");
        }
        catch (Exception ex)
        {
            Debug.WriteLine($"[DevToolsClient] Failed to send message: {ex.Message}");
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
        Debug.WriteLine("[DevToolsClient] Process launch not supported on this platform");
        return Task.CompletedTask;
    }

    public Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null, bool hidden = false)
    {
        Debug.WriteLine("[DevToolsClient] Process launch not supported on this platform");
        return Task.CompletedTask;
    }

    public void Dispose() { }
}
#endif