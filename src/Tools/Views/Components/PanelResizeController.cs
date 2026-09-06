using System;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;

namespace Tools.Views.Components;

/// <summary>
/// Pointer logic for a panel-top drag divider: dragging the grip Border vertically
/// applies deltas to the panel height through <c>apply</c>. Shared by every resizable
/// bottom panel (the bar's expandable panel, the OpenCode panel) so the feel stays
/// identical: pointer-move bursts are COALESCED into one height update per UI-thread
/// render pass (each height change re-measures the whole panel — per-event application
/// stutters), deltas accumulate across events, values round to whole logical pixels
/// downstream, and a lost pointer capture ends the drag.
/// </summary>
public sealed class PanelResizeController
{
    private readonly Border _grip;
    private readonly Control _relativeTo;
    private readonly Action<double> _apply;

    private bool _dragging;
    private double _lastY;
    private double _pendingDelta;
    private bool _applyScheduled;

    private PanelResizeController(Border grip, Control relativeTo, Action<double> apply)
    {
        _grip = grip;
        _relativeTo = relativeTo;
        _apply = apply;
    }

    /// <summary>Wires a grip Border up. The returned controller lives as long as the grip.</summary>
    public static PanelResizeController Attach(Border grip, Control relativeTo, Action<double> apply)
    {
        var controller = new PanelResizeController(grip, relativeTo, apply);
        grip.PointerPressed += controller.OnPressed;
        grip.PointerMoved += controller.OnMoved;
        grip.PointerReleased += controller.OnReleased;
        grip.PointerCaptureLost += controller.OnCaptureLost;
        return controller;
    }

    private void OnPressed(object? sender, PointerPressedEventArgs e)
    {
        _dragging = true;
        // Fixed reference (the control doesn't move during the drag), so the raw
        // delta is the pointer's travel.
        _lastY = e.GetPosition(_relativeTo).Y;
        _pendingDelta = 0;
        e.Pointer.Capture(_grip);
        e.Handled = true;
    }

    private void OnMoved(object? sender, PointerEventArgs e)
    {
        if (!_dragging || !e.GetCurrentPoint(_grip).Properties.IsLeftButtonPressed)
        {
            _dragging = false;
            return;
        }

        var y = e.GetPosition(_relativeTo).Y;
        _pendingDelta += _lastY - y; // dragging up grows the panel
        _lastY = y;
        ScheduleApply();
        e.Handled = true;
    }

    private void OnReleased(object? sender, PointerReleasedEventArgs e)
    {
        _dragging = false;
        e.Pointer.Capture(null);
        e.Handled = true;
    }

    private void OnCaptureLost(object? sender, PointerCaptureLostEventArgs e) => _dragging = false;

    private void ScheduleApply()
    {
        if (_applyScheduled) return;
        _applyScheduled = true;
        Dispatcher.UIThread.Post(
            () =>
            {
                _applyScheduled = false;
                if (!_dragging || Math.Abs(_pendingDelta) < 0.5) return;
                _apply(_pendingDelta);
                _pendingDelta = 0;
            },
            DispatcherPriority.Render);
    }
}
