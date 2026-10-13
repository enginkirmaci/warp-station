namespace Tools.SnapIt.Contracts;

public interface IInitialize : IDisposable
{
    public bool IsInitialized { get; }

    Task InitializeAsync();
}