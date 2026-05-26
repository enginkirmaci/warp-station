using Tools.SnapIt.Common.Contracts;

namespace Tools.SnapIt.Application.Contracts;

public interface IScreenManager : IInitialize
{
    void SetSnapManager(ISnapManager snapManager);
}