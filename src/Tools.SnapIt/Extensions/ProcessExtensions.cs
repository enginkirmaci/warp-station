using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Management;

namespace Tools.SnapIt.Extensions;

public static class ProcessExtensions
{
    public static IList<Process> GetChildProcesses(this Process process)
    {
        using var searcher = new ManagementObjectSearcher(
                $"Select * From Win32_Process Where ParentProcessID={process.Id}");
        using var results = searcher.Get();
        return results.Cast<ManagementObject>()
            .Select(mo =>
                Process.GetProcessById(Convert.ToInt32(mo["ProcessID"])))
            .ToList();
    }

    public static string ProcessExecutablePath(this Process process)
    {
        try
        {
            return process.MainModule.FileName;
        }
        catch
        {
            using var searcher = new ManagementObjectSearcher("SELECT ExecutablePath, ProcessID FROM Win32_Process");
            using var results = searcher.Get();
            foreach (ManagementObject item in results)
            {
                try
                {
                    object id = item["ProcessID"];
                    object path = item["ExecutablePath"];

                    if (path != null && id.ToString() == process.Id.ToString())
                    {
                        return path.ToString();
                    }
                }
                finally
                {
                    ((IDisposable)item).Dispose();
                }
            }
        }

        return "";
    }
}