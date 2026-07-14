using Serilog.Events;

namespace Tools.Library.Services.Logging;

/// <summary>
/// A single log entry captured by <see cref="MemoryLogSink"/> for the in-app log panel.
/// Immutable snapshot of a Serilog <see cref="LogEvent"/>.
/// </summary>
public sealed class LogEntry
{
    /// <summary>The UTC timestamp of the log event.</summary>
    public DateTimeOffset Timestamp { get; init; }

    /// <summary>The severity level (Verbose/Debug/Information/Warning/Error/Fatal).</summary>
    public LogEventLevel Level { get; init; }

    /// <summary>The rendered message (template + properties applied).</summary>
    public string Message { get; init; } = string.Empty;

    /// <summary>The optional exception type+message, when the event carried an exception.</summary>
    public string? Exception { get; init; }

    /// <summary>Convenience: the <see cref="Level"/> as a display string.</summary>
    public string LevelText => Level switch
    {
        LogEventLevel.Verbose => "VRB",
        LogEventLevel.Debug => "DBG",
        LogEventLevel.Information => "INF",
        LogEventLevel.Warning => "WRN",
        LogEventLevel.Error => "ERR",
        LogEventLevel.Fatal => "FTL",
        _ => Level.ToString(),
    };
}
