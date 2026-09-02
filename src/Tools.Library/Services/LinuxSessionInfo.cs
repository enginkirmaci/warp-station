namespace Tools.Library.Services;

/// <summary>
/// Session-type detection shared by the Linux global-hotkey backends.
/// </summary>
internal static class LinuxSessionInfo
{
    /// <summary>
    /// True when the process inherited a Wayland session (<c>WAYLAND_DISPLAY</c> or
    /// <c>WAYLAND_SOCKET</c> set by the compositor session).
    /// </summary>
    public static bool IsWaylandSession =>
        Environment.GetEnvironmentVariable("WAYLAND_DISPLAY") is not null
        || Environment.GetEnvironmentVariable("WAYLAND_SOCKET") is not null;
}
