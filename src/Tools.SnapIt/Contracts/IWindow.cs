using Tools.SnapIt.Contracts;

namespace Tools.SnapIt.Contracts;

public interface IWindow
{
    event EventHandler Loaded;

    void Show();
}
