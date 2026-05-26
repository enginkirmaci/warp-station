using System.IO;
using System.Windows;

namespace Tools.SnapIt.Common.Entities;

public class Constants
{
    public static string AppName => System.Windows.Application.ResourceAssembly.GetName().Name;
    public static string AppTitle => $"{AppName} - Window Manager";
    public static string AppVersion => $"version {System.Windows.Application.ResourceAssembly.GetName().Version}";
    public static readonly string RootFolder = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), AppName);
    public const string AppRegistryKey = "Tools.SnapIt";
    public const string MainRegion = "MainRegion";
}
