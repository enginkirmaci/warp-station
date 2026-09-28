using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Tools.Library.Services.Abstractions;

namespace Tools.Services;

/// <summary>One ask as the popup renders it: the friendly action sentence plus the tool detail.</summary>
public sealed record OpenCodePermissionAskView(string Action, string? Detail);

/// <summary>
/// The OpenCode permission popup's state machine, bound by the main window's overlay.
/// One ask visible at a time (later asks queue behind it); every ask resolves itself
/// after a short countdown, because a pending ask blocks the run's turn — leaving it
/// open forever would pin the wand. "Allow all" answers "always" (opencode stops
/// asking for that kind for the rest of the run's session); "Reject" — and the
/// countdown — answer "reject", which denies just that tool call and lets the run
/// continue. AskAsync is safe from the event pump's thread; every state mutation
/// rides the UI thread.
/// </summary>
public partial class OpenCodePermissionService : ObservableObject, IOpenCodePermissionPrompt
{
    /// <summary>How long an ask stays up unanswered before rejecting itself.</summary>
    private const int AutoRejectSeconds = 15;

    private readonly Queue<(OpenCodePermissionRequest Request, TaskCompletionSource<bool> Reply)> _pending = new();
    private (OpenCodePermissionRequest Request, TaskCompletionSource<bool> Reply)? _current;
    private DispatcherTimer? _countdown;
    private int _secondsLeft;

    [ObservableProperty]
    private OpenCodePermissionAskView? _currentAsk;

    /// <summary>Gates the popup overlay's visibility.</summary>
    public bool HasCurrentAsk => CurrentAsk is not null;

    public string AutoRejectText => $"Rejecting automatically in {_secondsLeft} s";

    public IRelayCommand AllowAllCommand { get; }
    public IRelayCommand RejectCommand { get; }

    public OpenCodePermissionService()
    {
        AllowAllCommand = new RelayCommand(() => ResolveCurrent(allow: true));
        RejectCommand = new RelayCommand(() => ResolveCurrent(allow: false));
    }

    /// <inheritdoc/>
    public async Task<bool> AskAsync(OpenCodePermissionRequest request, CancellationToken cancellationToken)
    {
        var reply = new TaskCompletionSource<bool>(TaskCreationOptions.RunContinuationsAsynchronously);
        // The ask must release when the run dies (shutdown, repo switch) — otherwise
        // the queue would stall on a reply nobody will ever send.
        using var cancelled = cancellationToken.Register(() => reply.TrySetResult(false));
        Dispatcher.UIThread.Post(() => Enqueue(request, reply));
        return await reply.Task;
    }

    private void Enqueue(OpenCodePermissionRequest request, TaskCompletionSource<bool> reply)
    {
        if (reply.Task.IsCompleted)
        {
            // The run died before the dispatcher got here; showing a dead ask would
            // only sit in the queue until its countdown rejects nobody.
            return;
        }

        _pending.Enqueue((request, reply));
        if (_current is null)
        {
            ShowNext();
        }
    }

    private void ShowNext()
    {
        StopCountdown();
        if (!_pending.TryDequeue(out var next))
        {
            _current = null;
            CurrentAsk = null;
            return;
        }

        _current = next;
        CurrentAsk = new OpenCodePermissionAskView(DescribeAction(next.Request.Kind), next.Request.Detail);
        _secondsLeft = AutoRejectSeconds;
        OnPropertyChanged(nameof(AutoRejectText));
        _countdown ??= new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _countdown.Tick += OnCountdownTick;
        _countdown.Start();
    }

    private void ResolveCurrent(bool allow)
    {
        if (_current is not { } current)
        {
            return;
        }

        _current = null;
        current.Reply.TrySetResult(allow);
        ShowNext();
    }

    private void StopCountdown()
    {
        if (_countdown is { } countdown)
        {
            countdown.Tick -= OnCountdownTick;
            countdown.Stop();
        }
    }

    private void OnCountdownTick(object? sender, EventArgs e)
    {
        _secondsLeft--;
        if (_secondsLeft <= 0)
        {
            ResolveCurrent(allow: false);
            return;
        }

        OnPropertyChanged(nameof(AutoRejectText));
    }

    partial void OnCurrentAskChanged(OpenCodePermissionAskView? value) => OnPropertyChanged(nameof(HasCurrentAsk));

    private static string DescribeAction(string kind) => kind switch
    {
        "bash" => "OpenCode wants to run a command",
        "edit" or "write" => "OpenCode wants to edit a file",
        "webfetch" => "OpenCode wants to fetch a web page",
        "external_directory" => "OpenCode wants to access a folder outside the project",
        _ => $"OpenCode requests permission ({kind})",
    };
}
