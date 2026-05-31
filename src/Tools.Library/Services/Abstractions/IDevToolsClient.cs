namespace Tools.Library.Services.Abstractions;

public interface IDevToolsClient
{
    Task ConnectAsync();
    Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null);
    Task SendProcessLaunchRequestAsync(string fileName, string? arguments = null, bool hidden = false);
    bool IsConnected { get; }
    event EventHandler<string>? ConnectionStatusChanged;
}