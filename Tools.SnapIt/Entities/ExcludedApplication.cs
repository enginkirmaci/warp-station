using Tools.SnapIt.Mvvm;

namespace Tools.SnapIt.Entities;

public class ExcludedApplication : Bindable
{
    private bool enabledForMouse = true;
    private MatchRule matchRule = MatchRule.Contains;
    private string keyword;

    public string Keyword { get => keyword; set => SetProperty(ref keyword, value); }
    public MatchRule MatchRule { get => matchRule; set => SetProperty(ref matchRule, value); }

    [JsonIgnore]
    public InputDevice AppliedFor => Mouse ? InputDevice.Mouse : InputDevice.None;

    public bool Mouse
    {
        get => enabledForMouse;
        set => SetProperty(ref enabledForMouse, value);
    }
}