namespace Tools.Library.Entities;

public interface IWindow
{
    event EventHandler Loaded;

    void Show();
}