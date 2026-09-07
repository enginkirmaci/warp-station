using System;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Interactivity;
using Avalonia.Threading;

namespace Tools.Views.Components;

/// <summary>The drag axis a resize grip works on.</summary>
public enum PanelResizeAxis
{
    /// <summary>Vertical drag adjusts height (bottom panels: dragging up grows).</summary>
    Vertical,

    /// <summary>Horizontal drag adjusts width (right-docked drawers: dragging left grows).</summary>
    Horizontal
}

/// <summary>
/// Pointer logic for a panel drag divider: dragging the grip Border applies deltas to
/// the panel size through <c>apply</c> along the configured <see cref="PanelResizeAxis"/>
/// (the bottom panels' top edge vertically, the tool drawer's left edge horizontally).
/// Shared by every resizable panel so the feel stays identical: pointer-move bursts are
/// COALESCED into one size update per UI-thread render pass (each size change
/// re-measures the whole panel — per-event application stutters), deltas accumulate
/// across events, values round to whole logical pixels downstream, and a lost pointer
/// capture ends the drag.
/// </summary>
public sealed class PanelResizeController
{
    private readonly Border _grip;
    private readonly Control _relativeTo;
    private readonly Action<double> _apply;
    private readonly PanelResizeAxis _axis;

    private bool _dragging;
    private double _lastPosition;
    private double _pendingDelta;
    private bool _applyScheduled;

    private PanelResizeController(Border grip, Control relativeTo, Action<double> apply, PanelResizeAxis axis)
    {
        _grip = grip;
        _relativeTo = relativeTo;
        _apply = apply;
        _axis = axis;
    }

    /// <summary>Wires a grip Border up. The returned controller lives as long as the grip.</summary>
    public static PanelResizeController Attach(
        Border grip,
        Control relativeTo,
        Action<double> apply,
        PanelResizeAxis axis = PanelResizeAxis.Vertical)
    {
        var controller = new PanelResizeController(grip, relativeTo, apply, axis);
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
        _lastPosition = PointerPosition(e);
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

        var position = PointerPosition(e);
        _pendingDelta += _lastPosition - position; // up (vertical) / left (horizontal) grows the panel
        _lastPosition = position;
        ScheduleApply();
        e.Handled = true;
    }

    private double PointerPosition(PointerEventArgs e)
    {
        var position = e.GetPosition(_relativeTo);
        return _axis == PanelResizeAxis.Vertical ? position.Y : position.X;
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
