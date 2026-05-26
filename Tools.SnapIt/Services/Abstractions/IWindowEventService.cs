using Tools.SnapIt.Common.Contracts;

namespace Tools.SnapIt.Services.Contracts;

public interface IWindowEventService : IInitialize
{
    void StartMonitoring();
    void StopMonitoring();
    bool IsMonitoring { get; }
}
