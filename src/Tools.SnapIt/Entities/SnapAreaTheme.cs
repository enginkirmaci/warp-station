using Avalonia;
using Tools.SnapIt.Extensions;
using Tools.SnapIt.Mvvm;
using Color = Avalonia.Media.Color;

namespace Tools.SnapIt.Entities;

public class SnapAreaTheme : Bindable
{
	private Color highlightColor;
	private Color overlayColor;
	private Color borderColor;
	private Thickness borderThickness;
	private double opacity;
	private SolidColorBrush overlayBrush;
	private SolidColorBrush highlightBrush;
	private SolidColorBrush borderBrush;

	[JsonIgnore]
	public SolidColorBrush OverlayBrush { get => overlayBrush; set => SetProperty(ref overlayBrush, value); }

	[JsonIgnore]
	public SolidColorBrush HighlightBrush { get => highlightBrush; set => SetProperty(ref highlightBrush, value); }

	[JsonIgnore]
	public SolidColorBrush BorderBrush { get => borderBrush; set => SetProperty(ref borderBrush, value); }

	public Color HighlightColor
	{
		get => highlightColor;
		set
		{
			SetProperty(ref highlightColor, value);
			HighlightBrush = new SolidColorBrush(value);

			ThemeChanged?.Invoke();
		}
	}

	public Color OverlayColor
	{
		get => overlayColor;
		set
		{
			SetProperty(ref overlayColor, value);
			OverlayBrush = new SolidColorBrush(value);

			ThemeChanged?.Invoke();
		}
	}

	public Color BorderColor
	{
		get => borderColor;
		set
		{
			SetProperty(ref borderColor, value);
			BorderBrush = new SolidColorBrush(value);

			ThemeChanged?.Invoke();
		}
	}

	public Thickness BorderThickness
	{ get => borderThickness; set { SetProperty(ref borderThickness, value); ThemeChanged?.Invoke(); } }

	public double Opacity
	{ get => opacity; set { SetProperty(ref opacity, value); ThemeChanged?.Invoke(); } }

	public delegate void ThemeChangedEvent();

	public event ThemeChangedEvent ThemeChanged;

	public SnapAreaTheme()
	{
		HighlightColor = Color.FromArgb(200, 0, 0, 0);
		OverlayColor = Color.FromArgb(150, 0, 0, 255);
		BorderColor = Color.FromArgb(200, 150, 150, 150);
		BorderThickness = new Thickness(1);
		Opacity = 0.2;
	}

	public SnapAreaTheme(
		Graphics.Color highlightColor,
		Graphics.Color overlayColor,
		Graphics.Color borderColor,
		int borderThickness,
		double opacity
		)
	{
		HighlightColor = highlightColor.Convert();
		OverlayColor = overlayColor.Convert();
		BorderColor = borderColor.Convert();
		Opacity = opacity;
		BorderThickness = new Thickness(borderThickness);
	}
}
