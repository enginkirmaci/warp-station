using System.Windows;

namespace Tools.SnapIt.Common.Contracts;

public interface IWindow
{
    event RoutedEventHandler Loaded;

    void Show();
}