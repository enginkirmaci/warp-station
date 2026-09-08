using System.Text.Json;

namespace Tools.Library.Services;

/// <summary>
/// Shared JSON plumbing for the file-backed services: canonical
/// <see cref="JsonSerializerOptions"/> singletons and the atomic write helper.
/// Centralizing the options guarantees every config file is read and written
/// with the same shape (case-insensitive reads, indented writes).
/// </summary>
public static class JsonIO
{
    /// <summary>
    /// Shared options for deserializing config files: case-insensitive property
    /// matching so hand-edited files tolerate casing drift.
    /// </summary>
    public static readonly JsonSerializerOptions ReadOptions = new() { PropertyNameCaseInsensitive = true };

    /// <summary>
    /// Shared options for serializing config files: indented so the files stay
    /// human-readable and hand-editable.
    /// </summary>
    public static readonly JsonSerializerOptions WriteOptions = new() { WriteIndented = true };

    /// <summary>
    /// Writes <paramref name="contents"/> to <paramref name="path"/> atomically:
    /// write to a temp file (<c>path + ".tmp"</c>) in the same directory, then
    /// replace the target. <c>File.Replace</c>/<c>File.Move</c> is atomic on the
    /// same volume (POSIX rename / Win32 ReplaceFile semantics), so a crash during
    /// the write leaves the previous file intact rather than a truncated or
    /// partial one. Creates the target directory when missing.
    /// </summary>
    public static async Task WriteAtomicallyAsync(string path, string contents)
    {
        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(directory))
            Directory.CreateDirectory(directory);

        var tempPath = path + ".tmp";

        await File.WriteAllTextAsync(tempPath, contents);

        // File.Move with overwrite is atomic on the same volume (POSIX rename / Win
        // ReplaceFile semantics), preventing a partial-write from corrupting the file.
        if (File.Exists(path))
            File.Replace(tempPath, path, destinationBackupFileName: null);
        else
            File.Move(tempPath, path);
    }
}
