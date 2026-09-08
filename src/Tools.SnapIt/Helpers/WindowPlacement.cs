using Tools.SnapIt.Graphics;

namespace Tools.SnapIt.Helpers;

/// <summary>
/// Shared window-placement math for correcting a target rectangle by the invisible
/// DWM extended-frame margins of a window (see <c>IWinApiService.GetWindowMargin</c>).
/// </summary>
public static class WindowPlacement
{
	/// <summary>
	/// Expands <paramref name="target"/> in place so that the window's visible client
	/// area (not its outer frame) lands exactly on it: half of the horizontal margin is
	/// added on each side and the full vertical margin on the bottom.
	/// Used identically by the interactive SnapIt path (<c>SnapManager.MoveWindow</c>)
	/// and the OpenCode grid launcher (<c>OpenCodeGridLauncher.MoveIntoCell</c>).
	/// </summary>
	/// <param name="target">Rectangle to correct in place.</param>
	/// <param name="outer">Outer rectangle of the window (bounds or <c>GetWindowRect</c>).</param>
	/// <param name="withMargin">Extended-frame rectangle reported for the window.</param>
	public static void ExpandByFrameMargins(Rectangle target, Rectangle outer, Rectangle withMargin)
	{
		var marginHorizontal = (outer.Width - withMargin.Width) / 2;
		target.Left -= marginHorizontal;
		target.Right += marginHorizontal;
		target.Bottom += outer.Height - withMargin.Height;
	}
}
