namespace Tools.Library.Entities;

/// <summary>
/// The selected repo's working-tree changes split by staging area, parsed from
/// <c>git status --porcelain=v2</c> for the bottom bar's Changes tab: every file's
/// index status becomes one <see cref="Entities.GitChangedFile"/> in
/// <see cref="Staged"/> and its worktree status one in <see cref="Unstaged"/>
/// (a file changed on both sides appears in both lists). Untracked files are
/// unstaged entries with the "?" code; line counts are per side, so the same
/// file carries different numbers in each list.
/// </summary>
public sealed record GitChangeGroups(
    IReadOnlyList<GitChangedFile> Staged,
    IReadOnlyList<GitChangedFile> Unstaged);
