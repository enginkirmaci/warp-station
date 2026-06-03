using Avalonia.Controls;
using Avalonia.VisualTree;
using Avalonia.LogicalTree;

namespace Tools.SnapIt.Extensions;

public static class DependencyObjectExtensions
{
    public static IEnumerable<T> FindChildren<T>(this Visual depObj) where T : Visual
    {
        if (depObj != null)
        {
            foreach (var child in depObj.GetVisualChildren())
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

    public static IEnumerable<T> FindChildren<T>(this Visual depObj, string childName) where T : Visual
    {
        var collection = depObj.FindChildren<T>();

        foreach (var child in collection)
        {
            var frameworkElement = child as Control;

            if (frameworkElement != null && frameworkElement.Name == childName)
            {
                yield return child;
            }
        }
    }

    public static T FindChild<T>(this Visual depObj, string childName) where T : Visual
    {
        var collection = depObj.FindChildren<T>();
        T foundChild = null;

        foreach (var child in collection)
        {
            var frameworkElement = child as Control;

            if (frameworkElement != null && frameworkElement.Name == childName)
            {
                foundChild = child;
                break;
            }
        }

        return foundChild;
    }

    public static T FindParent<T>(this Visual child) where T : Visual
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
        else
        {
            return FindParent<T>(parent);
        }
    }

    public static T FindParent<T>(this AvaloniaObject child) where T : Visual
    {
        if (child is Visual visual)
        {
            return FindParent<T>(visual);
        }
        return null;
    }
}
