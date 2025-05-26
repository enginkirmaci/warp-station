namespace Tools.Library.Entities;

public class NavigationItem
{
    public string Title { get; set; }
    public string Subtitle { get; set; }
    public string Symbol { get; set; }
    public ICommand Command { get; set; }
    public object CommandParameter { get; set; }
    public Type TargetPageType { get; set; }
}