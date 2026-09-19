namespace Tools.Library.Services.Abstractions;

/// <summary>
/// One node of a repository's notes tree: a folder or a markdown file. The whole tree
/// is rebuilt on every load, so nodes carry no change notification.
/// </summary>
public sealed class NotesTreeItem
{
    public required string Name { get; init; }

    public required string FullPath { get; init; }

    public required bool IsFolder { get; init; }

    public IReadOnlyList<NotesTreeItem> Children { get; init; } = Array.Empty<NotesTreeItem>();
}

/// <summary>One keyword-search match: the note, where it sits inside the repo's notes
/// folder, and a one-line excerpt around the matched content (empty for filename-only
/// matches).</summary>
public sealed record NotesSearchHit(string FullPath, string Name, string RelativePath, string Excerpt);

/// <summary>
/// Filesystem access for the per-repository markdown notes store. All paths the
/// service hands out live under the repo's notes root; mutating calls guard against
/// paths escaping it.
/// </summary>
public interface INotesService
{
    /// <summary>The store root: the configured path when non-empty, else
    /// <c>&lt;UserDataRoot&gt;/notes</c>. No directory is created.</summary>
    string ResolveStoreRoot(string? configuredPath);

    /// <summary>The notes folder of one repository: <c>&lt;storeRoot&gt;/&lt;repoName&gt;</c>
    /// (the repo's own folder name; two same-named repos share a notes folder).</summary>
    string GetRepoNotesRoot(string storeRoot, string repoName);

    /// <summary>Loads the notes tree under <paramref name="repoNotesRoot"/>: folders and
    /// <c>.md</c> files only, dot-entries skipped, folders before files. Missing root
    /// yields an empty list.</summary>
    Task<IReadOnlyList<NotesTreeItem>> LoadTreeAsync(string repoNotesRoot, CancellationToken ct = default);

    Task<string> ReadNoteAsync(string notePath, CancellationToken ct = default);

    Task WriteNoteAsync(string notePath, string content, CancellationToken ct = default);

    /// <summary>Creates <paramref name="name"/> (sanitized, <c>.md</c> enforced) in
    /// <paramref name="parentDirectory"/> (must sit under <paramref name="repoNotesRoot"/>),
    /// creating directories as needed. Returns the new file path; throws when the file
    /// already exists.</summary>
    Task<string> CreateNoteAsync(string repoNotesRoot, string parentDirectory, string name, CancellationToken ct = default);

    /// <summary>Creates a subfolder named <paramref name="name"/> (sanitized) in
    /// <paramref name="parentDirectory"/> (must sit under <paramref name="repoNotesRoot"/>).
    /// Returns the new folder path; throws when it already exists.</summary>
    Task<string> CreateFolderAsync(string repoNotesRoot, string parentDirectory, string name, CancellationToken ct = default);

    /// <summary>Deletes a note file or a folder (recursively). The target must sit
    /// under <paramref name="repoNotesRoot"/>.</summary>
    Task DeleteAsync(string repoNotesRoot, string targetPath, CancellationToken ct = default);

    /// <summary>Case-insensitive keyword search over note names and contents under
    /// <paramref name="repoNotesRoot"/>. Oversized files are searched by name only.</summary>
    Task<IReadOnlyList<NotesSearchHit>> SearchAsync(string repoNotesRoot, string term, CancellationToken ct = default);

    /// <summary>Strips path-invalid characters and trims to a usable entry name; null
    /// when nothing usable remains. With <paramref name="enforceMarkdownExtension"/> a
    /// missing <c>.md</c> extension is appended.</summary>
    string? SanitizeEntryName(string name, bool enforceMarkdownExtension);
}
