using System.Collections.ObjectModel;
using System.Diagnostics;
using System.Text.Json;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using ConceptChat.Models;
using ConceptChat.Services;

namespace ConceptChat.ViewModels;

public partial class MainViewModel : ObservableObject
{
    private const string DefaultModelEntry = "(server default)";

    private readonly OpenCodeServer _server = new();
    private OpenCodeApiClient? _api;
    private CancellationTokenSource? _events;
    private string? _sessionId;
    private readonly Dictionary<string, string> _roleByMessage = new();
    private readonly Dictionary<string, AssistantTextItem> _textByPart = new();
    private readonly Dictionary<string, ToolItem> _toolByPart = new();

    public ObservableCollection<object> ChatItems { get; } = [];
    public ObservableCollection<string> Models { get; } = [DefaultModelEntry];

    [ObservableProperty] private string? _selectedModel = DefaultModelEntry;
    [ObservableProperty] private string _statusText = "Server stopped";
    [ObservableProperty] private bool _isBusy;
    [ObservableProperty] private bool _isServerRunning;
    [ObservableProperty] private string _folder = Environment.CurrentDirectory;
    [ObservableProperty] private string _port = "14096";
    [ObservableProperty] private string _input = "";

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
        // an attached server is not ours to kill; disconnect only
        _server.Stop();
        IsServerRunning = false;
        IsBusy = false;
        StatusText = "Server stopped";
        if (IsServerRunning == false && !wasSpawned && _api is not null)
            AddNote("Disconnected (the attached server is still running).");
        _api = null;
        _sessionId = null;
    }

    public void Shutdown() => Stop();

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
        var model = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        _ = Task.Run(async () =>
        {
            try
            {
                await _api.SendMessageAsync(_sessionId!, text, model, CancellationToken.None);
            }
            catch (Exception ex)
            {
                PostNote($"Message failed: {ex.Message}", error: true);
            }
        });
    }

    [RelayCommand]
    private void NewChat()
    {
        _sessionId = null;
        _roleByMessage.Clear();
        _textByPart.Clear();
        _toolByPart.Clear();
        ChatItems.Clear();
    }

    private async Task EnsureSessionAsync()
    {
        if (_sessionId is not null)
            return;
        _sessionId = await _api!.CreateSessionAsync("concept chat", CancellationToken.None);
        AddNote($"Session started ({_sessionId}).");
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
                    if (IsOurSession(data))
                        IsBusy = false;
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
        if (id is not null && role is not null)
            _roleByMessage[id] = role;
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
                    textItem = new AssistantTextItem();
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
                {
                    if (state.TryGetProperty("status", out var s) && s.GetString() is { } status)
                        toolItem.Status = status;
                    if (state.TryGetProperty("title", out var ti) && ti.GetString() is { Length: > 0 } title)
                        toolItem.Title = title;
                }
                break;
        }
    }

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

    private bool IsOurSession(JsonElement data) =>
        _sessionId is null
        || !data.TryGetProperty("sessionID", out var sid)
        || sid.GetString() is not { } value
        || value == _sessionId;

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
            if (name is not null || message is not null)
                return $"Error: {name ?? "opencode"}{(message is { Length: > 0 } ? $" — {message}" : "")}";
        }
        return "Session error.";
    }

    private void AddNote(string text, bool error = false) => ChatItems.Add(new SystemNoteItem(text, error));

    private void PostNote(string text, bool error = false) => Dispatcher.UIThread.Post(() => AddNote(text, error));
}
