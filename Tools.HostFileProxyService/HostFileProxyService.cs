using System.Diagnostics;
using System.Management;
using System.Net;
using System.ServiceProcess;
using System.Timers;
using Timer = System.Timers.Timer;

namespace Tools.HostFileProxyService;

public class HostFileProxyService : ServiceBase
{
    private readonly Timer _serviceTimer;
    private Process _ftpUseProcess;

    public HostFileProxyService()
    {
        _serviceTimer = new Timer();
        _serviceTimer.Elapsed += new ElapsedEventHandler(ServiceTimerElapsed);
        _serviceTimer.Interval = 60000.0;
        AppDomain.CurrentDomain.UnhandledException += new UnhandledExceptionEventHandler(CurrentDomain_UnhandledException);
    }

    private void CurrentDomain_UnhandledException(object sender, UnhandledExceptionEventArgs e)
    {
        Exception exception;
        try
        {
            exception = (Exception)e.ExceptionObject;
        }
        catch
        {
            exception = null;
        }
        if (exception == null)
            return;

        Console.WriteLine("Exception Details:" + exception.Message + exception.StackTrace);
        Console.WriteLine(exception.ToString(), "Logging Unhandled Exception:" + sender?.ToString());
    }

    protected override void OnStart(string[] args)
    {
        _serviceTimer.Start();
    }

    protected override void OnStop()
    {
        _serviceTimer.Stop();

        if (_ftpUseProcess != null)
            KillProcessAndChildrens(_ftpUseProcess.Id);
    }

    public void TestServiceTimer()
    {
        ServiceTimerElapsed(null, null);
        if (_ftpUseProcess != null)
            KillProcessAndChildrens(_ftpUseProcess.Id);
    }

    private void ServiceTimerElapsed(object sender, ElapsedEventArgs e)
    {
        _serviceTimer.Stop();
        try
        {
            var isLocal = IsLocalNetworkAsync();
            CommentUnCommentHostEntry(isLocal);
            if (_ftpUseProcess == null)
                StartMappingProcess();
        }
        catch (Exception ex)
        {
            Console.WriteLine(ex.Message);
        }
        _serviceTimer.Start();
    }

    private void StartMappingProcess()
    {
        _ftpUseProcess = Process.Start(new ProcessStartInfo(
               "C:\\Program Files\\Ferro Software\\FtpUse\\ftpuse.exe",
               "Z: enginkirmaci.synology.me I3engkir! /USER:administrator"));
    }

    private static void KillProcessAndChildrens(int pid)
    {
        ManagementObjectCollection objectCollection = new ManagementObjectSearcher("Select * From Win32_Process Where ParentProcessID=" + pid.ToString()).Get();
        try
        {
            Process processById = Process.GetProcessById(pid);
            if (!processById.HasExited)
                processById.Kill();
        }
        catch
        {
        }
        if (objectCollection == null)
            return;
        foreach (ManagementBaseObject managementBaseObject in objectCollection)
            KillProcessAndChildrens(Convert.ToInt32(managementBaseObject["ProcessID"]));
    }

    private bool IsLocalNetworkAsync()
    {
        try
        {
            using (HttpClient httpClient = new HttpClient())
            {
                HttpResponseMessage result = httpClient.GetAsync("http://<nas-lan>5:5000").Result;
                Console.WriteLine(result.StatusCode);
                return result.StatusCode == HttpStatusCode.OK;
            }
        }
        catch
        {
            return false;
        }
    }

    private void CommentUnCommentHostEntry(bool isLocal)
    {
        string path = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.System), "drivers\\etc\\hosts");
        string[] contents = File.ReadAllLines(path);
        for (int index = 0; index < contents.Length; ++index)
        {
            if (contents[index].Contains("<nas-lan>5"))
            {
                if (isLocal)
                {
                    if (contents[index].StartsWith("#"))
                    {
                        contents[index] = contents[index].Replace("#", "");
                        File.WriteAllLines(path, contents);
                    }
                }
                else if (!contents[index].StartsWith("#"))
                {
                    contents[index] = "#" + contents[index];
                    File.WriteAllLines(path, contents);
                }
            }
        }
    }
}