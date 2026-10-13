using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Microsoft.Extensions.DependencyInjection;
using Tools.SnapIt.Contracts;

namespace Tools.SnapIt;

public class SnapItApp : Application
{
	/// <summary>Built in Program.cs before the Avalonia lifetime starts.</summary>
	public static IServiceProvider? Services { get; set; }

	public static SnapItApp? Current { get; private set; }

	private ISnapManager? snapManager;

	public SnapItApp()
	{
		Current = this;
	}

	public override void Initialize()
	{
		AvaloniaXamlLoader.Load(this);
	}

	public override void OnFrameworkInitializationCompleted()
	{
		snapManager = Services?.GetRequiredService<ISnapManager>();

		// Window creation inside WindowManager.InitializeAsync must run on the UI
		// thread — this override is the first point where that is guaranteed.
		_ = InitializeAsync();

		base.OnFrameworkInitializationCompleted();
	}

	private async Task InitializeAsync()
	{
		if (snapManager is null)
		{
			return;
		}

		try
		{
			await snapManager.InitializeAsync();
			Console.WriteLine("SnapIt companion running.");
		}
		catch (Exception ex)
		{
			Console.Error.WriteLine($"SnapIt engine failed to start: {ex}");
		}
	}

	/// <summary>
	/// Stops the engine and ends the process. Must be invoked on the UI thread —
	/// engine dispose closes Avalonia windows.
	/// </summary>
	public void ShutdownEngine()
	{
		try
		{
			snapManager?.Dispose();
		}
		catch
		{
		}

		try
		{
			if (Application.Current?.ApplicationLifetime is IControlledApplicationLifetime lifetime)
			{
				lifetime.Shutdown();
			}
		}
		catch
		{
		}

		Environment.Exit(0);
	}
}
