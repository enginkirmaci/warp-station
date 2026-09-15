using System.Collections.Concurrent;
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
    /// One write gate per target path. Services serialize their in-memory adoption
    /// under their own locks but perform the disk write AFTER releasing them, so two
    /// writers of the same file can overlap; without this gate they also shared a
    /// fixed temp path and could interleave the temp write or fail the replace.
    /// Entries are bounded (one per config file the app writes).
    /// </summary>
    private static readonly ConcurrentDictionary<string, SemaphoreSlim> WriteGates = new(StringComparer.Ordinal);

    /// <summary>
    /// Writes <paramref name="contents"/> to <paramref name="path"/> atomically and
    /// serialized per path: writers queue on a per-target gate, write a unique temp
    /// file in the same directory, then replace the target. The gate keeps the
    /// last-adopted writer last on disk; the unique temp name removes collisions
    /// between gate holders across processes and any leftover temp from a crash.
    /// <c>File.Replace</c>/<c>File.Move</c> is atomic on the same volume (POSIX
    /// rename / Win32 ReplaceFile semantics), so a crash during the write leaves the
    /// previous file intact rather than a truncated or partial one. Creates the
    /// target directory when missing.
    /// </summary>
    public static async Task WriteAtomicallyAsync(string path, string contents)
    {
        var directory = Path.GetDirectoryName(path);
        if (!string.IsNullOrEmpty(directory))
            Directory.CreateDirectory(directory);

        var gate = WriteGates.GetOrAdd(path, _ => new SemaphoreSlim(1, 1));
        await gate.WaitAsync();
        try
        {
            var tempPath = path + "." + Guid.NewGuid().ToString("N") + ".tmp";

            await File.WriteAllTextAsync(tempPath, contents);

            // File.Move with overwrite is atomic on the same volume (POSIX rename /
            // Win32 ReplaceFile semantics), preventing a partial write from corrupting
            // the file.
            if (File.Exists(path))
                File.Replace(tempPath, path, destinationBackupFileName: null);
            else
                File.Move(tempPath, path);
        }
        finally
        {
            gate.Release();
        }
    }
}
