namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides clipboard operations.
/// </summary>
public interface IClipboardService
{
    /// <summary>
    /// Copies the specified text to the clipboard. Fire-and-forget: failures are logged,
    /// never thrown (safe to call without awaiting).
    /// </summary>
    /// <param name="text">The text to copy.</param>
    void CopyText(string text);

    /// <summary>
    /// Copies the specified text to the clipboard and completes once the write has been
    /// dispatched, so callers can await it and raise their own feedback afterwards.
    /// Clipboard failures are logged, not thrown.
    /// </summary>
    /// <param name="text">The text to copy.</param>
    Task CopyTextAsync(string text);
}
