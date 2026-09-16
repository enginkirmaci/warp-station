using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace OpenCodeAgent.Models;

public abstract class ChatItem : ObservableObject;

public sealed class UserMessageItem(string text, DateTime? createdAt = null) : ChatItem
{
    public string Text { get; } = text;
    public DateTime? CreatedAt { get; } = createdAt;
    public string TimeLabel => TimeLabels.Message(CreatedAt);
}

public sealed class AssistantTextItem(DateTime? createdAt = null) : ChatItem
{
    private string _text = "";

    public DateTime? CreatedAt { get; } = createdAt;

    public string Text { get => _text; set => SetProperty(ref _text, value); }

    public string TimeLabel => TimeLabels.Message(CreatedAt);
}

public sealed class ToolItem(string tool) : ChatItem
{
    private string _status = "pending";
    private string _title = tool;

    public string Tool { get; } = tool;

    public string Status { get => _status; set => SetProperty(ref _status, value); }

    public string Title { get => _title; set => SetProperty(ref _title, value); }
}

public sealed class SystemNoteItem(string text, bool isError = false) : ChatItem
{
    public string Text { get; } = text;
    public bool IsError { get; } = isError;
}

public partial class PermissionItem : ChatItem
{
    private string? _answer;

    public required string Id { get; init; }
    public required string SessionId { get; init; }
    public required string Kind { get; init; }
    public required string Detail { get; init; }
    public required bool IsV2 { get; init; }
    public required Func<PermissionItem, string, Task> Respond { get; init; }

    public string Title => $"Permission required — {Kind}";
    public bool IsPending => _answer is null;

    public string? Answer
    {
        get => _answer;
        private set
        {
            if (SetProperty(ref _answer, value))
                OnPropertyChanged(nameof(IsPending));
        }
    }

    public void SetAnswer(string label) => Answer = label;

    [RelayCommand]
    private Task Allow() => Respond(this, "once");

    [RelayCommand]
    private Task AllowAlways() => Respond(this, "always");

    [RelayCommand]
    private Task Deny() => Respond(this, "reject");
}

public static class TimeLabels
{
    public static string Message(DateTime? at)
    {
        if (at is not { } value)
            return "";
        return value.Date == DateTime.Today
            ? value.ToString("HH:mm")
            : value.ToString("dd MMM HH:mm");
    }

    public static string Relative(DateTime at)
    {
        var span = DateTime.Now - at;
        if (span.TotalMinutes < 1)
            return "now";
        if (span.TotalHours < 1)
            return $"{(int)span.TotalMinutes}m";
        if (span.TotalDays < 1)
            return $"{(int)span.TotalHours}h";
        if (span.TotalDays < 7)
            return $"{(int)span.TotalDays}d";
        return at.ToString("yyyy-MM-dd");
    }
}
