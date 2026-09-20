using System.Security.Cryptography;
using System.Text;
using Tools.Library.Configuration;
using Tools.Library.Services.Abstractions;

namespace Tools.Library.Services;

/// <summary>Filesystem-backed markdown notes store: one folder per repository under a
/// configurable root (see <see cref="GeneralSettings.NotesStorePath"/>). Blocking IO is
/// wrapped in <see cref="Task.Run"/> (the RepoScanner convention).</summary>
public sealed class NotesService : INotesService
{
    /// <summary>Search reads at most this many bytes of a note's content; larger files
    /// match on filename only (the editor itself always reads the whole file).</summary>
    private const long SearchContentCapBytes = 2 * 1024 * 1024;

    /// <summary>Search stops after this many notes so a stray huge store cannot stall
    /// the UI thread's follow-up work.</summary>
    private const int SearchFileCap = 2000;

    public string ResolveStoreRoot(string? configuredPath)
    {
        var trimmed = configuredPath?.Trim();
        return string.IsNullOrEmpty(trimmed)
            ? Path.Combine(UserPaths.UserDataRoot, "notes")
            : trimmed;
    }

    public string GetRepoNotesRoot(string storeRoot, string repoName)
        => Path.Combine(storeRoot, SanitizeEntryName(repoName, enforceMarkdownExtension: false) ?? "notes");

    public async Task<IReadOnlyList<NotesTreeItem>> LoadTreeAsync(string repoNotesRoot, CancellationToken ct = default)
    {
        if (!Directory.Exists(repoNotesRoot))
        {
            return Array.Empty<NotesTreeItem>();
        }

        return await Task.Run(() => LoadDirectory(repoNotesRoot, ct), ct);
    }

    public async Task<string> ReadNoteAsync(string notePath, CancellationToken ct = default)
    {
        return await Task.Run(() => File.ReadAllText(notePath, Encoding.UTF8), ct);
    }

    public async Task WriteNoteAsync(string notePath, string content, CancellationToken ct = default)
    {
        // BOM-less UTF-8: Encoding.UTF8 emits a BOM that shows up as stray bytes in
        // git diffs and other editors.
        await Task.Run(() => File.WriteAllText(notePath, content, new UTF8Encoding(encoderShouldEmitUTF8Identifier: false)), ct);
    }

    public async Task<string> CreateNoteAsync(
        string repoNotesRoot, string parentDirectory, string name, CancellationToken ct = default)
    {
        var fileName = SanitizeEntryName(name, enforceMarkdownExtension: true)
            ?? throw new ArgumentException("The note name is empty.", nameof(name));

        return await Task.Run(() =>
        {
            EnsureUnderRoot(repoNotesRoot, parentDirectory);
            Directory.CreateDirectory(parentDirectory);
            var path = Path.Combine(parentDirectory, fileName);
            using (new FileStream(path, FileMode.CreateNew, FileAccess.Write))
            {
            }

            return path;
        }, ct);
    }

    public async Task<string> CreateFolderAsync(
        string repoNotesRoot, string parentDirectory, string name, CancellationToken ct = default)
    {
        var folderName = SanitizeEntryName(name, enforceMarkdownExtension: false)
            ?? throw new ArgumentException("The folder name is empty.", nameof(name));

        return await Task.Run(() =>
        {
            EnsureUnderRoot(repoNotesRoot, parentDirectory);
            var path = Path.Combine(parentDirectory, folderName);
            if (Directory.Exists(path) || File.Exists(path))
            {
                throw new IOException($"'{folderName}' already exists.");
            }

            Directory.CreateDirectory(path);
            return path;
        }, ct);
    }

    public async Task DeleteAsync(string repoNotesRoot, string targetPath, CancellationToken ct = default)
    {
        await Task.Run(() =>
        {
            EnsureUnderRoot(repoNotesRoot, targetPath);
            if (Directory.Exists(targetPath))
            {
                Directory.Delete(targetPath, recursive: true);
            }
            else if (File.Exists(targetPath))
            {
                File.Delete(targetPath);
            }
        }, ct);
    }

    public async Task<IReadOnlyList<NotesSearchHit>> SearchAsync(
        string repoNotesRoot, string term, CancellationToken ct = default, int maxHits = int.MaxValue)
    {
        if (!Directory.Exists(repoNotesRoot) || string.IsNullOrWhiteSpace(term))
        {
            return Array.Empty<NotesSearchHit>();
        }

        var hitCap = Math.Min(maxHits, SearchFileCap);

        return await Task.Run(() =>
        {
            var hits = new List<NotesSearchHit>();
            foreach (var path in EnumerateNoteFiles(repoNotesRoot))
            {
                ct.ThrowIfCancellationRequested();
                var nameMatch = Path.GetFileName(path).Contains(term, StringComparison.OrdinalIgnoreCase);
                var excerpt = nameMatch ? string.Empty : ReadExcerpt(path, term);
                if (!nameMatch && excerpt is null)
                {
                    continue;
                }

                hits.Add(new NotesSearchHit(
                    path,
                    Path.GetFileNameWithoutExtension(path),
                    Path.GetRelativePath(repoNotesRoot, path),
                    excerpt ?? FirstContentLine(path) ?? string.Empty));
                if (hits.Count >= hitCap)
                {
                    break;
                }
            }

            return (IReadOnlyList<NotesSearchHit>)hits;
        }, ct);
    }

