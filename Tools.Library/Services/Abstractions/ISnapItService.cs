namespace Tools.Library.Services.Abstractions;

public interface ISnapItService
{
    bool IsRunning { get; }

    event EventHandler<bool>? RunningChanged;

    Task StartAsync();
    void Stop();
}
