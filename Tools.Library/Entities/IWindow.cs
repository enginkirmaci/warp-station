namespace Tools.Library.Entities;

public interface IWindow
{
    event RoutedEventHandler Loaded;

    void Show();
}