namespace Tools.SnapIt.Contracts;

public interface IScreenManager : IInitialize
{
    void SetSnapManager(ISnapManager snapManager);
}