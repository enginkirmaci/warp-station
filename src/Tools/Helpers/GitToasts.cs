using Serilog;
using Tools.Library.Services.Abstractions;

namespace Tools.Helpers;

/// <summary>
/// The outcome-toasting shape every git action VM helper repeats: run the service
/// call, toast the success/failure text, treat a thrown exception as a failed outcome
/// (logged and toasted — every call site discards the task or hands it to a relay
/// command, so a propagating exception would vanish unobserved). The callers keep
/// their own busy flags and settle hooks.
/// </summary>
public static class GitToasts
{
    /// <summary>
    /// Runs one git service call and toasts its outcome. <paramref name="successText"/>
    /// (null skips a toast) fires on success; <paramref name="errorText"/> (null skips a
    /// toast; evaluated after the action, so fetch/pull/push can embed git's stderr
    /// line) on failure; <paramref name="onFailure"/> runs on a false or thrown outcome.
    /// Returns whether the action succeeded.
    /// </summary>
    public static async Task<bool> RunAsync(
        INotificationService notifications,
        Func<Task<bool>> action,
        Func<string?> successText,
        Func<string?> errorText,
        Action? onFailure = null,
        string? logContext = null)
    {
        try
        {
            if (await action())
            {
                if (successText() is { } success)
                {
                    notifications.Show(success, NotificationKind.Success);
                }

                return true;
            }
        }
        catch (Exception ex)
        {
            Log.Logger.Error(ex, "Git action failed{Context}", logContext is null ? string.Empty : $" — {logContext}");
            if (errorText() is { } thrown)
            {
                notifications.Show(thrown, NotificationKind.Error);
            }

            onFailure?.Invoke();
            return false;
        }

        if (errorText() is { } error)
        {
            notifications.Show(error, NotificationKind.Error);
        }

        onFailure?.Invoke();
        return false;
    }
}