    public string? SanitizeEntryName(string name, bool enforceMarkdownExtension)
    {
        var invalid = Path.GetInvalidFileNameChars();
        var builder = new StringBuilder(name.Length);
        foreach (var ch in name.Trim())
        {
            builder.Append(Array.IndexOf(invalid, ch) >= 0 || ch is '/' or '\\' ? '-' : ch);
        }

        var cleaned = builder.ToString().Trim(' ', '.');
        if (cleaned.Length == 0)
        {
            return null;
        }

        return enforceMarkdownExtension && !cleaned.EndsWith(".md", StringComparison.OrdinalIgnoreCase)
            ? cleaned + ".md"
            : cleaned;
    }

    private static NotesTreeItem[] LoadDirectory(string directory, CancellationToken ct)
    {
        var folders = new List<NotesTreeItem>();
        var notes = new List<NotesTreeItem>();
        try
        {
            foreach (var entry in Directory.EnumerateDirectories(directory))
            {
                ct.ThrowIfCancellationRequested();
                var name = Path.GetFileName(entry);
                if (name.StartsWith('.'))
                {
                    continue;
                }

                try
                {
                    folders.Add(new NotesTreeItem
                    {
                        Name = name,
                        FullPath = entry,
                        IsFolder = true,
                        Children = LoadDirectory(entry, ct)
                    });
                }
                catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
                {
                    Serilog.Log.Logger.Debug(ex, "Notes tree: skipped unreadable folder {Folder}", entry);
                }
            }

            foreach (var file in Directory.EnumerateFiles(directory, "*.md"))
            {
                var name = Path.GetFileName(file);
                if (!name.StartsWith('.'))
                {
                    notes.Add(new NotesTreeItem { Name = name, FullPath = file, IsFolder = false });
                }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            Serilog.Log.Logger.Debug(ex, "Notes tree: skipped unreadable folder {Folder}", directory);
        }

        return folders
            .Concat(notes)
            .OrderBy(item => item.Name, StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private static IEnumerable<string> EnumerateNoteFiles(string root)
    {
        var folders = new Queue<string>();
        folders.Enqueue(root);
        while (folders.Count > 0)
        {
            var current = folders.Dequeue();
            string[] entries;
            try
            {
                entries = Directory.GetFileSystemEntries(current).ToArray();
            }
            catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
            {
                continue;
            }

            foreach (var entry in entries)
            {
                if (Directory.Exists(entry))
                {
                    if (!Path.GetFileName(entry).StartsWith('.'))
                    {
                        folders.Enqueue(entry);
                    }
                }
                else if (entry.EndsWith(".md", StringComparison.OrdinalIgnoreCase))
                {
                    yield return entry;
                }
            }
        }
    }

    /// <summary>The first content line containing the term, whitespace-collapsed and cut
    /// around the match; null when the content has no match (filename-only hit).</summary>
    private static string? ReadExcerpt(string path, string term)
    {
        try
        {
            var info = new FileInfo(path);
            if (info.Length > SearchContentCapBytes)
            {
                return null;
            }

            foreach (var rawLine in File.ReadLines(path))
            {
                var line = string.Join(' ', rawLine.Split(' ', StringSplitOptions.RemoveEmptyEntries));
                var index = line.IndexOf(term, StringComparison.OrdinalIgnoreCase);
                if (index < 0)
                {
                    continue;
                }

                var start = Math.Max(0, index - 40);
                var excerpt = line.Substring(start, Math.Min(160, line.Length - start));
                return (start > 0 ? "…" : string.Empty) + excerpt + (start + excerpt.Length < line.Length ? "…" : string.Empty);
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            return null;
        }

        return null;
    }

    private static string? FirstContentLine(string path)
    {
        try
        {
            foreach (var rawLine in File.ReadLines(path))
            {
                var line = string.Join(' ', rawLine.Split(' ', StringSplitOptions.RemoveEmptyEntries));
                if (line.Length > 0)
                {
                    return line.Length <= 160 ? line : line[..160] + "…";
                }
            }
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
        }

        return null;
    }

    /// <summary>Guards a user-derived path against escaping the repo's notes root (path
    /// traversal via hand-typed folder paths in settings).</summary>
    private static void EnsureUnderRoot(string root, string path)
    {
        var fullRoot = Path.GetFullPath(root);
        var fullPath = Path.GetFullPath(path);
        if (!fullPath.StartsWith(fullRoot.TrimEnd(Path.DirectorySeparatorChar) + Path.DirectorySeparatorChar, StringComparison.OrdinalIgnoreCase)
            && !string.Equals(fullPath, fullRoot, StringComparison.OrdinalIgnoreCase))
        {
            throw new InvalidOperationException($"Notes path escapes the store root: {path}");
        }
    }
}
