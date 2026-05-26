using Tools.SnapIt.Contracts;

namespace Tools.SnapIt.Services.Abstractions;

public interface IWindowEventService : IInitialize
{
    void StartMonitoring();
    void StopMonitoring();
    bool IsMonitoring { get; }
}
