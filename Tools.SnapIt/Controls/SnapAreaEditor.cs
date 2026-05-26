using System.Windows;
using System.Windows.Controls;
using System.Windows.Input;
using Tools.SnapIt.Common.Entities;
using Tools.SnapIt.Common.Extensions;
using Tools.SnapIt.Common.Mvvm;
using Point = System.Windows.Point;
using Size = System.Windows.Size;

namespace Tools.SnapIt.Controls;

public class SnapAreaEditor : Control
{
    public SnapControl SnapControl { get; set; }

    public Thickness AreaPadding
    {
        get => (Thickness)GetValue(AreaPaddingProperty);
        set => SetValue(AreaPaddingProperty, value);
    }

    public static readonly DependencyProperty AreaPaddingProperty
     = DependencyProperty.Register("AreaPadding", typeof(Thickness), typeof(SnapAreaEditor),
       new FrameworkPropertyMetadata()
       {
           DefaultValue = new Thickness(0),
           BindsTwoWayByDefault = true,
           PropertyChangedCallback = new PropertyChangedCallback(AreaPaddingPropertyChanged)
       });

    private static void AreaPaddingPropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        var snapAreaEditor = (SnapAreaEditor)d;
        snapAreaEditor.AreaPadding = (Thickness)e.NewValue;
    }

    public bool IsAreaMouseOver
    {
        get => (bool)GetValue(IsAreaMouseOverProperty);
        set => SetValue(IsAreaMouseOverProperty, value);
    }

    public static readonly DependencyProperty IsAreaMouseOverProperty = DependencyProperty.Register("IsAreaMouseOver",
        typeof(bool), typeof(SnapAreaEditor), new PropertyMetadata(null));

    public SnapAreaTheme Theme
    {
        get => (SnapAreaTheme)GetValue(ThemeProperty);
        set => SetValue(ThemeProperty, value);
    }

    public static readonly DependencyProperty ThemeProperty
     = DependencyProperty.Register("Theme", typeof(SnapAreaTheme), typeof(SnapAreaEditor),
       new FrameworkPropertyMetadata()
       {
           BindsTwoWayByDefault = true,
           PropertyChangedCallback = new PropertyChangedCallback(ThemePropertyChanged)
       });

    private static void ThemePropertyChanged(DependencyObject d, DependencyPropertyChangedEventArgs e)
    {
        var SnapAreaEditor = (SnapAreaEditor)d;
        SnapAreaEditor.Theme = (SnapAreaTheme)e.NewValue;

        //if (SnapAreaEditor.Theme != null)
        //{
        //    SnapAreaEditor.Area.Opacity = SnapAreaEditor.Theme.Opacity;
        //    SnapAreaEditor.Area.Background = SnapAreaEditor.Theme.OverlayBrush;
        //}
    }

    public static readonly DependencyProperty SplitVerticallyCommandProperty =
        DependencyProperty.Register("SplitVerticallyCommand",
            typeof(ICommand), typeof(SnapAreaEditor), new PropertyMetadata(null));

    private ICommand SplitVerticallyCommand => (ICommand)GetValue(SplitHorizantallyCommandProperty);

    public static readonly DependencyProperty SplitHorizantallyCommandProperty =
        DependencyProperty.Register("SplitHorizantallyCommand",
            typeof(ICommand), typeof(SnapAreaEditor), new PropertyMetadata(null));

    public ICommand SplitHorizantallyCommand => (ICommand)GetValue(SplitHorizantallyCommandProperty);

    public SnapAreaEditor()
    {
        SetValue(SplitVerticallyCommandProperty,
            new DelegateCommand<object>(o =>
            {
                Split(SplitDirection.Vertical);
            }));

        SetValue(SplitHorizantallyCommandProperty,
            new DelegateCommand<object>(o =>
            {
                Split(SplitDirection.Horizontal);
            }));

        //DesignPanel.Visibility = Visibility.Hidden;

        Loaded += SnapAreaEditor_Loaded;
    }

    private void SnapAreaEditor_Loaded(object sender, RoutedEventArgs e)
    {
        var area = this.FindChild<Grid>("Area");
        if (area != null)
        {
            area.IsMouseDirectlyOverChanged += SnapAreaEditor_IsMouseDirectlyOverChanged;
        }
    }

    private void SnapAreaEditor_IsMouseDirectlyOverChanged(object sender, DependencyPropertyChangedEventArgs e)
    {
        IsAreaMouseOver = IsMouseOver;
    }

    private void Split(SplitDirection direction)
    {
        Point point;
        Size size;

        var rect = GetRect();

        if (direction == SplitDirection.Vertical)
        {
            point = new Point((rect.TopLeft.X + rect.BottomRight.X) / 2, rect.TopLeft.Y);
            size = new Size(double.NaN, rect.Height);
        }
        else
        {
            point = new Point(rect.TopLeft.X, (rect.TopLeft.Y + rect.BottomRight.Y) / 2);
            size = new Size(rect.Width, double.NaN);
        }

        var newBorder = new SnapBorder(SnapControl, new SnapAreaTheme());
        newBorder.SetPos(point, size, direction);

        SnapControl.AddBorder(newBorder);
    }

    public Rect GetRect()
    {
        return new Rect(
            new Point(Margin.Left, Margin.Top),
            new Size(
                ActualWidth == 0 ? Width : ActualWidth,
                ActualHeight == 0 ? Height : ActualHeight));
    }
}