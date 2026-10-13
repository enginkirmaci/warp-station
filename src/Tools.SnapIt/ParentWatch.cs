using System.Runtime.InteropServices;

namespace Tools.SnapIt;

/// <summary>Ties the companion's lifetime to the process that launched it (DevTools).</summary>
internal static class ParentWatch
{
	private const uint SYNCHRONIZE = 0x00100000;
	private const uint INFINITE = 0xFFFFFFFF;

	[DllImport("kernel32.dll", SetLastError = true)]
	private static extern IntPtr OpenProcess(uint dwDesiredAccess, bool bInheritHandle, int dwProcessId);

	[DllImport("kernel32.dll", SetLastError = true)]
	private static extern uint WaitForSingleObject(IntPtr hHandle, uint dwMilliseconds);

	[DllImport("kernel32.dll")]
	private static extern bool CloseHandle(IntPtr hObject);

	/// <summary>False when the parent process no longer exists.</summary>
	public static bool Start(int parentPid, Action onParentExited)
	{
		var handle = OpenProcess(SYNCHRONIZE, false, parentPid);
		if (handle == IntPtr.Zero)
		{
			return false;
		}

		var thread = new Thread(() =>
		{
			try
			{
				WaitForSingleObject(handle, INFINITE);
			}
			finally
			{
				CloseHandle(handle);
			}

			onParentExited();
		})
		{
			IsBackground = true,
			Name = "SnapItParentWatch",
		};
		thread.Start();
		return true;
	}
}
