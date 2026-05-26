using System.Windows;

namespace Tools.SnapIt.Contracts;

public interface IWindow
{
    event RoutedEventHandler Loaded;

    void Show();
}