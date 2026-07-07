using System.IO;

namespace Tools.SnapIt.Entities;

public class Constants
{
	/// <summary>Per-user root folder for DevTools data: %USERPROFILE%\.devtools.</summary>
	public static readonly string UserDataRoot =
		Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".devtools");

	/// <summary>
	/// Where SnapIt reads/writes its runtime data. Lives under the per-user folder so
	/// settings/layouts survive reinstalls. Was previously inside the install directory.
	/// </summary>
	public static readonly string RootFolder = Path.Combine(UserDataRoot, "settings", "snapit");

	/// <summary>
	/// Install directory, where shipped default files (used to seed the user folder on
	/// first run) live. Was previously the runtime <c>RootFolder</c>.
	/// </summary>
	public static readonly string InstallDefaultsFolder =
		Path.Combine(AppContext.BaseDirectory, "settings", "snapit");
}
