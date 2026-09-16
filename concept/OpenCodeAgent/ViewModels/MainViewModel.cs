using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Text.Json;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using OpenCodeAgent.Models;
using OpenCodeAgent.Services;

namespace OpenCodeAgent.ViewModels;

public partial class MainViewModel : ObservableObject
{
    private const string DefaultModelEntry = "(server default)";

    private readonly OpenCodeServer _server = new();
    private OpenCodeApiClient? _api;
    private CancellationTokenSource? _events;
    private string? _sessionId;
    private bool _pendingAutoTitle;
    private readonly Dictionary<string, string> _roleByMessage = new();
    private readonly Dictionary<string, long> _createdByMessage = new();
    private readonly Dictionary<string, AssistantTextItem> _textByPart = new();
    private readonly Dictionary<string, ToolItem> _toolByPart = new();

    public ObservableCollection<object> ChatItems { get; } = [];
    public ObservableCollection<SessionItem> Sessions { get; } = [];
    public ObservableCollection<string> Models { get; } = [DefaultModelEntry];

    public bool HasMessages => ChatItems.Count > 0;
    public bool HasSessionsHint => IsServerRunning && Sessions.Count == 0;

    public MainViewModel()
    {
        ChatItems.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasMessages));
        Sessions.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasSessionsHint));
    }

    [ObservableProperty] private SessionItem? _selectedSession;
    [ObservableProperty] private string? _selectedModel = DefaultModelEntry;
    [ObservableProperty] private string _statusText = "Server stopped";
    [ObservableProperty] private bool _isBusy;
    [ObservableProperty] private bool _isServerRunning;
    [ObservableProperty] private string _folder = Environment.CurrentDirectory;
    [ObservableProperty] private string _port = "14096";
    [ObservableProperty] private string _input = "";

    partial void OnIsServerRunningChanged(bool value) => OnPropertyChanged(nameof(HasSessionsHint));

    [RelayCommand]
    private async Task StartAsync()
    {
        if (IsServerRunning)
            return;
        if (!int.TryParse(Port.Trim(), out var port) || port is < 1 or > 65535)
        {
            StatusText = "Invalid port";
            return;
        }

        StatusText = "Starting opencode serve…";
        try
        {
            await _server.StartAsync("opencode", Folder, port, s => Debug.WriteLine($"[opencode] {s}"), CancellationToken.None);
            _api = new OpenCodeApiClient(_server.BaseUrl);
            IsServerRunning = true;
            StatusText = (_server.Spawned ? "Server ready — " : "Attached — ") + _server.BaseUrl;
            _events = new CancellationTokenSource();
            _ = PumpEventsAsync(_events.Token);
            _ = LoadModelsAsync();
            await RefreshSessionsAsync();
        }
        catch (Exception ex)
        {
            StatusText = "Start failed";
            AddNote($"Failed to start: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private void Stop()
    {
        _events?.Cancel();
        _events = null;
        var wasSpawned = _server.Spawned;
        var wasAttached = !wasSpawned && _api is not null;
        // an attached server is not ours to kill; disconnect only
        _server.Stop();
        IsServerRunning = false;
        IsBusy = false;
        StatusText = "Server stopped";
        Sessions.Clear();
        SelectedSession = null;
        _api = null;
        _sessionId = null;
        if (wasAttached)
            AddNote("Disconnected (the attached server is still running).");
    }

    public void Shutdown() => Stop();

    partial void OnSelectedSessionChanged(SessionItem? value)
    {
        if (value is null || value.Id == _sessionId)
            return;
        _ = OpenSessionCommand.ExecuteAsync(value);
    }

    [RelayCommand]
    private async Task OpenSessionAsync(SessionItem? session)
    {
        if (session is null || session.Id == _sessionId)
            return;
        if (IsBusy)
        {
            AddNote("Stop the running turn before switching chats.", error: true);
            SnapSelection();
            return;
        }
        if (_api is null)
        {
            SnapSelection();
            return;
        }

        try
        {
            var messages = await _api.GetMessagesAsync(session.Id, CancellationToken.None);
            _sessionId = session.Id;
            _pendingAutoTitle = false;
            ClearTranscript();
            Restore(messages);
            SetActive(session);
        }
        catch (Exception ex)
        {
            AddNote($"Could not load chat: {ex.Message}", error: true);
            SnapSelection();
        }
    }

    [RelayCommand]
    private async Task SendAsync()
    {
        var text = Input.Trim();
        if (text.Length == 0 || !IsServerRunning || _api is null || IsBusy)
            return;

        try
        {
            await EnsureSessionAsync();
        }
        catch (Exception ex)
        {
            AddNote($"Could not create a session: {ex.Message}", error: true);
            return;
        }

        Input = "";
        ChatItems.Add(new UserMessageItem(text));
        IsBusy = true;
        AutoTitle(text);
        var model = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        _ = Task.Run(async () =>
        {
            try
            {
                await _api!.SendMessageAsync(_sessionId!, text, model, CancellationToken.None);
            }
            catch (Exception ex)
            {
                PostNote($"Message failed: {ex.Message}", error: true);
            }
        });
    }

    [RelayCommand]
    private async Task AbortAsync()
    {
        if (_api is null || _sessionId is null || !IsBusy)
            return;
        try
        {
            await _api.AbortAsync(_sessionId, CancellationToken.None);
            IsBusy = false;
        }
        catch (Exception ex)
        {
            AddNote($"Abort failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private void NewChat()
    {
        if (IsBusy)
        {
            AddNote("Stop the running turn before starting a new chat.", error: true);
            return;
        }
        _sessionId = null;
        _pendingAutoTitle = false;
        ClearTranscript();
        foreach (var s in Sessions)
            s.IsActive = false;
        SelectedSession = null;
    }

    public void BeginRename(SessionItem session)
    {
        session.DraftTitle = session.Title;
        session.IsEditing = true;
    }

    public void CancelRename(SessionItem session) => session.IsEditing = false;

    [RelayCommand]
    private async Task CommitRenameAsync(SessionItem? session)
    {
        if (session is null)
            return;
        session.IsEditing = false;
        var title = session.DraftTitle.Trim();
        if (title.Length == 0 || title == session.Title || _api is null)
            return;
        try
        {
            var info = await _api.RenameSessionAsync(session.Id, title, CancellationToken.None);
            session.Title = info.Title.Length > 0 ? info.Title : title;
            session.Updated = info.UpdatedAt;
            SortSessions();
        }
        catch (Exception ex)
        {
            AddNote($"Rename failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private async Task DeleteSessionAsync(SessionItem? session)
    {
        if (session is null || _api is null)
            return;
        if (IsBusy && session.Id == _sessionId)
        {
            AddNote("Stop the running turn before deleting this chat.", error: true);
            return;
        }
        try
        {
            await _api.DeleteSessionAsync(session.Id, CancellationToken.None);
        }
        catch (Exception ex)
        {
            AddNote($"Delete failed: {ex.Message}", error: true);
            return;
        }

        var wasActive = session.Id == _sessionId;
        Sessions.Remove(session);
        if (wasActive)
        {
            _sessionId = null;
            _pendingAutoTitle = false;
            ClearTranscript();
            SelectedSession = null;
        }
    }

    private async Task EnsureSessionAsync()
    {
        if (_sessionId is not null || _api is null)
            return;
        var info = await _api.CreateSessionAsync("New chat", CancellationToken.None);
        _sessionId = info.Id;
        _pendingAutoTitle = true;
        foreach (var s in Sessions)
            s.IsActive = false;
        var item = new SessionItem(info.Id, info.Title, info.UpdatedAt) { IsActive = true };
        Sessions.Insert(0, item);
        SelectedSession = item;
    }

    private void AutoTitle(string text)
    {
        if (!_pendingAutoTitle || _sessionId is not { } id)
            return;
        _pendingAutoTitle = false;
        var title = DeriveTitle(text);
        var api = _api!;
        _ = Task.Run(async () =>
        {
            SessionInfo info;
            try
            {
                info = await api.RenameSessionAsync(id, title, CancellationToken.None);
            }
            catch
            {
                return;
            }
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                var item = Sessions.FirstOrDefault(s => s.Id == id);
                if (item is null)
                    return;
                item.Title = info.Title.Length > 0 ? info.Title : title;
                item.Updated = info.UpdatedAt;
                SortSessions();
            });
        });
    }

    private static string DeriveTitle(string text)
    {
        var firstLine = text.Split('\n', 2)[0].Trim();
        foreach (var extra in new[] { "\r", "\t" })
            firstLine = firstLine.Replace(extra, " ");
        while (firstLine.Contains("  "))
            firstLine = firstLine.Replace("  ", " ");
        return firstLine.Length <= 48 ? firstLine : firstLine[..48].TrimEnd() + "…";
    }

    private async Task RefreshSessionsAsync()
    {
        if (_api is null)
            return;
        try
        {
            var sessions = await _api.ListSessionsAsync(CancellationToken.None);
            var folder = NormalizeDir(Folder);
            // the server lists every session of the instance; keep the working folder's chats
            var mine = sessions
                .Where(s => s.Directory is null || NormalizeDir(s.Directory) == folder)
                .OrderByDescending(s => s.UpdatedAt)
                .ToList();
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                Sessions.Clear();
                foreach (var s in mine)
                    Sessions.Add(new SessionItem(s.Id, s.Title, s.UpdatedAt));
                if (_sessionId is { } active)
                {
                    var match = Sessions.FirstOrDefault(s => s.Id == active);
                    if (match is not null)
                        match.IsActive = true;
                }
                SnapSelection();
            });
        }
        catch
        {
            // the sidebar is optional; chat still works without it
        }
    }

    private void SortSessions()
    {
        var ordered = Sessions.OrderByDescending(s => s.Updated).ToList();
        for (var i = 0; i < ordered.Count; i++)
        {
            var index = Sessions.IndexOf(ordered[i]);
            if (index != i)
                Sessions.Move(index, i);
        }
    }

    private void SnapSelection()
    {
        var match = _sessionId is null ? null : Sessions.FirstOrDefault(s => s.Id == _sessionId);
        if (!ReferenceEquals(SelectedSession, match))
            SelectedSession = match;
    }

    private void SetActive(SessionItem session)
    {
        foreach (var s in Sessions)
            s.IsActive = ReferenceEquals(s, session);
    }

    private static string NormalizeDir(string path)
    {
        try
        {
            return Path.GetFullPath(path.Trim()).TrimEnd(Path.DirectorySeparatorChar, Path.AltDirectorySeparatorChar);
        }
        catch
        {
            return path.Trim();
        }
    }

    private async Task LoadModelsAsync()
    {
        try
        {
            var models = await _api!.GetModelsAsync(CancellationToken.None);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                Models.Clear();
                Models.Add(DefaultModelEntry);
                foreach (var m in models)
                    Models.Add(m);
                SelectedModel ??= DefaultModelEntry;
            });
        }
        catch
        {
            // the model list is optional; the server default still works
        }
    }

    private async Task PumpEventsAsync(CancellationToken ct)
    {
        try
        {
            await foreach (var evt in _api!.StreamEventsAsync(ct))
                HandleEvent(evt);
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            PostNote($"Event stream ended: {ex.Message}", error: true);
        }
    }

    private void HandleEvent(JsonElement evt)
    {
        var type = evt.TryGetProperty("type", out var t) ? t.GetString() : null;
        var data = evt.TryGetProperty("properties", out var p) ? p : evt;
        switch (type)
        {
            case "server.connected":
                PostNote("Connected to the opencode event stream.");
                break;
            case "message.updated":
                Dispatcher.UIThread.Post(() => OnMessageUpdated(data));
                break;
            case "message.part.updated":
                Dispatcher.UIThread.Post(() => OnPartUpdated(data));
                break;
            case "permission.asked":
                Dispatcher.UIThread.Post(() => OnPermissionAsked(data, v2: false));
                break;
            case "permission.v2.asked":
                Dispatcher.UIThread.Post(() => OnPermissionAsked(data, v2: true));
                break;
            case "permission.replied":
            case "permission.v2.replied":
                Dispatcher.UIThread.Post(() => OnPermissionReplied(data));
                break;
            case "session.status":
                var status = ReadStatus(data);
                Dispatcher.UIThread.Post(() =>
                {
                    if (IsOurSession(data))
                        IsBusy = status is "busy" or "retry";
                });
                break;
            case "session.idle":
                Dispatcher.UIThread.Post(() =>
                {
                    if (!IsOurSession(data))
                        return;
                    IsBusy = false;
                    _ = RefreshSessionsAsync();
                });
                break;
            case "session.error":
                var message = ReadError(data);
                PostNote(message, error: true);
                break;
        }
    }

    private void OnMessageUpdated(JsonElement data)
    {
        if (!IsOurSession(data) || !data.TryGetProperty("info", out var info))
            return;
        var id = info.TryGetProperty("id", out var i) ? i.GetString() : null;
        var role = info.TryGetProperty("role", out var r) ? r.GetString() : null;
        if (id is null || role is null)
            return;
        _roleByMessage[id] = role;
        if (info.TryGetProperty("time", out var time) && time.TryGetProperty("created", out var c) && c.TryGetInt64(out var ms))
            _createdByMessage[id] = ms;
    }

    private void OnPartUpdated(JsonElement data)
    {
        if (!IsOurSession(data) || !data.TryGetProperty("part", out var part))
            return;
        var id = part.TryGetProperty("id", out var i) ? i.GetString() : null;
        var partType = part.TryGetProperty("type", out var ty) ? ty.GetString() : null;
        if (id is null)
            return;

        // user parts are rendered locally at send time; stream echoes are skipped
        var messageId = part.TryGetProperty("messageID", out var mi) ? mi.GetString() : null;
        if (messageId is not null && _roleByMessage.TryGetValue(messageId, out var role) && role == "user")
            return;

        switch (partType)
        {
            case "text":
                if (!_textByPart.TryGetValue(id, out var textItem))
                {
                    textItem = new AssistantTextItem(CreatedAtFor(messageId));
                    _textByPart[id] = textItem;
                    ChatItems.Add(textItem);
                }
                if (part.TryGetProperty("text", out var txt))
                    textItem.Text = txt.GetString() ?? "";
                break;
            case "tool":
                if (!_toolByPart.TryGetValue(id, out var toolItem))
                {
                    var toolName = part.TryGetProperty("tool", out var tn) ? tn.GetString() ?? "tool" : "tool";
                    toolItem = new ToolItem(toolName);
                    _toolByPart[id] = toolItem;
                    ChatItems.Add(toolItem);
                }
                if (part.TryGetProperty("state", out var state))
                    ApplyToolState(state, toolItem);
                break;
        }
    }

    private void Restore(List<StoredMessage> messages)
    {
        foreach (var msg in messages)
        {
            _roleByMessage[msg.Id] = msg.Role;
            _createdByMessage[msg.Id] = msg.CreatedMs;
            var createdAt = FromMs(msg.CreatedMs);
            foreach (var part in msg.Parts)
            {
                var id = part.TryGetProperty("id", out var pid) ? pid.GetString() : null;
                var partType = part.TryGetProperty("type", out var ty) ? ty.GetString() : null;
                if (id is null || partType is null)
                    continue;
                switch (partType)
                {
                    case "text":
                        var text = part.TryGetProperty("text", out var tx) ? tx.GetString() ?? "" : "";
                        if (msg.Role == "user")
                            ChatItems.Add(new UserMessageItem(text, createdAt));
                        else
                        {
                            var item = new AssistantTextItem(createdAt) { Text = text };
                            _textByPart[id] = item;
                            ChatItems.Add(item);
                        }
                        break;
                    case "tool":
                        var toolName = part.TryGetProperty("tool", out var tn) ? tn.GetString() ?? "tool" : "tool";
                        var toolItem = new ToolItem(toolName);
                        if (part.TryGetProperty("state", out var state))
                            ApplyToolState(state, toolItem);
                        _toolByPart[id] = toolItem;
                        ChatItems.Add(toolItem);
                        break;
                }
            }
        }
    }

    private void ApplyToolState(JsonElement state, ToolItem toolItem)
    {
        if (state.TryGetProperty("status", out var s) && s.GetString() is { } status)
            toolItem.Status = status;
        if (state.TryGetProperty("title", out var ti) && ti.GetString() is { Length: > 0 } title)
            toolItem.Title = title;
    }

    private DateTime? CreatedAtFor(string? messageId) =>
        messageId is not null && _createdByMessage.TryGetValue(messageId, out var ms) && ms > 0
            ? FromMs(ms)
            : null;

    private static DateTime? FromMs(long ms) =>
        ms > 0 ? DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().DateTime : null;

    private void OnPermissionAsked(JsonElement data, bool v2)
    {
        var id = data.TryGetProperty("id", out var i) ? i.GetString() : null;
        var sessionId = data.TryGetProperty("sessionID", out var s) ? s.GetString() : null;
        if (id is null || sessionId is null || !IsOurSession(data))
            return;

        string kind, detail;
        if (v2)
        {
            kind = data.TryGetProperty("action", out var a) ? a.GetString() ?? "permission" : "permission";
            detail = data.TryGetProperty("resources", out var r) && r.ValueKind == JsonValueKind.Array
                ? string.Join("\n", r.EnumerateArray().Select(x => x.GetString() ?? "").Where(x => x.Length > 0))
                : "";
        }
        else
        {
            kind = data.TryGetProperty("permission", out var k) ? k.GetString() ?? "permission" : "permission";
            detail = "";
            if (data.TryGetProperty("metadata", out var md) && md.ValueKind == JsonValueKind.Object &&
                md.TryGetProperty("command", out var c) && c.GetString() is { Length: > 0 } command)
                detail = command;
            if (detail.Length == 0 && data.TryGetProperty("patterns", out var pats) && pats.ValueKind == JsonValueKind.Array)
                detail = string.Join("\n", pats.EnumerateArray().Select(x => x.GetString() ?? "").Where(x => x.Length > 0));
        }
        if (detail.Length == 0)
            detail = "(no detail provided)";

        ChatItems.Add(new PermissionItem
        {
            Id = id,
            SessionId = sessionId,
            Kind = kind,
            Detail = detail,
            IsV2 = v2,
            Respond = RespondToPermission,
        });
    }

    private void OnPermissionReplied(JsonElement data)
    {
        var requestId = data.TryGetProperty("requestID", out var r) ? r.GetString() : null;
        if (requestId is null)
            return;
        foreach (var item in ChatItems.OfType<PermissionItem>())
            if (item.Id == requestId && item.IsPending)
                item.SetAnswer("answered elsewhere");
    }

    private async Task RespondToPermission(PermissionItem item, string response)
    {
        if (_api is null)
            return;
        // close the buttons immediately so a slow reply cannot be clicked twice
        item.SetAnswer("sending…");
        try
        {
            if (item.IsV2)
                await _api.ReplyV2Async(item.Id, response, CancellationToken.None);
            else
                await _api.RespondPermissionAsync(item.SessionId, item.Id, response, CancellationToken.None);
            item.SetAnswer(response switch
            {
                "once" => "allowed (once)",
                "always" => "always allowed",
                _ => "denied",
            });
        }
        catch (Exception ex)
        {
            item.SetAnswer($"reply failed: {ex.Message}");
        }
    }

    private bool IsOurSession(JsonElement data)
    {
        if (!data.TryGetProperty("sessionID", out var sid))
            return true;
        var value = sid.GetString();
        // a fresh new chat owns no session yet; stray events from other chats must not land there
        return value is null || value == _sessionId;
    }

    private static string ReadStatus(JsonElement data)
    {
        if (!data.TryGetProperty("status", out var status))
            return "";
        if (status.ValueKind == JsonValueKind.String)
            return status.GetString() ?? "";
        return status.ValueKind == JsonValueKind.Object && status.TryGetProperty("type", out var type)
            ? type.GetString() ?? ""
            : "";
    }

    private static string ReadError(JsonElement data)
    {
        if (data.TryGetProperty("error", out var error) && error.ValueKind == JsonValueKind.Object)
        {
            var name = error.TryGetProperty("name", out var n) ? n.GetString() : null;
            var message = error.TryGetProperty("message", out var m) ? m.GetString() : null;
            if (name is { } n2 && n2.Contains("Abort", StringComparison.OrdinalIgnoreCase))
                return "Turn stopped.";
            if (name is not null || message is not null)
                return $"Error: {name ?? "opencode"}{(message is { Length: > 0 } ? $" — {message}" : "")}";
        }
        return "Session error.";
    }

    private void ClearTranscript()
    {
        _roleByMessage.Clear();
        _createdByMessage.Clear();
        _textByPart.Clear();
        _toolByPart.Clear();
        ChatItems.Clear();
    }

    private void AddNote(string text, bool error = false) => ChatItems.Add(new SystemNoteItem(text, error));

    private void PostNote(string text, bool error = false) => Dispatcher.UIThread.Post(() => AddNote(text, error));
}
