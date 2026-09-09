namespace Tools.ViewModels.Components.BottomBar;

/// <summary>The panels the bottom bar can expand to. <see cref="None"/> has no
/// expanded panel (the bar shows nothing — the old strip is gone).</summary>
public enum BottomBarTab
{
    None = 0,
    Overview,
    Changes,
    PullRequests,
    Issues,
    Azure,
}
