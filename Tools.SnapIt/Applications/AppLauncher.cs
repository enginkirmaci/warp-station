namespace Tools.SnapIt.Applications;

public class AppLauncher
{
	public static void RunAsAdmin()
	{
		Run("runas", true);
	}

	public static bool IsAdmin(string[] startupArgs)
	{
		return startupArgs.Any(arg => arg.Contains("runas"));
	}

	public static bool BypassSingleInstance(string[] startupArgs)
	{
		return startupArgs.Any(arg => arg.Contains("nosingle"));
	}

	private static void Run(string argument = null, bool useShellExecute = false)
	{
		var info = new ProcessStartInfo
		{
			UseShellExecute = useShellExecute,
			FileName = Environment.ProcessPath // Application.ExecutablePath; // localAppDataPath + @"\microsoft\windowsapps\Tools.SnapIt.exe" // path to the appExecutionAlias
		};

		if (!string.IsNullOrEmpty(argument))
		{
			info.Verb = argument;
			info.Arguments = $"-{argument}";
		}

		Process.Start(info);
	}
}