namespace Tools.Library.Media;

/// <summary>
/// Shared accent hex palette for the cached chip/status brushes (branch pills in
/// the Repos table, git status rows and GitHub state/label chips in the bottom
/// bar). Each accent hue is declared once here in its full-strength and tinted
/// ARGB forms; the converters build their cached singletons from these strings,
/// so a retint stays a one-file change and sibling chips can never drift apart.
/// <para>
/// String constants — the converters parse them once at static init into
/// <c>ImmutableSolidColorBrush</c>/<c>SolidColorBrush</c> singletons, so nothing
/// here runs on the per-row hot path.
/// </para>
/// </summary>
public static class ChipPalette
{
    /// <summary>Muted gray — the fallback for anything unmatched.</summary>
    public const string Gray = "#B4B4B4";

    /// <summary>Low-alpha (~15%) gray wash — fallback tinted chip backgrounds.</summary>
    public const string GrayTint = "#26B4B4B4";

    // Green (#228B22): added / approved / community labels / main branch.

    /// <summary>Near-opaque green — chip text and glyphs.</summary>
    public const string GreenStrong = "#CC228B22";

    /// <summary>Low-alpha green — tinted chip backgrounds.</summary>
    public const string GreenTint = "#26228B22";

    // Red (#F5222D): deleted rows / bug labels.

    /// <summary>Red at the git status rows' softer alpha.</summary>
    public const string RedDeleted = "#BBF5222D";

    /// <summary>Near-opaque red — chip text and glyphs.</summary>
    public const string RedStrong = "#CCF5222D";

    /// <summary>Low-alpha red — tinted chip backgrounds.</summary>
    public const string RedTint = "#26F5222D";

    // Amber (#FA8C16): modified / performance labels / release branch.

    /// <summary>Near-opaque amber — chip text and glyphs.</summary>
    public const string AmberStrong = "#CCFA8C16";

    /// <summary>Low-alpha amber — tinted chip backgrounds.</summary>
    public const string AmberTint = "#26FA8C16";

    // Purple (#8B5CF6): review / master branch / enhancement labels.

    /// <summary>Full-strength purple — chip text and glyphs.</summary>
    public const string Purple = "#8B5CF6";

    /// <summary>Near-opaque purple — chip text and glyphs.</summary>
    public const string PurpleStrong = "#CC8B5CF6";

    /// <summary>Low-alpha purple — tinted chip backgrounds.</summary>
    public const string PurpleTint = "#268B5CF6";

    // Documentation blue (#3B82F6): GitHub documentation labels only.

    /// <summary>Near-opaque documentation blue — chip text and glyphs.</summary>
    public const string BlueStrong = "#CC3B82F6";

    /// <summary>Low-alpha documentation blue — tinted chip backgrounds.</summary>
    public const string BlueTint = "#263B82F6";

    // Develop blue (#2F54EB): a distinct blue reserved for develop-family branches.

    /// <summary>Near-opaque develop blue — chip text and glyphs.</summary>
    public const string DevelopBlueStrong = "#CC2F54EB";

    // Bright variants — the strong hues above are tuned for light surfaces and read
    // muddy on the dark theme; pill text/glyphs and tint backgrounds use these
    // higher-luminance versions of the same hues instead (still saturated — NOT
    // pastel). Tints derive from them at ~0.18 alpha.

    /// <summary>Bright green — readable green on dark surfaces.</summary>
    public const string GreenBright = "#4EC94E";

    /// <summary>Bright amber — readable amber on dark surfaces.</summary>
    public const string AmberBright = "#FFAB40";

    /// <summary>Bright purple — readable purple on dark surfaces.</summary>
    public const string PurpleBright = "#A78BFA";

    /// <summary>Bright blue — readable blue on dark surfaces.</summary>
    public const string BlueBright = "#6E9BF7";
}
