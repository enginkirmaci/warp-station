using System.Collections.ObjectModel;
using System.Windows.Input;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

/// <summary>
/// Avalonia-backed implementation of <see cref="INotificationService"/>. Maintains a bounded,
/// auto-dismissing toast collection on the UI thread. Bound by the main window's toast overlay.
/// </summary>
public class NotificationService : INotificationService
{
    /// <summary>How long a toast stays visible before auto-dismissing.</summary>
    private static readonly TimeSpan ToastDuration = TimeSpan.FromSeconds(3.5);

    /// <summary>Maximum toasts kept on screen at once (oldest dropped when exceeded).</summary>
    private const int MaxVisibleToasts = 4;

    public ObservableCollection<ToastMessage> Toasts { get; } = new();

    /// <summary>
    /// Bound by the toast overlay's dismiss button. Removes the supplied toast immediately.
    /// </summary>
    public ICommand DismissCommand { get; }

    public NotificationService()
    {
        DismissCommand = new RelayCommand<ToastMessage>(Dismiss);
    }

    public void Show(string message, NotificationKind kind = NotificationKind.Info, string? title = null)
    {
        // Marshal to the UI thread: the collection is bound by the overlay and must be
        // mutated on the dispatcher. Show() may be called from any thread (e.g. background work).
        Dispatcher.UIThread.Post(() =>
        {
            var toast = new ToastMessage(title ?? DefaultTitle(kind), message, kind);
            Toasts.Insert(0, toast);

            // Cap the number of visible toasts so a burst doesn't pile up.
            while (Toasts.Count > MaxVisibleToasts)
            {
                Toasts.RemoveAt(Toasts.Count - 1);
            }

            // Schedule auto-dismissal.
            var timer = new DispatcherTimer { Interval = ToastDuration };
            timer.Tick += (_, _) =>
            {
                timer.Stop();
                Toasts.Remove(toast);
            };
            timer.Start();
        });
    }

    private void Dismiss(ToastMessage? toast)
    {
        if (toast != null)
        {
            Toasts.Remove(toast);
        }
    }

    private static string DefaultTitle(NotificationKind kind) => kind switch
    {
        NotificationKind.Success => "Success",
        NotificationKind.Warning => "Warning",
        NotificationKind.Error => "Error",
        _ => string.Empty,
    };
}

