using Serilog;
using Serilog.Core;
using Serilog.Events;
using Tools.Library.Services.Logging;

namespace Tools.Library.Extensions;

/// <summary>
/// Shared Serilog bootstrap configuration for the application.
/// Centralizes the minimum log level and file sink so each host (Tools, DevTools)
/// configures logging consistently instead of re-typing the boilerplate.
/// </summary>
public static class LoggerConfigurationExtensions
{
    /// <summary>
    /// Configures a <see cref="LoggerConfiguration"/> with the application's standard
    /// minimum level (Error) and a daily rolling file sink at <paramref name="logFilePath"/>.
    /// </summary>
    /// <param name="loggerConfiguration">The logger configuration to extend.</param>
    /// <param name="logFilePath">Portable, OS-agnostic path to the rolling log file.</param>
    /// <returns>The configured logger configuration for chaining.</returns>
    public static LoggerConfiguration WriteToFileDaily(
        this LoggerConfiguration loggerConfiguration,
        string logFilePath)
    {
        return loggerConfiguration
            .MinimumLevel.Error()
            .WriteTo.File(logFilePath, rollingInterval: RollingInterval.Day);
    }

    /// <summary>
    /// Configures a <see cref="LoggerConfiguration"/> for the GUI host: the global minimum is
    /// <see cref="LogEventLevel.Information"/> so the in-app log panel (fed by
    /// <paramref name="memorySink"/>) captures serve/approval activity, while the daily file
    /// sink is independently restricted to <see cref="LogEventLevel.Error"/> via a
    /// <see cref="LevelFilter"/> so the on-disk log stays terse as before.
    /// </summary>
    /// <param name="loggerConfiguration">The logger configuration to extend.</param>
    /// <param name="logFilePath">Portable, OS-agnostic path to the rolling log file.</param>
    /// <param name="memorySink">The in-memory sink feeding the log panel.</param>
    /// <returns>The configured logger configuration for chaining.</returns>
    public static LoggerConfiguration WriteToFileDailyWithMemory(
        this LoggerConfiguration loggerConfiguration,
        string logFilePath,
        MemoryLogSink memorySink)
    {
        return loggerConfiguration
            .MinimumLevel.Information()
            .WriteTo.Sink(memorySink)
            .WriteTo.Logger(lc => lc
                .Filter.With(new LevelFilter(LogEventLevel.Error))
                .WriteTo.File(logFilePath, rollingInterval: RollingInterval.Day));
    }

    /// <summary>
    /// A simple minimum-level filter (Serilog's built-in <c>MinimumLevel</c> only applies at
    /// the pipeline root, not to a sub-logger; this filter lets a sub-logger enforce its own floor).
    /// </summary>
    private sealed class LevelFilter(LogEventLevel minimum) : ILogEventFilter
    {
        private readonly LogEventLevel _minimum = minimum;

        public bool IsEnabled(LogEvent logEvent) => logEvent.Level >= _minimum;
    }
}

