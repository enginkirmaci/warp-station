using System.Threading;
using Avalonia;
using Avalonia.Threading;
using Microsoft.Extensions.DependencyInjection;
using Tools.SnapIt;
using Tools.SnapIt.Extensions;

if (!OperatingSystem.IsWindows())
{
	Console.Error.WriteLine("The SnapIt companion is Windows-only.");
	return 1;
}

int? parentPid = null;
for (var i = 0; i + 1 < args.Length; i++)
{
	if (args[i] == "--parent" && int.TryParse(args[i + 1], out var pid))
	{
		parentPid = pid;
	}
}

// A second companion launch while one is already running is a no-op.
using var instanceLock = new Mutex(true, @"Local\DevTools.Tools.SnapIt.Companion", out var isFirstInstance);
if (!isFirstInstance)
{
	return 0;
}

if (parentPid is int watchPid && !ParentWatch.Start(watchPid, OnParentExited))
{
	Console.Error.WriteLine($"Parent process {watchPid} not found; exiting.");
	return 0;
}

Console.CancelKeyPress += (_, eventArgs) =>
{
	eventArgs.Cancel = true;
	PostShutdown();
};

SnapItApp.Services = new ServiceCollection().AddSnapItEngine().BuildServiceProvider();

AppBuilder.Configure<SnapItApp>()
	.UsePlatformDetect()
	.StartWithClassicDesktopLifetime([], ShutdownMode.OnExplicitShutdown);

return 0;

static void PostShutdown()
{
	try
	{
		// Engine dispose closes Avalonia windows, so it must land on the UI thread.
		Dispatcher.UIThread.Post(() => SnapItApp.Current?.ShutdownEngine());
	}
	catch
	{
		Environment.Exit(0);
	}
}

static void OnParentExited() => PostShutdown();
