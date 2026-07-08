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

    /// <summary>
    /// Always refreshes <paramref name="userFile"/> from the shipped default at
    /// <c>&lt;install&gt;/settings/<paramref name="shippedRelPath"/></c> when that default
    /// exists, overwriting any existing user copy. Used for files the app owns
    /// authoritatively (e.g. the OpenCode model catalog): the shipped copy is the source
    /// of truth and replaces the user copy on every load so upgrades pick up new model
    /// lists. Safe to call on every load. Best-effort: errors are swallowed so a missing
    /// default never blocks startup. Returns true when a refresh was performed.
    /// </summary>
    public static bool RefreshFromDefault(string userFile, string shippedRelPath)
    {
        try
        {
            var shippedFile = Path.Combine(InstallRoot, ShippedSettingsFolder, shippedRelPath);
            if (!File.Exists(shippedFile))
                return false;

            Directory.CreateDirectory(Path.GetDirectoryName(userFile)!);
            File.Copy(shippedFile, userFile, overwrite: true);
            return true;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>
    /// One-time seed for a folder of subfolders: for each top-level subfolder under the
    /// shipped default at <c>&lt;install&gt;/settings/<paramref name="shippedRelDirectory"/></c>,
    /// copy it (recursively) into <paramref name="userDirectory"/> only when the matching
    /// user subfolder does not exist yet. Used to seed editable folder-based resources
    /// (e.g. OpenCode templates) without clobbering user edits or user-added entries.
    /// Safe to call on every load. Best-effort: errors are swallowed so a missing default
    /// never blocks startup. Returns true when at least one subfolder was seeded.
    /// </summary>
    public static bool SeedDirectoryFromDefault(string userDirectory, string shippedRelDirectory)
    {
        try
        {
            var shippedDirectory = Path.Combine(InstallRoot, ShippedSettingsFolder, shippedRelDirectory);
            if (!Directory.Exists(shippedDirectory))
                return false;

            var seeded = false;
            foreach (var shippedSub in Directory.EnumerateDirectories(shippedDirectory))
            {
                var subName = Path.GetFileName(shippedSub);
                var userSub = Path.Combine(userDirectory, subName);
                if (Directory.Exists(userSub))
                    continue; // never overwrite user edits / user-added templates

                Directory.CreateDirectory(userDirectory);
                CopyDirectory(shippedSub, userSub);
                seeded = true;
            }
            return seeded;
        }
        catch
        {
            return false;
        }
    }

    /// <summary>Recursively copies a directory tree (contents included).</summary>
    private static void CopyDirectory(string source, string destination)
    {
        Directory.CreateDirectory(destination);
        foreach (var file in Directory.EnumerateFiles(source))
        {
            File.Copy(file, Path.Combine(destination, Path.GetFileName(file)), overwrite: true);
        }
        foreach (var dir in Directory.EnumerateDirectories(source))
        {
            CopyDirectory(dir, Path.Combine(destination, Path.GetFileName(dir)));
        }
    }
}
