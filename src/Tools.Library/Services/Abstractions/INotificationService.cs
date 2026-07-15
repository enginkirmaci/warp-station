using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Tools.Library.Services.Abstractions;

/// <summary>
/// The severity/category of a toast notification. Drives the accent color of the toast.
/// </summary>
public enum NotificationKind
{
    Info,
    Success,
    Warning,
    Error
}

/// <summary>
/// A single toast notification shown by the host overlay. Observable so the UI can react to
/// removal. Each toast auto-dismisses after a short delay (see <see cref="INotificationService"/>).
/// </summary>
public partial class ToastMessage : ObservableObject
{
    /// <summary>Unique id used to correlate dismissal.</summary>
    public Guid Id { get; } = Guid.NewGuid();

    [ObservableProperty]
    private string _title = string.Empty;

    [ObservableProperty]
    private string _message = string.Empty;

    [ObservableProperty]
    private NotificationKind _kind = NotificationKind.Info;

    public ToastMessage(string title, string message, NotificationKind kind)
    {
        _title = title;
        _message = message;
        _kind = kind;
    }
}

/// <summary>
/// Shows transient, auto-dismissing toast notifications (e.g. "copied to clipboard", errors).
/// Implementations expose the live <see cref="Toasts"/> collection for the host overlay to bind.
/// </summary>
public interface INotificationService
{
    /// <summary>The live toast collection, newest on top. Bound by the host overlay.</summary>
    ObservableCollection<ToastMessage> Toasts { get; }

    /// <summary>
    /// Shows a toast that auto-dismisses after a short delay.
    /// </summary>
    /// <param name="message">The message body.</param>
    /// <param name="kind">The severity/category driving the accent color.</param>
    /// <param name="title">Optional title; when null a sensible default for <paramref name="kind"/> is used.</param>
    void Show(string message, NotificationKind kind = NotificationKind.Info, string? title = null);
}
