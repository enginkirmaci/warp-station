using System.IO;
using Avalonia.Media.Imaging;
using CommunityToolkit.Mvvm.ComponentModel;

namespace OpenCodeAgent.ViewModels;

public sealed class AttachmentItem(string fileName, byte[] png) : ObservableObject
{
    public string FileName { get; } = fileName;

    public byte[] Png { get; } = png;

    public Bitmap Preview { get; } = Decode(png);

    private static Bitmap Decode(byte[] png)
    {
        using var ms = new MemoryStream(png);
        return Bitmap.DecodeToWidth(ms, 96);
    }
}
