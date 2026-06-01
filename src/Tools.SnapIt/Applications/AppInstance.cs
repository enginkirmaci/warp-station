namespace Tools.SnapIt.Applications;

public class AppInstance
{
    private static Mutex mutex;

    public static bool RegisterSingleInstance()
    {
        mutex = new Mutex(true, "{FF1FFB1E-5D42-4B8F-B42A-52DA1A1964B7}");

        if (mutex.WaitOne(TimeSpan.Zero, true))
        {
            return true;
        }

        return false;
    }
}