using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Tools.Library.Services;

namespace Tools.Views.Components.History;

/// <summary>
/// Renders raw unified-diff text as colored rows (see the XAML header). The whole
/// control is a dumb text sink: the caller re-feeds <see cref="Text"/> whenever its
/// patch lands, and every change reparses into the typed row list the template
/// colors. Bind Text OneWay — the viewer never writes back.
/// </summary>
public partial class DiffViewer : UserControl
{
    public static readonly StyledProperty<string?> TextProperty =
        AvaloniaProperty.Register<DiffViewer, string?>(nameof(Text));

    public static readonly StyledProperty<IReadOnlyList<DiffLine>> LinesProperty =
        AvaloniaProperty.Register<DiffViewer, IReadOnlyList<DiffLine>>(
            nameof(Lines), Array.Empty<DiffLine>());

    public static readonly StyledProperty<bool> HasLinesProperty =
        AvaloniaProperty.Register<DiffViewer, bool>(nameof(HasLines));

    public DiffViewer()
    {
        InitializeComponent();
    }

    public string? Text
    {
        get => GetValue(TextProperty);
        set => SetValue(TextProperty, value);
    }

    /// <summary>The parsed rows behind <see cref="Text"/>.</summary>
    public IReadOnlyList<DiffLine> Lines => GetValue(LinesProperty);

    /// <summary>Whether any rows exist (gates the empty note).</summary>
    public bool HasLines => GetValue(HasLinesProperty);

    static DiffViewer()
    {
        TextProperty.Changed.AddClassHandler<DiffViewer>((viewer, _) => viewer.Reparse());
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    private void Reparse()
    {
        var lines = DiffLineParser.Parse(Text);
        SetValue(LinesProperty, lines);
        SetValue(HasLinesProperty, lines.Count > 0);
    }
}
