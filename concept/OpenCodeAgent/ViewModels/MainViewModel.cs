using System.Collections.ObjectModel;
using System.Diagnostics;
using System.IO;
using System.Text;
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
    private const string DefaultVariantEntry = "(default)";
    private const string DefaultAgentEntry = "(default build)";

    private readonly UiState _state;
    private readonly PromptStore _prompts;
    // one serve instance per workspace; several can run at the same time
    private readonly Dictionary<string, WorkspaceRuntime> _runtimes = new();
    private readonly Dictionary<string, string> _workspaceBySession = new();
    private readonly Dictionary<string, string> _titleBySession = new();
    private readonly Dictionary<string, List<PermissionItem>> _pendingBySession = new();
    private readonly List<QueueEntry> _queue = [];
    private int _workspaceGeneration;
    private string? _sessionId;
    private bool _pendingAutoTitle;
    private Task? _switchTask;
    // captured before every model-list reload: clearing the combo ItemsSource echo-writes
    // Selected*=null and would otherwise erase the selection before it is restored
    private string? _restoreModel;
    private string? _restoreVariant;
    private string? _restoreAgent;
    private readonly Dictionary<string, string> _roleByMessage = new();
    private readonly Dictionary<string, long> _createdByMessage = new();
    private readonly Dictionary<string, string> _metaByMessage = new();
    private readonly Dictionary<string, List<AssistantTextItem>> _textsByMessage = new();
    private readonly Dictionary<string, AssistantTextItem> _textByPart = new();
    private readonly Dictionary<string, ToolItem> _toolByPart = new();
    private readonly Dictionary<string, ReasoningItem> _reasoningByPart = new();
    private readonly Dictionary<string, ImageItem> _imageByPart = new();
    private readonly HashSet<string> _runningBySession = new();
    private Dictionary<string, List<string>> _variantsByModel = new();
    private Dictionary<string, int> _contextLimitByModel = new(StringComparer.OrdinalIgnoreCase);
    private string? _lastUserMessageId;
    private string? _lastUsedModel;
    private long _contextTokens;
    private int _contextLimit;
    private bool _canRedo;

    private sealed class WorkspaceRuntime
    {
        public required OpenCodeServer Server { get; init; }
        public required OpenCodeApiClient Api { get; init; }
        public CancellationTokenSource? Events { get; set; }
    }

    private sealed record QueueEntry(string SessionId, string Text, List<FileAttachment> Attachments, QueuedMessageItem Item);

    public ObservableCollection<object> ChatItems { get; } = [];
    public ObservableCollection<WorkspaceItem> Workspaces { get; } = [];
    public ObservableCollection<SessionItem> Sessions { get; } = [];
    public ObservableCollection<string> Models { get; } = [DefaultModelEntry];
    public ObservableCollection<string> Variants { get; } = [];
    public ObservableCollection<string> Agents { get; } = [DefaultAgentEntry];
    public ObservableCollection<AttachmentItem> Attachments { get; } = [];
    public ObservableCollection<CommandInfo> Commands { get; } = [];
    public ObservableCollection<string> Prompts { get; } = [];
    public ObservableCollection<FileChange> Changes { get; } = [];
    public ObservableCollection<TaskItem> Tasks { get; } = [];
    public ObservableCollection<RunningAgentItem> RunningAgents { get; } = [];

    public bool HasMessages => ChatItems.Count > 0;
    public bool HasSessionsHint => IsServerRunning && Sessions.Count == 0;
    public bool HasNoWorkspace => Workspaces.Count == 0;
    public bool HasIdleWorkspace => Workspaces.Count > 0 && !IsServerRunning;
    public bool CanStartSelected => SelectedWorkspace is not null && !IsServerRunning;
    public bool CanStopSelected => IsServerRunning;
    public bool MultipleWorkspaces => Workspaces.Count > 1;
    public string WorkspacePathLabel => SelectedWorkspace?.FolderPath ?? "no workspace selected";
    public bool HasAttachments => Attachments.Count > 0;
    public bool HasVariants => Variants.Count > 1;
    public bool HasAgents => Agents.Count > 1;
    public bool CanRedo => _canRedo;
    public bool HasTasks => Tasks.Count > 0;
    public bool HasRunningAgents => RunningAgents.Count > 0;
    public bool HasSidePanel => HasTasks || HasRunningAgents;

    /// <summary>Busy state of the open chat only; other chats may keep streaming in the background.</summary>
    public bool IsSelectedBusy => _sessionId is { } id && _runningBySession.Contains(id);
    public bool AnyBusy => _runningBySession.Count > 0;
    public string WorkingLabel => AnyBusy ? $"{_runningBySession.Count} working" : "working";

    /// <summary>Context usage of the last assistant turn, e.g. "Ctx 21% · 42.1k / 200k"; null hides the pill.</summary>
    [ObservableProperty] private string? _contextText;

    public bool HasContext => ContextText is not null;

    partial void OnContextTextChanged(string? value) => OnPropertyChanged(nameof(HasContext));

    public MainViewModel()
    {
        _state = UiState.Load();
        _prompts = PromptStore.Load();
        if (_state.Workspaces.Count == 0 && !string.IsNullOrWhiteSpace(_state.Folder))
            _state.Workspaces.Add(new WorkspaceState { Path = NormalizeDir(_state.Folder) });
        foreach (var ws in _state.Workspaces)
        {
            Workspaces.Add(new WorkspaceItem(ws));
            foreach (var sid in ws.Sessions)
                _workspaceBySession[sid] = ws.Path;
        }
        _restoreModel = _state.Model;
        _restoreVariant = _state.Variant;
        _restoreAgent = _state.Agent;
        _autoAllow = _state.AutoAllowAlways ?? true;
        foreach (var prompt in _prompts.Prompts)
            Prompts.Add(prompt);
        ChatItems.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasMessages));
        Sessions.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasSessionsHint));
        Workspaces.CollectionChanged += (_, _) =>
        {
            OnPropertyChanged(nameof(HasNoWorkspace));
            OnPropertyChanged(nameof(HasIdleWorkspace));
            OnPropertyChanged(nameof(CanStartSelected));
            OnPropertyChanged(nameof(MultipleWorkspaces));
        };
        Attachments.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasAttachments));
        Variants.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasVariants));
        Agents.CollectionChanged += (_, _) => OnPropertyChanged(nameof(HasAgents));
        Tasks.CollectionChanged += (_, _) => OnSidePanelChanged();
        RunningAgents.CollectionChanged += (_, _) => OnSidePanelChanged();
        // restore the last workspace selection but never auto-start a server
        var selected = Workspaces.FirstOrDefault(w => w.FolderPath == _state.ActiveWorkspace) ?? Workspaces.FirstOrDefault();
        if (selected is not null)
            SelectedWorkspace = selected;
    }

    [ObservableProperty] private SessionItem? _selectedSession;
    [ObservableProperty] private WorkspaceItem? _selectedWorkspace;
    [ObservableProperty] private string? _selectedModel = DefaultModelEntry;
    [ObservableProperty] private string? _selectedVariant;
    [ObservableProperty] private string? _selectedAgent = DefaultAgentEntry;
    [ObservableProperty] private string _statusText = "Server stopped";
    [ObservableProperty] private bool _isServerRunning;
    [ObservableProperty] private string _input = "";
    [ObservableProperty] private bool _autoAllow;

    partial void OnAutoAllowChanged(bool value)
    {
        _state.AutoAllowAlways = value;
        SaveState();
    }

    partial void OnIsServerRunningChanged(bool value)
    {
        OnPropertyChanged(nameof(HasSessionsHint));
        OnPropertyChanged(nameof(HasIdleWorkspace));
        OnPropertyChanged(nameof(CanStartSelected));
        OnPropertyChanged(nameof(CanStopSelected));
    }

    partial void OnSelectedWorkspaceChanged(WorkspaceItem? value)
    {
        OnPropertyChanged(nameof(WorkspacePathLabel));
        OnPropertyChanged(nameof(CanStartSelected));
        OnPropertyChanged(nameof(CanStopSelected));
        if (value is not null)
            _switchTask = SwitchWorkspaceAsync(value);
    }

    private void OnSidePanelChanged()
    {
        OnPropertyChanged(nameof(HasTasks));
        OnPropertyChanged(nameof(HasRunningAgents));
        OnPropertyChanged(nameof(HasSidePanel));
    }

    partial void OnSelectedModelChanged(string? value)
    {
        RefreshVariants();
        SaveState();
    }

    partial void OnSelectedVariantChanged(string? value) => SaveState();

    partial void OnSelectedAgentChanged(string? value) => SaveState();

    private void SaveState()
    {
        _state.ActiveWorkspace = SelectedWorkspace?.FolderPath;
        _state.Model = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        _state.Variant = SelectedVariant is { } v && v != DefaultVariantEntry ? v : null;
        _state.Agent = SelectedAgent is { } a && a != DefaultAgentEntry ? a : null;
        _state.AutoAllowAlways = AutoAllow;
        _state.Save();
    }

    private void NotifyBusyChanged()
    {
        OnPropertyChanged(nameof(IsSelectedBusy));
        OnPropertyChanged(nameof(AnyBusy));
        OnPropertyChanged(nameof(WorkingLabel));
    }

    private void ArmRestore()
    {
        _restoreModel = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        _restoreVariant = SelectedVariant is { } v && v != DefaultVariantEntry ? v : null;
        _restoreAgent = SelectedAgent is { } a && a != DefaultAgentEntry ? a : null;
    }

    private OpenCodeApiClient? SelectedApi =>
        SelectedWorkspace is { } ws && _runtimes.TryGetValue(ws.FolderPath, out var rt) ? rt.Api : null;

    private OpenCodeApiClient? ApiFor(string sessionId) =>
        _workspaceBySession.TryGetValue(sessionId, out var path) && _runtimes.TryGetValue(path, out var rt) ? rt.Api : null;

    private bool IsAppSession(string sessionId) => _workspaceBySession.ContainsKey(sessionId);

    private string TitleFor(string sessionId) =>
        _titleBySession.TryGetValue(sessionId, out var title)
            ? title
            : $"agent {sessionId[..Math.Min(8, sessionId.Length)]}";

    private void RefreshVariants()
    {
        Variants.Clear();
        Variants.Add(DefaultVariantEntry);
        if (SelectedModel is { } model && _variantsByModel.TryGetValue(model, out var variants))
            foreach (var variant in variants)
                Variants.Add(variant);
        SelectedVariant = DefaultVariantEntry;
    }

    public void AddWorkspace(string path)
    {
        if (string.IsNullOrWhiteSpace(path))
            return;
        var normalized = NormalizeDir(path);
        if (!Directory.Exists(normalized))
        {
            Announce($"Folder does not exist: {normalized}", error: true);
            return;
        }
        if (Workspaces.Any(w => w.FolderPath == normalized))
        {
            Announce("That folder is already a workspace.");
            return;
        }
        var state = new WorkspaceState { Path = normalized };
        _state.Workspaces.Add(state);
        var item = new WorkspaceItem(state);
        Workspaces.Add(item);
        SaveState();
        SelectedWorkspace = item;
    }

    public void RemoveWorkspace(WorkspaceItem ws)
    {
        var wasSelected = ReferenceEquals(SelectedWorkspace, ws);
        if (_runtimes.TryGetValue(ws.FolderPath, out var rt))
        {
            rt.Events?.Cancel();
            rt.Server.Stop();
            _runtimes.Remove(ws.FolderPath);
        }
        foreach (var sid in ws.State.Sessions)
            SetRunning(sid, false);
        DropQueue(ws.State.Sessions);
        DropPending(ws.State.Sessions);
        foreach (var sid in ws.State.Sessions)
        {
            _workspaceBySession.Remove(sid);
            _titleBySession.Remove(sid);
        }
        _state.Workspaces.Remove(ws.State);
        Workspaces.Remove(ws);
        if (wasSelected)
        {
            SelectedWorkspace = null;
            _sessionId = null;
            _pendingAutoTitle = false;
            Sessions.Clear();
            SelectedSession = null;
            ClearTranscript();
            IsServerRunning = false;
            StatusText = "Server stopped";
        }
        SaveState();
    }

    [RelayCommand]
    private async Task StartWorkspaceAsync(WorkspaceItem? ws)
    {
        if (ws is null || (_runtimes.TryGetValue(ws.FolderPath, out var existing) && existing.Server.IsRunning))
            return;
        StatusText = $"Starting opencode serve — {ws.Name}…";
        try
        {
            var port = OpenCodeServer.FindFreePort();
            var server = new OpenCodeServer();
            await server.StartAsync("opencode", ws.FolderPath, port, s => Debug.WriteLine($"[opencode] {s}"), CancellationToken.None);
            var rt = new WorkspaceRuntime { Server = server, Api = new OpenCodeApiClient(server.BaseUrl) };
            _runtimes[ws.FolderPath] = rt;
            ws.IsRunning = true;
            rt.Events = new CancellationTokenSource();
            _ = PumpEventsAsync(rt.Api, rt.Events.Token, ws.Name);
            if (!ReferenceEquals(ws, SelectedWorkspace))
            {
                // started from the context menu; the open workspace's data stays untouched
                StatusText = $"Server ready — {ws.Name} ({server.BaseUrl})";
                _ = PrimeWorkspaceTitlesAsync(ws, rt);
                return;
            }
            IsServerRunning = true;
            StatusText = (server.Spawned ? "Server ready — " : "Attached — ") + server.BaseUrl;
            await LoadWorkspaceDataAsync(ws, rt);
        }
        catch (Exception ex)
        {
            StatusText = "Start failed";
            AddNote($"Failed to start: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private void StopWorkspace(WorkspaceItem? ws)
    {
        if (ws is null || !_runtimes.TryGetValue(ws.FolderPath, out var rt))
            return;
        rt.Events?.Cancel();
        var wasAttached = !rt.Server.Spawned;
        rt.Server.Stop();
        _runtimes.Remove(ws.FolderPath);
        ws.IsRunning = false;
        var sessionIds = ws.State.Sessions.ToList();
        foreach (var sid in sessionIds)
            SetRunning(sid, false);
        DropQueue(sessionIds);
        DropPending(sessionIds);
        if (ReferenceEquals(ws, SelectedWorkspace))
        {
            IsServerRunning = false;
            StatusText = "Server stopped";
            Sessions.Clear();
            SelectedSession = null;
            if (_sessionId is { } sid && sessionIds.Contains(sid))
            {
                _sessionId = null;
                ClearTranscript();
            }
            NotifyBusyChanged();
        }
        if (wasAttached)
            AddNote("Disconnected (the attached server is still running).");
    }

    public void Shutdown()
    {
        foreach (var rt in _runtimes.Values)
        {
            rt.Events?.Cancel();
            rt.Server.Stop();
        }
        _runtimes.Clear();
    }

    private async Task SwitchWorkspaceAsync(WorkspaceItem ws)
    {
        _sessionId = null;
        _pendingAutoTitle = false;
        SelectedSession = null;
        ClearTranscript();
        SwapQueueView(null);
        Sessions.Clear();
        if (!_runtimes.TryGetValue(ws.FolderPath, out var rt) || !rt.Server.IsRunning)
        {
            IsServerRunning = false;
            StatusText = $"Server stopped — {ws.Name}";
            return;
        }
        IsServerRunning = true;
        StatusText = (rt.Server.Spawned ? "Server ready — " : "Attached — ") + rt.Server.BaseUrl;
        await LoadWorkspaceDataAsync(ws, rt);
    }

    /// <summary>Models, agents, commands and the chat list of a workspace whose server is already up.</summary>
    private async Task LoadWorkspaceDataAsync(WorkspaceItem ws, WorkspaceRuntime rt)
    {
        var gen = ++_workspaceGeneration;
        ArmRestore();
        _ = LoadModelsAsync(rt, gen);
        _ = LoadAgentsAsync(rt, gen);
        _ = LoadCommandsAsync(rt, gen);
        await RefreshSessionsAsync();
        if (!ReferenceEquals(SelectedWorkspace, ws))
            return;
        if (ws.State.ActiveSession is { } id)
        {
            var item = Sessions.FirstOrDefault(s => s.Id == id);
            if (item is not null)
                await OpenSessionAsync(item);
        }
    }

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
        if (SelectedWorkspace is not { } ws || !_runtimes.TryGetValue(ws.FolderPath, out var rt))
        {
            SnapSelection();
            return;
        }

        try
        {
            var messages = await rt.Api.GetMessagesAsync(session.Id, CancellationToken.None);
            if (!ReferenceEquals(SelectedWorkspace, ws) || !_runtimes.TryGetValue(ws.FolderPath, out rt))
                return;
            SwapQueueView(session.Id);
            _sessionId = session.Id;
            ws.State.ActiveSession = session.Id;
            _pendingAutoTitle = false;
            ClearTranscript();
            Restore(messages);
            FlushPendingPermissions(session.Id);
            SetActive(session);
            SelectedSession = session;
            NotifyBusyChanged();
            SaveState();
        }
        catch (Exception ex)
        {
            AddNote($"Could not load chat: {ex.Message}", error: true);
            SnapSelection();
        }
    }

    /// <summary>Opens a chat from the running-agents panel, switching workspaces if needed.</summary>
    public async Task OpenRunningAsync(RunningAgentItem running)
    {
        if (!_workspaceBySession.TryGetValue(running.SessionId, out var path))
            return;
        var ws = Workspaces.FirstOrDefault(w => w.FolderPath == path);
        if (ws is null)
            return;
        if (!ReferenceEquals(SelectedWorkspace, ws))
            SelectedWorkspace = ws; // the change hook stores its switch task in _switchTask
        if (_switchTask is { } task)
        {
            try
            {
                await task;
            }
            catch
            {
                // switch failures surface through notes elsewhere
            }
        }
        var session = Sessions.FirstOrDefault(s => s.Id == running.SessionId);
        if (session is not null)
            await OpenSessionAsync(session);
    }

    [RelayCommand]
    private async Task SendAsync()
    {
        var text = Input.Trim();
        if ((text.Length == 0 && Attachments.Count == 0) || !IsServerRunning || SelectedApi is null)
            return;
        var attachments = TakeAttachments();
        Input = "";
        if (IsSelectedBusy)
        {
            // park the message on this chat; it is sent when that chat goes idle
            var item = new QueuedMessageItem(text, attachments.Count) { Remove = RemoveQueued };
            _queue.Add(new QueueEntry(_sessionId!, text, attachments, item));
            ChatItems.Add(item);
            return;
        }
        if (_sessionId is null)
        {
            try
            {
                await EnsureSessionAsync();
            }
            catch (Exception ex)
            {
                AddNote($"Could not create a session: {ex.Message}", error: true);
                return;
            }
        }
        if (_sessionId is { } sessionId)
            SendCoreAsync(sessionId, text, attachments);
    }

    private void SendCoreAsync(string sessionId, string text, List<FileAttachment> attachments)
    {
        if (ApiFor(sessionId) is not { } api)
            return;
        var sentAt = DateTime.Now;
        if (sessionId == _sessionId)
        {
            ChatItems.Add(new UserMessageItem(text, sentAt));
            foreach (var attachment in attachments)
                ChatItems.Add(attachment.IsImage
                    ? new ImageItem(attachment.FileName, attachment.Data, sentAt)
                    : new SystemNoteItem($"Attachment: {attachment.FileName}"));
        }
        SetRunning(sessionId, true);
        AutoTitle(sessionId, api, text.Length > 0 ? text : attachments.FirstOrDefault()?.FileName ?? "New chat");
        var model = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        var variant = SelectedVariant is { } v && v != DefaultVariantEntry ? v : null;
        var agent = SelectedAgent is { } a && a != DefaultAgentEntry ? a : null;
        _ = Task.Run(async () =>
        {
            try
            {
                await api.SendMessageAsync(sessionId, text, model, variant, agent, attachments, CancellationToken.None);
            }
            catch (Exception ex)
            {
                PostNote(sessionId, $"Message failed: {ex.Message}", error: true);
            }
        });
    }

    private List<FileAttachment> TakeAttachments()
    {
        var attachments = Attachments.Select(a => new FileAttachment(a.FileName, a.Mime, a.Data)).ToList();
        Attachments.Clear();
        return attachments;
    }

    private void RemoveQueued(QueuedMessageItem item)
    {
        _queue.RemoveAll(e => ReferenceEquals(e.Item, item));
        ChatItems.Remove(item);
    }

    /// <summary>Queued bubbles are only visible in their own chat; parked out of view on switch.</summary>
    private void SwapQueueView(string? sessionId)
    {
        foreach (var entry in _queue)
            ChatItems.Remove(entry.Item);
        if (sessionId is { } id)
            foreach (var entry in _queue.Where(e => e.SessionId == id).ToList())
                ChatItems.Add(entry.Item);
    }

    private void DropQueue(IReadOnlyCollection<string> sessionIds)
    {
        for (var i = _queue.Count - 1; i >= 0; i--)
        {
            if (!sessionIds.Contains(_queue[i].SessionId))
                continue;
            ChatItems.Remove(_queue[i].Item);
            _queue.RemoveAt(i);
        }
    }

    private void DropPending(IReadOnlyCollection<string> sessionIds)
    {
        foreach (var sid in sessionIds)
            if (_pendingBySession.Remove(sid, out _))
                MarkAttention(sid, false);
    }

    /// <summary>Sends the oldest queued message after that chat finishes its turn; called on session.idle.</summary>
    private void TryDrainQueue(string sessionId)
    {
        if (ApiFor(sessionId) is null)
            return;
        var entry = _queue.FirstOrDefault(e => e.SessionId == sessionId);
        if (entry is null)
            return;
        _queue.Remove(entry);
        ChatItems.Remove(entry.Item);
        SendCoreAsync(sessionId, entry.Text, entry.Attachments);
    }

    public void AttachImage(byte[] png)
    {
        Attachments.Add(new AttachmentItem($"paste-{Attachments.Count + 1}.png", "image/png", png));
    }

    public void AttachFile(string fileName, string mime, byte[] data)
    {
        Attachments.Add(new AttachmentItem(fileName, mime, data));
    }

    public void AppendInput(string text) => Input = Input.Length == 0 ? text : Input + text;

    /// <summary>Replaces the input text (edit-last-message / prompt library).</summary>
    public void LoadIntoInput(string text) => Input = text;

    [RelayCommand]
    private void RemoveAttachment(AttachmentItem? attachment)
    {
        if (attachment is not null)
            Attachments.Remove(attachment);
    }

    [RelayCommand]
    private async Task AbortAsync()
    {
        if (_sessionId is not { } id || !IsSelectedBusy || ApiFor(id) is not { } api)
            return;
        try
        {
            await api.AbortAsync(id, CancellationToken.None);
            SetRunning(id, false);
        }
        catch (Exception ex)
        {
            AddNote($"Abort failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private async Task StopRunningAsync(string? sessionId)
    {
        if (sessionId is null || ApiFor(sessionId) is not { } api)
            return;
        try
        {
            await api.AbortAsync(sessionId, CancellationToken.None);
            SetRunning(sessionId, false);
        }
        catch (Exception ex)
        {
            PostNote(sessionId, $"Abort failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private void NewChat()
    {
        _sessionId = null;
        _pendingAutoTitle = false;
        if (SelectedWorkspace is { } ws)
            ws.State.ActiveSession = null;
        SwapQueueView(null);
        ClearTranscript();
        foreach (var s in Sessions)
            s.IsActive = false;
        SelectedSession = null;
        NotifyBusyChanged();
        SaveState();
    }

    public void TogglePin(SessionItem session)
    {
        session.IsPinned = !session.IsPinned;
        if (session.IsPinned)
            _state.Pins[session.Id] = true;
        else
            _state.Pins.Remove(session.Id);
        SaveState();
        SortSessions();
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
        if (title.Length == 0 || title == session.Title || ApiFor(session.Id) is not { } api)
            return;
        try
        {
            var info = await api.RenameSessionAsync(session.Id, title, CancellationToken.None);
            session.Title = info.Title.Length > 0 ? info.Title : title;
            session.Updated = info.UpdatedAt;
            _titleBySession[session.Id] = session.Title;
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
        if (session is null)
            return;
        if (IsSelectedBusy && session.Id == _sessionId)
        {
            AddNote("Stop the running turn before deleting this chat.", error: true);
            return;
        }
        if (ApiFor(session.Id) is not { } api)
            return;
        try
        {
            await api.DeleteSessionAsync(session.Id, CancellationToken.None);
        }
        catch (Exception ex)
        {
            AddNote($"Delete failed: {ex.Message}", error: true);
            return;
        }

        ForgetSession(session.Id);
        SaveState();
    }

    /// <summary>Unlinks the chat from its workspace; the session itself survives on the server.</summary>
    public void RemoveFromWorkspace(SessionItem session)
    {
        if (IsSelectedBusy && session.Id == _sessionId)
        {
            AddNote("Stop the running turn before removing this chat.", error: true);
            return;
        }
        ForgetSession(session.Id);
        SaveState();
    }

    private void ForgetSession(string id)
    {
        var wasActive = id == _sessionId;
        _workspaceBySession.Remove(id);
        foreach (var ws in _state.Workspaces)
            ws.Sessions.Remove(id);
        _titleBySession.Remove(id);
        var item = Sessions.FirstOrDefault(s => s.Id == id);
        if (item is not null)
            Sessions.Remove(item);
        _state.Pins.Remove(id);
        DropQueue([id]);
        DropPending([id]);
        if (wasActive)
        {
            _sessionId = null;
            _pendingAutoTitle = false;
            ClearTranscript();
            SelectedSession = null;
            if (SelectedWorkspace is { } ws)
                ws.State.ActiveSession = null;
            NotifyBusyChanged();
        }
    }

    private async Task EnsureSessionAsync()
    {
        if (_sessionId is not null)
            return;
        if (SelectedWorkspace is not { } ws || !_runtimes.TryGetValue(ws.FolderPath, out var rt))
            return;
        var info = await rt.Api.CreateSessionAsync("New chat", CancellationToken.None);
        _sessionId = info.Id;
        ws.State.ActiveSession = info.Id;
        ws.State.Sessions.Insert(0, info.Id);
        _workspaceBySession[info.Id] = ws.FolderPath;
        _titleBySession[info.Id] = info.Title;
        _pendingAutoTitle = true;
        foreach (var s in Sessions)
            s.IsActive = false;
        var item = new SessionItem(info.Id, info.Title, info.UpdatedAt) { IsActive = true };
        Sessions.Insert(0, item);
        SelectedSession = item;
        SaveState();
    }

    private void AutoTitle(string sessionId, OpenCodeApiClient api, string text)
    {
        if (!_pendingAutoTitle || sessionId != _sessionId)
            return;
        _pendingAutoTitle = false;
        var title = DeriveTitle(text);
        _ = Task.Run(async () =>
        {
            SessionInfo info;
            try
            {
                info = await api.RenameSessionAsync(sessionId, title, CancellationToken.None);
            }
            catch
            {
                return;
            }
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                _titleBySession[sessionId] = info.Title.Length > 0 ? info.Title : title;
                var item = Sessions.FirstOrDefault(s => s.Id == sessionId);
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
        if (SelectedWorkspace is not { } ws || !_runtimes.TryGetValue(ws.FolderPath, out var rt))
            return;
        try
        {
            var sessions = await rt.Api.ListSessionsAsync(CancellationToken.None);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (!ReferenceEquals(SelectedWorkspace, ws))
                    return;
                // only chats this app created under this workspace are listed
                var known = ws.State.Sessions.ToHashSet();
                var mine = sessions
                    .Where(s => known.Contains(s.Id))
                    .OrderByDescending(s => _state.Pins.ContainsKey(s.Id))
                    .ThenByDescending(s => s.UpdatedAt)
                    .ToList();
                var alive = mine.Select(s => s.Id).ToHashSet();
                if (ws.State.Sessions.RemoveAll(id => !alive.Contains(id)) > 0)
                {
                    var gone = known.Where(id => !alive.Contains(id)).ToList();
                    foreach (var id in gone)
                    {
                        _workspaceBySession.Remove(id);
                        _titleBySession.Remove(id);
                    }
                    DropQueue(gone);
                    DropPending(gone);
                }
                foreach (var s in mine)
                    _titleBySession[s.Id] = s.Title;
                Sessions.Clear();
                foreach (var s in mine)
                    Sessions.Add(new SessionItem(s.Id, s.Title, s.UpdatedAt)
                    {
                        IsPinned = _state.Pins.ContainsKey(s.Id),
                    });
                if (_sessionId is { } active)
                {
                    var match = Sessions.FirstOrDefault(s => s.Id == active);
                    if (match is not null)
                        match.IsActive = true;
                }
                foreach (var running in RunningAgents)
                    if (alive.Contains(running.SessionId))
                        running.Title = _titleBySession[running.SessionId];
                SnapSelection();
                SaveState();
            });
        }
        catch
        {
            // the sidebar is optional; chat still works without it
        }
    }

    private async Task PrimeWorkspaceTitlesAsync(WorkspaceItem ws, WorkspaceRuntime rt)
    {
        try
        {
            var sessions = await rt.Api.ListSessionsAsync(CancellationToken.None);
            var known = ws.State.Sessions.ToHashSet();
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                foreach (var s in sessions.Where(s => known.Contains(s.Id)))
                    _titleBySession[s.Id] = s.Title;
                foreach (var running in RunningAgents)
                    if (known.Contains(running.SessionId) && _titleBySession.TryGetValue(running.SessionId, out var title))
                        running.Title = title;
            });
        }
        catch
        {
        }
    }

    private void SortSessions()
    {
        var ordered = Sessions
            .OrderByDescending(s => s.IsPinned)
            .ThenByDescending(s => s.Updated)
            .ToList();
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

    private async Task LoadModelsAsync(WorkspaceRuntime rt, int gen)
    {
        try
        {
            var models = await rt.Api.GetModelsAsync(CancellationToken.None);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (gen != _workspaceGeneration)
                    return;
                Models.Clear();
                Models.Add(DefaultModelEntry);
                _variantsByModel = models.ToDictionary(m => m.Key, m => m.Variants.ToList());
                _contextLimitByModel = models.ToDictionary(m => m.Key, m => m.ContextLimit, StringComparer.OrdinalIgnoreCase);
                foreach (var m in models)
                    Models.Add(m.Key);
                // restore the remembered model first so RefreshVariants repopulates for it
                if (_restoreModel is { } saved && Models.Contains(saved))
                    SelectedModel = saved;
                else
                    SelectedModel ??= DefaultModelEntry;
                if (_restoreVariant is { } savedVariant && Variants.Contains(savedVariant))
                    SelectedVariant = savedVariant;
                _restoreModel = null;
                _restoreVariant = null;
            });
        }
        catch
        {
            // the model list is optional; the server default still works
        }
    }

    private async Task LoadAgentsAsync(WorkspaceRuntime rt, int gen)
    {
        try
        {
            var agents = await rt.Api.GetAgentsAsync(CancellationToken.None);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (gen != _workspaceGeneration)
                    return;
                Agents.Clear();
                Agents.Add(DefaultAgentEntry);
                foreach (var agent in agents.Where(a => !a.Hidden && a.Mode is "primary" or "all"))
                    Agents.Add(agent.Name);
                if (_restoreAgent is { } saved && Agents.Contains(saved))
                    SelectedAgent = saved;
                _restoreAgent = null;
            });
        }
        catch
        {
            // the agent list is optional; the server default (build) still works
        }
    }

    private async Task LoadCommandsAsync(WorkspaceRuntime rt, int gen)
    {
        try
        {
            var commands = await rt.Api.GetCommandsAsync(CancellationToken.None);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (gen != _workspaceGeneration)
                    return;
                Commands.Clear();
                foreach (var command in commands)
                    Commands.Add(command);
            });
        }
        catch
        {
            // server commands are optional
        }
    }

    [RelayCommand]
    private async Task CompactAsync()
    {
        if (_sessionId is not { } id || ApiFor(id) is not { } api)
            return;
        if (IsSelectedBusy)
        {
            AddNote("Stop the running turn before compacting.", error: true);
            return;
        }
        var model = SelectedModel is { } m && m != DefaultModelEntry ? m : _lastUsedModel;
        if (model is null)
        {
            AddNote("Pick a model to compact with.", error: true);
            return;
        }
        AddNote("Compacting the conversation…");
        try
        {
            await api.SummarizeAsync(id, model, CancellationToken.None);
        }
        catch (Exception ex)
        {
            AddNote($"Compact failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private async Task UndoAsync()
    {
        if (_sessionId is not { } id || ApiFor(id) is not { } api)
            return;
        if (IsSelectedBusy)
        {
            AddNote("Stop the running turn before undoing.", error: true);
            return;
        }
        if (_lastUserMessageId is not { } target)
        {
            AddNote("Nothing to undo yet.", error: true);
            return;
        }
        try
        {
            await api.RevertAsync(id, target, CancellationToken.None);
            _canRedo = true;
            OnPropertyChanged(nameof(CanRedo));
            AddNote("Reverted the last turn (files restored).");
            await ReloadTranscriptAsync();
        }
        catch (Exception ex)
        {
            AddNote($"Undo failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private async Task RedoAsync()
    {
        if (_sessionId is not { } id || ApiFor(id) is not { } api || !_canRedo)
            return;
        if (IsSelectedBusy)
        {
            AddNote("Stop the running turn before redoing.", error: true);
            return;
        }
        try
        {
            await api.UnrevertAsync(id, CancellationToken.None);
            _canRedo = false;
            OnPropertyChanged(nameof(CanRedo));
            AddNote("Re-applied the reverted turn.");
            await ReloadTranscriptAsync();
        }
        catch (Exception ex)
        {
            AddNote($"Redo failed: {ex.Message}", error: true);
        }
    }

    private async Task ReloadTranscriptAsync()
    {
        if (_sessionId is not { } id || ApiFor(id) is not { } api)
            return;
        try
        {
            var messages = await api.GetMessagesAsync(id, CancellationToken.None);
            ClearTranscript();
            Restore(messages);
        }
        catch (Exception ex)
        {
            AddNote($"Could not reload the transcript: {ex.Message}", error: true);
        }
    }

    /// <summary>Shares the session and returns the public URL for the caller to copy; null when the server gave none.</summary>
    public async Task<string?> ShareSessionAsync(SessionItem session)
    {
        if (ApiFor(session.Id) is not { } api)
            return null;
        try
        {
            return await api.ShareAsync(session.Id, CancellationToken.None);
        }
        catch (Exception ex)
        {
            AddNote($"Share failed: {ex.Message}", error: true);
            return null;
        }
    }

    public async Task UnshareSessionAsync(SessionItem session)
    {
        if (ApiFor(session.Id) is not { } api)
            return;
        try
        {
            await api.UnshareAsync(session.Id, CancellationToken.None);
            AddNote("Session is no longer shared.");
        }
        catch (Exception ex)
        {
            AddNote($"Unshare failed: {ex.Message}", error: true);
        }
    }

    [RelayCommand]
    private void SavePrompt()
    {
        var text = Input.Trim();
        if (text.Length == 0)
            return;
        if (Prompts.Contains(text))
            return;
        Prompts.Add(text);
        _prompts.Prompts = [.. Prompts];
        _prompts.Save();
    }

    [RelayCommand]
    private void DeletePrompt(string? prompt)
    {
        if (prompt is null || !Prompts.Remove(prompt))
            return;
        _prompts.Prompts = [.. Prompts];
        _prompts.Save();
    }

    /// <summary>Runs a server command as a turn; the current input becomes $ARGUMENTS.</summary>
    public async Task RunCommandAsync(CommandInfo? command)
    {
        if (command is null || !IsServerRunning || SelectedApi is null)
            return;
        if (IsSelectedBusy)
        {
            AddNote("Stop the running turn before running a command.", error: true);
            return;
        }
        var arguments = Input.Trim();
        Input = "";
        try
        {
            await EnsureSessionAsync();
        }
        catch (Exception ex)
        {
            AddNote($"Could not create a session: {ex.Message}", error: true);
            return;
        }
        if (_sessionId is not { } sessionId || ApiFor(sessionId) is not { } api)
            return;
        ChatItems.Add(new UserMessageItem("/" + command.Name + (arguments.Length > 0 ? " " + arguments : ""), DateTime.Now));
        SetRunning(sessionId, true);
        var model = SelectedModel is { } m && m != DefaultModelEntry ? m : null;
        var agent = SelectedAgent is { } a && a != DefaultAgentEntry ? a : null;
        _ = Task.Run(async () =>
        {
            try
            {
                await api.RunCommandAsync(sessionId, command.Name, arguments, agent, model, CancellationToken.None);
            }
            catch (Exception ex)
            {
                PostNote(sessionId, $"Command failed: {ex.Message}", error: true);
            }
        });
    }

    private async Task PumpEventsAsync(OpenCodeApiClient api, CancellationToken ct, string workspaceName)
    {
        try
        {
            await foreach (var evt in api.StreamEventsAsync(ct))
                HandleEvent(evt);
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            PostNote($"[{workspaceName}] event stream ended: {ex.Message}", error: true);
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
                var statusSession = ReadSessionId(data);
                Dispatcher.UIThread.Post(() => SetRunning(statusSession, status is "busy" or "retry"));
                break;
            case "session.idle":
                var idleSession = ReadSessionId(data);
                Dispatcher.UIThread.Post(() =>
                {
                    SetRunning(idleSession, false);
                    if (idleSession is not { } sid)
                        return;
                    if (sid == _sessionId)
                        _ = RefreshSessionsAsync();
                    TryDrainQueue(sid);
                });
                break;
            case "todo.updated":
                Dispatcher.UIThread.Post(() => OnTodosUpdated(data));
                break;
            case "message.part.removed":
                Dispatcher.UIThread.Post(() => OnPartRemoved(data));
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
        if (role == "user")
            _lastUserMessageId = id;
        if (role != "assistant")
            return;
        // remember the model that produced the last turn, e.g. for Compact
        if (info.TryGetProperty("providerID", out var pid) && info.TryGetProperty("modelID", out var mid) &&
            pid.GetString() is { Length: > 0 } provider && mid.GetString() is { Length: > 0 } modelName)
            _lastUsedModel = $"{provider}/{modelName}";
        if (BuildMeta(info) is { } meta)
        {
            _metaByMessage[id] = meta;
            if (_textsByMessage.TryGetValue(id, out var metaItems))
                foreach (var item in metaItems)
                    item.MetaLabel = meta;
        }
        var total = ReadTokenTotal(info);
        if (total > 0)
        {
            _contextTokens = total;
            _contextLimit = _lastUsedModel is { } key && _contextLimitByModel.TryGetValue(key, out var limit) ? limit : 0;
            UpdateContextText();
        }
    }

    private static long ReadTokenTotal(JsonElement info)
    {
        if (!info.TryGetProperty("tokens", out var tokens) || tokens.ValueKind != JsonValueKind.Object)
            return 0;
        long Sum(string name) =>
            tokens.TryGetProperty(name, out var v) && v.TryGetInt64(out var n) ? n : 0;
        long cacheRead = 0, cacheWrite = 0;
        if (tokens.TryGetProperty("cache", out var cache) && cache.ValueKind == JsonValueKind.Object)
        {
            cacheRead = cache.TryGetProperty("read", out var cr) && cr.TryGetInt64(out var crn) ? crn : 0;
            cacheWrite = cache.TryGetProperty("write", out var cw) && cw.TryGetInt64(out var cwn) ? cwn : 0;
        }
        return Sum("input") + Sum("output") + Sum("reasoning") + cacheRead + cacheWrite;
    }

    private void UpdateContextText()
    {
        var used = FormatTokens(_contextTokens);
        ContextText = _contextLimit > 0
            ? $"Ctx {_contextTokens * 100 / _contextLimit}% · {used} / {FormatTokens(_contextLimit)}"
            : $"Ctx {used} tok";
    }

    private static string FormatTokens(long tokens)
    {
        if (tokens < 1000)
            return tokens.ToString();
        if (tokens < 1_000_000)
            return (tokens / 1000d).ToString("0.#") + "k";
        return (tokens / 1_000_000d).ToString("0.##") + "M";
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
                    textItem = new AssistantTextItem(CreatedAtFor(messageId)) { MetaLabel = MetaFor(messageId) };
                    _textByPart[id] = textItem;
                    if (messageId is not null)
                    {
                        if (!_textsByMessage.TryGetValue(messageId, out var textList))
                            _textsByMessage[messageId] = textList = [];
                        textList.Add(textItem);
                    }
                    ChatItems.Add(textItem);
                }
                if (part.TryGetProperty("text", out var txt))
                    textItem.Text = txt.GetString() ?? "";
                break;
            case "reasoning":
                if (!_reasoningByPart.TryGetValue(id, out var reasoningItem))
                {
                    reasoningItem = new ReasoningItem(CreatedAtFor(messageId));
                    _reasoningByPart[id] = reasoningItem;
                    ChatItems.Add(reasoningItem);
                }
                if (part.TryGetProperty("text", out var reasoningText))
                    reasoningItem.Text = reasoningText.GetString() ?? "";
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
                    ApplyToolState(state, toolItem);
                    ApplyToolMetadata(state, toolItem);
                }
                break;
            case "file":
                if (!_imageByPart.ContainsKey(id) && part.TryGetProperty("url", out var url) &&
                    part.TryGetProperty("filename", out var fn))
                {
                    var item = ImageItem.FromDataUrl(url.GetString(), fn.GetString() ?? "image.png", CreatedAtFor(messageId));
                    if (item is not null)
                    {
                        _imageByPart[id] = item;
                        ChatItems.Add(item);
                    }
                }
                break;
        }
    }

    private void Restore(List<StoredMessage> messages)
    {
        List<TaskItem>? todos = null;
        foreach (var msg in messages)
        {
            _roleByMessage[msg.Id] = msg.Role;
            _createdByMessage[msg.Id] = msg.CreatedMs;
            if (msg.Role == "user")
                _lastUserMessageId = msg.Id;
            if (BuildMeta(msg.Info) is { } meta)
                _metaByMessage[msg.Id] = meta;
            if (msg.Role == "assistant")
            {
                // refill the ctx pill and Compact's model after a chat switch
                if (msg.Info.TryGetProperty("providerID", out var rp) && msg.Info.TryGetProperty("modelID", out var rm) &&
                    rp.GetString() is { Length: > 0 } provider && rm.GetString() is { Length: > 0 } modelName)
                    _lastUsedModel = $"{provider}/{modelName}";
                var total = ReadTokenTotal(msg.Info);
                if (total > 0)
                {
                    _contextTokens = total;
                    _contextLimit = _lastUsedModel is { } key && _contextLimitByModel.TryGetValue(key, out var limit) ? limit : 0;
                    UpdateContextText();
                }
            }
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
                            var item = new AssistantTextItem(createdAt) { Text = text, MetaLabel = MetaFor(msg.Id) };
                            _textByPart[id] = item;
                            if (!_textsByMessage.TryGetValue(msg.Id, out var restoredTexts))
                                _textsByMessage[msg.Id] = restoredTexts = [];
                            restoredTexts.Add(item);
                            ChatItems.Add(item);
                        }
                        break;
                    case "reasoning":
                        var reasoning = new ReasoningItem(createdAt);
                        if (part.TryGetProperty("text", out var rx))
                            reasoning.Text = rx.GetString() ?? "";
                        _reasoningByPart[id] = reasoning;
                        ChatItems.Add(reasoning);
                        break;
                    case "tool":
                        var toolName = part.TryGetProperty("tool", out var tn) ? tn.GetString() ?? "tool" : "tool";
                        var toolItem = new ToolItem(toolName);
                        if (part.TryGetProperty("state", out var state))
                        {
                            ApplyToolState(state, toolItem);
                            ApplyToolMetadata(state, toolItem);
                        }
                        _toolByPart[id] = toolItem;
                        ChatItems.Add(toolItem);
                        if (toolName == "todowrite")
                            todos = TodosFromPart(part);
                        break;
                    case "file":
                        if (part.TryGetProperty("url", out var url))
                        {
                            var fileName = part.TryGetProperty("filename", out var fn) ? fn.GetString() ?? "image.png" : "image.png";
                            var item = ImageItem.FromDataUrl(url.GetString(), fileName, createdAt);
                            if (item is not null)
                            {
                                _imageByPart[id] = item;
                                ChatItems.Add(item);
                            }
                            else
                            {
                                ChatItems.Add(new SystemNoteItem($"Attachment: {fileName}"));
                            }
                        }
                        break;
                }
            }
        }
        if (todos is not null)
            foreach (var t in todos)
                Tasks.Add(t);
        RefreshChanges();
    }

    private void OnPartRemoved(JsonElement data)
    {
        if (!IsOurSession(data))
            return;
        var id = data.TryGetProperty("partID", out var pid) ? pid.GetString()
            : data.TryGetProperty("part", out var p) && p.TryGetProperty("id", out var pi) ? pi.GetString()
            : null;
        if (id is null)
            return;
        ChatItem? removed = null;
        if (_textByPart.Remove(id, out var textItem))
            removed = textItem;
        else if (_toolByPart.Remove(id, out var toolItem))
            removed = toolItem;
        else if (_imageByPart.Remove(id, out var imageItem))
            removed = imageItem;
        if (removed is null)
            return;
        ChatItems.Remove(removed);
        RefreshChanges();
    }

    private void ApplyToolState(JsonElement state, ToolItem toolItem)
    {
        if (state.TryGetProperty("status", out var s) && s.GetString() is { } status)
        {
            toolItem.Status = status;
            toolItem.IsError = status.Equals("error", StringComparison.OrdinalIgnoreCase);
        }
        if (state.TryGetProperty("title", out var ti) && ti.GetString() is { Length: > 0 } title)
            toolItem.Title = title;
        var input = DescribePart(state, "input");
        var output = DescribePart(state, "output");
        var error = DescribePart(state, "error");
        toolItem.ErrorPreview = error.Length > 0 ? error.Split('\n', 2)[0] : "";
        toolItem.Details = BuildToolDetails(input, output, error);
    }

    private static string DescribePart(JsonElement state, string name)
    {
        if (!state.TryGetProperty(name, out var value) || value.ValueKind is JsonValueKind.Null or JsonValueKind.Undefined)
            return "";
        var text = value.ValueKind == JsonValueKind.String
            ? value.GetString() ?? ""
            : JsonSerializer.Serialize(value, new JsonSerializerOptions { WriteIndented = true });
        if (text.Length == 0)
            return "";
        return text.Length <= 4000 ? text : text[..4000] + "\n… (truncated)";
    }

    private static string BuildToolDetails(string input, string output, string error)
    {
        var sb = new StringBuilder();
        if (input.Length > 0)
            sb.Append("input:\n").Append(input).Append('\n');
        if (output.Length > 0)
        {
            if (sb.Length > 0)
                sb.Append('\n');
            sb.Append("output:\n").Append(output).Append('\n');
        }
        if (error.Length > 0)
        {
            if (sb.Length > 0)
                sb.Append('\n');
            sb.Append("error:\n").Append(error).Append('\n');
        }
        return sb.ToString();
    }

    private void ApplyToolMetadata(JsonElement state, ToolItem toolItem)
    {
        if (!state.TryGetProperty("metadata", out var md) || md.ValueKind != JsonValueKind.Object)
            return;
        if (md.TryGetProperty("filepath", out var fp) && fp.GetString() is { Length: > 0 } filePath)
            toolItem.FilePath = filePath;
        if (md.TryGetProperty("filediff", out var fd) && fd.ValueKind == JsonValueKind.Object)
        {
            toolItem.FileDiff = fd.TryGetProperty("patch", out var patch) ? patch.GetString() : null;
            toolItem.Additions = fd.TryGetProperty("additions", out var add) && add.TryGetInt32(out var a) ? a : 0;
            toolItem.Deletions = fd.TryGetProperty("deletions", out var del) && del.TryGetInt32(out var d) ? d : 0;
        }
        RefreshChanges();
    }

    /// <summary>Files touched by the visible transcript, last edit per path wins.</summary>
    private void RefreshChanges()
    {
        var byFile = new Dictionary<string, FileChange>(StringComparer.OrdinalIgnoreCase);
        foreach (var tool in ChatItems.OfType<ToolItem>())
        {
            if (tool.FilePath.Length == 0)
                continue;
            byFile[tool.FilePath] = new FileChange(tool.FilePath, tool.FileDiff, tool.Additions, tool.Deletions);
        }
        var ordered = byFile.Values.ToList();
        Changes.Clear();
        foreach (var change in ordered)
            Changes.Add(change);
        OnPropertyChanged(nameof(HasChanges));
    }

    public bool HasChanges => Changes.Count > 0;

    private DateTime? CreatedAtFor(string? messageId) =>
        messageId is not null && _createdByMessage.TryGetValue(messageId, out var ms) && ms > 0
            ? FromMs(ms)
            : null;

    private static DateTime? FromMs(long ms) =>
        ms > 0 ? DateTimeOffset.FromUnixTimeMilliseconds(ms).ToLocalTime().DateTime : null;

    private string MetaFor(string? messageId) =>
        messageId is not null && _metaByMessage.TryGetValue(messageId, out var meta) ? meta : "";

    /// <summary>Compact per-message detail line: model, token flow, cost.</summary>
    private static string? BuildMeta(JsonElement info)
    {
        if (info.ValueKind != JsonValueKind.Object)
            return null;
        var parts = new List<string>();
        var modelId = info.TryGetProperty("modelID", out var mid) ? mid.GetString() : null;
        var providerId = info.TryGetProperty("providerID", out var pid) ? pid.GetString() : null;
        if (!string.IsNullOrEmpty(modelId))
            parts.Add(!string.IsNullOrEmpty(providerId) ? $"{providerId}/{modelId}" : modelId);
        if (info.TryGetProperty("tokens", out var tokens) && tokens.ValueKind == JsonValueKind.Object)
        {
            var input = tokens.TryGetProperty("input", out var tin) && tin.TryGetInt64(out var inCount) ? inCount : 0;
            var output = tokens.TryGetProperty("output", out var tout) && tout.TryGetInt64(out var outCount) ? outCount : 0;
            if (input > 0 || output > 0)
                parts.Add($"↑{FormatTokens(input)} ↓{FormatTokens(output)}");
        }
        if (info.TryGetProperty("cost", out var cost) && cost.TryGetDouble(out var value) && value > 0)
            parts.Add($"${value:0.####}");
        return parts.Count > 0 ? string.Join(" · ", parts) : null;
    }

    private static string? ReadSessionId(JsonElement data) =>
        data.TryGetProperty("sessionID", out var sid) ? sid.GetString() : null;

    private void SetRunning(string? sessionId, bool running)
    {
        if (string.IsNullOrEmpty(sessionId) || !IsAppSession(sessionId))
            return;
        var changed = running ? _runningBySession.Add(sessionId) : _runningBySession.Remove(sessionId);
        if (!changed)
            return;
        var existing = RunningAgents.FirstOrDefault(r => r.SessionId == sessionId);
        if (running)
        {
            if (existing is null)
            {
                var workspace = _workspaceBySession.TryGetValue(sessionId, out var path) &&
                                Workspaces.FirstOrDefault(w => w.FolderPath == path) is { } wsItem
                    ? wsItem.Name
                    : "";
                RunningAgents.Add(new RunningAgentItem(sessionId, workspace, TitleFor(sessionId))
                {
                    NeedsAttention = _pendingBySession.ContainsKey(sessionId),
                });
            }
        }
        else if (existing is not null)
        {
            RunningAgents.Remove(existing);
        }
        NotifyBusyChanged();
    }

    private void OnTodosUpdated(JsonElement data)
    {
        if (!IsOurSession(data))
            return;
        Tasks.Clear();
        if (!data.TryGetProperty("todos", out var todos) || todos.ValueKind != JsonValueKind.Array)
            return;
        foreach (var todo in todos.EnumerateArray())
        {
            var content = todo.TryGetProperty("content", out var c) ? c.GetString() : null;
            if (string.IsNullOrEmpty(content))
                continue;
            var status = todo.TryGetProperty("status", out var st) ? st.GetString() ?? "pending" : "pending";
            Tasks.Add(new TaskItem(content, status));
        }
    }

    /// <summary>Last todowrite tool part in the restored transcript carries the session's todos.</summary>
    private static List<TaskItem>? TodosFromPart(JsonElement part)
    {
        JsonElement list = default;
        if (part.TryGetProperty("state", out var state))
        {
            if (state.TryGetProperty("input", out var input) &&
                input.TryGetProperty("todos", out var inputTodos) &&
                inputTodos.ValueKind == JsonValueKind.Array)
                list = inputTodos;
            else if (state.TryGetProperty("metadata", out var md) &&
                     md.ValueKind == JsonValueKind.Object &&
                     md.TryGetProperty("todos", out var mdTodos) &&
                     mdTodos.ValueKind == JsonValueKind.Array)
                list = mdTodos;
        }
        if (list.ValueKind != JsonValueKind.Array)
            return null;
        var result = new List<TaskItem>();
        foreach (var todo in list.EnumerateArray())
        {
            var content = todo.TryGetProperty("content", out var c) ? c.GetString() : null;
            if (string.IsNullOrEmpty(content))
                continue;
            var status = todo.TryGetProperty("status", out var st) ? st.GetString() ?? "pending" : "pending";
            result.Add(new TaskItem(content, status));
        }
        return result;
    }

    private void OnPermissionAsked(JsonElement data, bool v2)
    {
        var id = data.TryGetProperty("id", out var i) ? i.GetString() : null;
        var sessionId = data.TryGetProperty("sessionID", out var s) ? s.GetString() : null;
        if (id is null || sessionId is null)
            return;

        string kind, detail;
        string? diff = null;
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
            if (data.TryGetProperty("metadata", out var md) && md.ValueKind == JsonValueKind.Object)
            {
                if (md.TryGetProperty("command", out var c) && c.GetString() is { Length: > 0 } command)
                    detail = command;
                if (md.TryGetProperty("diff", out var d) && d.GetString() is { Length: > 0 } patch)
                    diff = patch;
            }
            if (detail.Length == 0 && data.TryGetProperty("patterns", out var pats) && pats.ValueKind == JsonValueKind.Array)
                detail = string.Join("\n", pats.EnumerateArray().Select(x => x.GetString() ?? "").Where(x => x.Length > 0));
        }
        if (detail.Length == 0)
            detail = "(no detail provided)";

        var item = new PermissionItem
        {
            Id = id,
            SessionId = sessionId,
            Kind = kind,
            Detail = detail,
            Diff = diff,
            IsV2 = v2,
            Respond = RespondToPermission,
        };
        if (sessionId == _sessionId)
        {
            ChatItems.Add(item);
            if (AutoAllow)
                _ = RespondToPermission(item, "always", isAuto: true);
            return;
        }
        // another chat's ask: answer it from its own workspace's server, or park it until opened
        if (!IsAppSession(sessionId))
            return;
        if (AutoAllow)
        {
            _ = RespondToPermission(item, "always", isAuto: true);
            return;
        }
        if (!_pendingBySession.TryGetValue(sessionId, out var pending))
            _pendingBySession[sessionId] = pending = [];
        pending.Add(item);
        MarkAttention(sessionId, true);
    }

    private void FlushPendingPermissions(string sessionId)
    {
        if (!_pendingBySession.Remove(sessionId, out var pending))
            return;
        foreach (var item in pending)
            ChatItems.Add(item);
        MarkAttention(sessionId, false);
    }

    private void MarkAttention(string sessionId, bool attention)
    {
        var item = RunningAgents.FirstOrDefault(r => r.SessionId == sessionId);
        if (item is not null)
            item.NeedsAttention = attention;
    }

    private void OnPermissionReplied(JsonElement data)
    {
        var requestId = data.TryGetProperty("requestID", out var r) ? r.GetString() : null;
        if (requestId is null)
            return;
        foreach (var item in ChatItems.OfType<PermissionItem>())
            if (item.Id == requestId && item.IsPending)
                item.SetAnswer("answered elsewhere", auto: false);
        foreach (var list in _pendingBySession.Values)
            foreach (var item in list)
                if (item.Id == requestId && item.IsPending)
                    item.SetAnswer("answered elsewhere", auto: false);
    }

    private async Task RespondToPermission(PermissionItem item, string response, bool isAuto)
    {
        if (ApiFor(item.SessionId) is not { } api)
        {
            item.SetAnswer("server unavailable", isAuto);
            return;
        }
        // close the buttons immediately so a slow reply cannot be clicked twice
        item.SetAnswer(isAuto ? "auto-allowing…" : "sending…", isAuto);
        try
        {
            if (item.IsV2)
                await api.ReplyV2Async(item.Id, response, CancellationToken.None);
            else
                await api.RespondPermissionAsync(item.SessionId, item.Id, response, CancellationToken.None);
            var label = response switch
            {
                "once" => "allowed (once)",
                "always" => isAuto ? "auto-allowed (always)" : "always allowed",
                _ => "denied",
            };
            item.SetAnswer(label, isAuto);
        }
        catch (Exception ex)
        {
            item.SetAnswer($"reply failed: {ex.Message}", isAuto);
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
            var head = $"Error: {name ?? "opencode"}{(message is { Length: > 0 } ? $" — {message}" : "")}";
            var extras = new StringBuilder();
            foreach (var prop in error.EnumerateObject())
            {
                if (prop.Name is "name" or "message")
                    continue;
                var raw = prop.Value.ValueKind == JsonValueKind.String ? prop.Value.GetString() : prop.Value.GetRawText();
                if (string.IsNullOrEmpty(raw))
                    continue;
                extras.Append('\n').Append(prop.Name).Append(": ").Append(raw);
            }
            var detail = extras.ToString();
            if (detail.Length > 2000)
                detail = detail[..2000] + "\n… (truncated)";
            return detail.Length > 0 ? head + detail : head;
        }
        return "Session error.";
    }

    private void ClearTranscript()
    {
        _roleByMessage.Clear();
        _createdByMessage.Clear();
        _metaByMessage.Clear();
        _textsByMessage.Clear();
        _textByPart.Clear();
        _toolByPart.Clear();
        _reasoningByPart.Clear();
        _imageByPart.Clear();
        ChatItems.Clear();
        Tasks.Clear();
        _lastUserMessageId = null;
        _lastUsedModel = null;
        _canRedo = false;
        _contextTokens = 0;
        _contextLimit = 0;
        ContextText = null;
        OnPropertyChanged(nameof(CanRedo));
        RefreshChanges();
    }

    private void AddNote(string text, bool error = false) => ChatItems.Add(new SystemNoteItem(text, error));

    private void PostNote(string text, bool error = false) => Dispatcher.UIThread.Post(() => AddNote(text, error));

    /// <summary>Errors for a background chat land in the open transcript tagged with that chat's title.</summary>
    private void PostNote(string sessionId, string text, bool error = false) =>
        PostNote(sessionId == _sessionId ? text : $"{TitleFor(sessionId)}: {text}", error);

    /// <summary>Surface-level messages from the view (clipboard, drag-drop) land in the transcript.</summary>
    public void Announce(string text, bool error = false) => AddNote(text, error);
}

/// <summary>One touched file in the Changes flyout; Patch is the unified diff when known.</summary>
public sealed record FileChange(string FilePath, string? Patch, int Additions, int Deletions)
{
    public string Name => Path.GetFileName(FilePath);
    public bool HasPatch => Patch is { Length: > 0 };
}

/// <summary>A chat whose turn is currently running; shown in the side panel, any workspace.</summary>
public sealed class RunningAgentItem(string sessionId, string workspace, string title) : ObservableObject
{
    private string _title = title;
    private bool _needsAttention;

    public string SessionId { get; } = sessionId;
    public string Workspace { get; } = workspace;
    public string Title { get => _title; set => SetProperty(ref _title, value); }

    /// <summary>A permission ask is waiting in this background chat (auto-allow off).</summary>
    public bool NeedsAttention { get => _needsAttention; set => SetProperty(ref _needsAttention, value); }
}
