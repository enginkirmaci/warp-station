using System.Collections.Concurrent;
using Serilog.Core;
using Serilog.Events;
using Serilog.Formatting.Display;

namespace Tools.Library.Services.Logging;

/// <summary>
/// An in-memory <see cref="ILogEventSink"/> that retains the most recent log entries for the
/// in-app log panel. Thread-safe (the Serilog pipeline calls <see cref="Emit"/> from arbitrary
/// background threads). Raises <see cref="EntryAppended"/> on each new entry so the UI can
/// refresh; handlers are expected to marshal onto the UI thread themselves.
/// </summary>
public sealed class MemoryLogSink : ILogEventSink
{
    /// <summary>Maximum number of entries retained. Older entries are dropped as new ones arrive.</summary>
    private const int Capacity = 500;

    private readonly ConcurrentQueue<LogEntry> _entries = new();
    private readonly MessageTemplateTextFormatter _formatter = new("{Message:lj}", null);

    /// <summary>Raised (on the logging thread) whenever a new entry is appended.</summary>
    public event Action<LogEntry>? EntryAppended;

    /// <summary>A snapshot of the current entries, oldest-first.</summary>
    public IReadOnlyList<LogEntry> Entries => _entries.ToArray();

    public void Emit(LogEvent logEvent)
    {
        var message = new StringWriter();
        _formatter.Format(logEvent, message);

        var entry = new LogEntry
        {
            Timestamp = logEvent.Timestamp,
            Level = logEvent.Level,
            Message = message.ToString().TrimEnd(),
            Exception = logEvent.Exception is null
                ? null
                : $"{logEvent.Exception.GetType().Name}: {logEvent.Exception.Message}",
        };

        _entries.Enqueue(entry);

        // Bounded: drop the oldest entries past capacity.
        while (_entries.Count > Capacity && _entries.TryDequeue(out _)) { }

        EntryAppended?.Invoke(entry);
    }

    /// <summary>Clears all retained entries.</summary>
    public void Clear() => _entries.Clear();
}
