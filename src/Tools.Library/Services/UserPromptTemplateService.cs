using System.IO;
using Tools.Library.Configuration;

namespace Tools.Library.Services;

/// <summary>
/// Shared mechanics of the wand prompt templates: each template lives in the user's
/// opencode folder (<c>~/.devtools/opencode/&lt;file name&gt;</c>), seeded once from the
/// shipped default so upgrades never clobber user edits, with the built-in constant as
/// the last resort (written back so the editable file always materializes). Re-read on
/// every generation — no caching — so template edits take effect immediately.
/// </summary>
public abstract class UserPromptTemplateService
{
    private readonly string _userFilePath;
    private readonly string _legacyFilePath;
    private readonly string _shippedRelPath;
    private readonly string _defaultTemplate;

    protected UserPromptTemplateService(string fileName, string shippedRelPath, string defaultTemplate)
    {
        _userFilePath = UserPaths.GetUserDataFile("opencode", fileName);
        _legacyFilePath = UserPaths.GetUserDataFile("settings", fileName);
        _shippedRelPath = shippedRelPath;
        _defaultTemplate = defaultTemplate;
    }

    /// <summary>
    /// Resolves the template text: the user's file wins; the shipped default seeds it;
    /// the built-in constant is the last resort (and writes itself back so the editable
    /// file exists). Read errors fall back to the built-in constant.
    /// </summary>
    protected string LoadTemplate()
    {
        try
        {
            MigrateLegacyFile();
            UserPaths.SeedFromDefault(_userFilePath, _shippedRelPath);
            if (File.Exists(_userFilePath))
            {
                return File.ReadAllText(_userFilePath);
            }

            Directory.CreateDirectory(Path.GetDirectoryName(_userFilePath)!);
            File.WriteAllText(_userFilePath, _defaultTemplate);
        }
        catch
        {
            // A broken template location must not kill the wand — use the built-in text.
        }

        return _defaultTemplate;
    }

    /// <summary>
    /// One-time move of the template from the pre-flatten settings folder to the opencode
    /// folder, so an upgraded install keeps its edited template instead of being re-seeded
    /// from the shipped default. The legacy folder is then deleted if the move left it
    /// empty. Best-effort: any failure leaves the old file in place and the seed/fallback
    /// logic proceeds.
    /// </summary>
    private void MigrateLegacyFile()
    {
        if (File.Exists(_userFilePath) || !File.Exists(_legacyFilePath))
            return;

        Directory.CreateDirectory(Path.GetDirectoryName(_userFilePath)!);
        File.Move(_legacyFilePath, _userFilePath);
        try
        {
            // Non-recursive: only succeeds when no other pre-flatten files remain.
            Directory.Delete(Path.GetDirectoryName(_legacyFilePath)!);
        }
        catch
        {
            // Stale pre-flatten files still in there — leave them alone.
        }
    }
}
