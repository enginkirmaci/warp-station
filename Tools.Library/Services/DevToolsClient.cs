using System.Diagnostics;
using System.IO.Pipes;
using System.Text;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

public class DevToolsClient : IDevToolsClient, IDisposable
{
    private const string PipeName = "devtools-pipe";
    private const int ConnectTimeout = 3000;
    
    private NamedPipeClientStream? _pipeClient;
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
            _pipeClient = new NamedPipeClientStream(
                ".",
                PipeName,
                PipeDirection.Out,
                PipeOptions.Asynchronous);
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