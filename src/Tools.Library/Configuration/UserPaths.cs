using System.IO;

namespace Tools.Library.Configuration;

/// <summary>
/// Resolves the locations of user-writable data files.
/// <para>
/// Runtime/user data lives under <c>%USERPROFILE%\.devtools</c> so it survives
/// reinstalls and upgrades — the installer only refreshes code and shipped
/// defaults inside the install directory. The shipped files under the install
/// directory are kept as one-time seed sources: on first run (or the first run
/// after an upgrade from a version that stored settings inside the install dir)
/// they are copied into the user folder if no user copy exists yet.
/// </para>
/// </summary>
public static class UserPaths
{
    /// <summary>Per-user root folder for all DevTools data: <c>%USERPROFILE%\.devtools</c>.</summary>
    public static readonly string UserDataRoot =
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".devtools");

    /// <summary>Install directory (where the executable and shipped default files live).</summary>
    public static readonly string InstallRoot = AppContext.BaseDirectory;

    /// <summary>Shipped default files live under <c>&lt;install&gt;/settings</c>.</summary>
    private const string ShippedSettingsFolder = "settings";

    /// <summary>
    /// Resolves a user data file path under <see cref="UserDataRoot"/>. Does not create
    /// the file or its directory; callers create directories as needed.
    /// </summary>
    public static string GetUserDataFile(params string[] rel)
    {
        var segments = new string[rel.Length + 1];
        segments[0] = UserDataRoot;
        Array.Copy(rel, 0, segments, 1, rel.Length);
        return Path.Combine(segments);
    }

    /// <summary>
    /// One-time seed/migration: if <paramref name="userFile"/> does not exist, copy it
    /// from the shipped default at <c>&lt;install&gt;/settings/<paramref name="shippedRelPath"/></c>
    /// when that default exists. Safe to call on every load. Returns true when a seed
    /// copy was performed. Errors are swallowed (best-effort) so a missing default never
    /// blocks startup — callers fall back to their own defaults.
    /// </summary>
    public static bool SeedFromDefault(string userFile, string shippedRelPath)
    {
        if (File.Exists(userFile))
            return false;

        try
        {
            var shippedFile = Path.Combine(InstallRoot, ShippedSettingsFolder, shippedRelPath);
            if (!File.Exists(shippedFile))
                return false;

            Directory.CreateDirectory(Path.GetDirectoryName(userFile)!);
            File.Copy(shippedFile, userFile);
            return true;
        }
        catch
        {
            return false;
        }
    }
}
