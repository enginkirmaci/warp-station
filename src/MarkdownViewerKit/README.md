# MarkdownViewerKit.Avalonia

Configurable interactive Markdown viewer control for [Avalonia](https://avaloniaui.net) — built on the Markdown.Avalonia engine (ClassIsland Tight fork) with an Obsidian-style default theme, live task-list checkboxes and text selection.

## Install

```sh
dotnet add package MarkdownViewerKit.Avalonia
```

## Quick start

```xml
<UserControl xmlns="https://github.com/avaloniaui"
             xmlns:mk="using:MarkdownViewerKit">
    <mk:MarkdownViewer Markdown="{Binding DocumentText, Mode=OneWay}"
                       InteractiveTaskLists="True"
                       TaskToggled="OnTaskToggled" />
</UserControl>
```

The control is drop-in: the default theme is merged into the control itself, no app-level style includes needed.

## Configuration

| Property | Default | Description |
|---|---|---|
| `Markdown` | `""` | The markdown source. While a task preview is active the property value holds the transformed preview text — bind **OneWay** and keep the source in your view model. |
| `InteractiveTaskLists` | `false` | Renders `- [ ]` / `- [x]` items as live checkboxes. Clicks raise `TaskToggled`; update the source text (e.g. via `MarkdownTasks.Toggle`) to apply. |
| `TaskToggled` (event) | — | `(line, isChecked)` — zero-based source line and the state after the click. |
| `HideTaskListMarkers` | `true` | Hides the list bullet cell of a task row. |
| `SelectionHitTestFix` | `true` | Keeps the engine's selection overlay from blocking clicks under a selection band. Depends on the engine's internal template shape; disable on engine upgrades that move it. |
| `TaskAccentBrush` / `TaskAccentHoverBrush` | theme tokens | Checked checkbox fill/border and hover border. |
| `SelectionEnabled` / `SelectionBrush` | `true` / `#4D6E9BF7` | Text selection (inherited from the engine; instance defaults here). |
| `SaveScrollValueWhenContentUpdated` | `true` | Keeps scroll position across re-renders. |

All other engine properties (`AssetPathRoot`, `Plugins`, `HyperlinkCommand`, `MarkdownStyle`, …) are inherited unchanged.

## Theming

Colors are exposed as resource tokens (dark defaults, `Light` variant via theme dictionaries):

`MarkwingTextBrush`, `MarkwingMutedBrush`, `MarkwingLinkBrush`, `MarkwingLinkHoverBrush`, `MarkwingAccentBrush`, `MarkwingAccentHoverBrush`, `MarkwingCodeBrush`, `MarkwingBorderBrush`, `MarkwingNoteBrush`, `MarkwingSelectionBrush`.

Override per instance:

```xml
<mk:MarkdownViewer.Resources>
    <SolidColorBrush x:Key="MarkwingAccentBrush">#FF8800</SolidColorBrush>
</mk:MarkdownViewer.Resources>
```

For a full restyle, add your own styles against the engine's `.Markdown_Avalonia_MarkdownViewer` class selectors (they must stay class-anchored to beat the engine's theme frame).

## License

MIT
