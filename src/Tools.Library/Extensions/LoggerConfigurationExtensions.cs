using Serilog;

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
}
