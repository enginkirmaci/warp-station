using Avalonia;
using Avalonia.Controls;
using Avalonia.VisualTree;

namespace Tools.Library.Extensions;

public static class DependencyObjectExtensions
{
    public static IEnumerable<T> FindChildren<T>(this Visual visual) where T : Visual
    {
        if (visual != null)
        {
            foreach (var child in visual.GetVisualChildren())
            {
                if (child is T t)
                {
                    yield return t;
                }

                foreach (T childOfChild in FindChildren<T>(child))
                {
                    yield return childOfChild;
                }
            }
        }
    }

    public static IEnumerable<T> FindChildren<T>(this Visual visual, string childName) where T : Control
    {
        var collection = FindChildren<T>(visual);

        foreach (var child in collection)
        {
            if (child.Name == childName)
            {
                yield return child;
            }
        }
    }

    public static T? FindChild<T>(this Visual visual, string childName) where T : Control
    {
        var collection = FindChildren<T>(visual);

        foreach (var child in collection)
        {
            if (child.Name == childName)
            {
                return child;
            }
        }

        return null;
    }

    public static T? FindParent<T>(this Visual child) where T : Visual
    {
        var parent = child.GetVisualParent();

        if (parent == null)
        {
            return null;
        }

        if (parent is T t)
        {
            return t;
        }

        return FindParent<T>(parent);
    }
}