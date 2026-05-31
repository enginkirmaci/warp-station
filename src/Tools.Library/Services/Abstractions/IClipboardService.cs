namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Provides clipboard operations.
/// </summary>
public interface IClipboardService
{
    /// <summary>
    /// Copies the specified text to the clipboard.
    /// </summary>
    /// <param name="text">The text to copy.</param>
    void CopyText(string text);
}
