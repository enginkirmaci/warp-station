using System.Text.RegularExpressions;
using Tools.SnapIt.Entities;
using Tools.SnapIt.Services.Abstractions;

namespace Tools.SnapIt.Services;

public class WindowsService : IWindowsService
{
    private readonly ISettingService settingService;
    private readonly IWinApiService winApiService;
    private List<ExcludedApplication> matchRulesForMouse;
    private Dictionary<string, Regex> regexCache = new(StringComparer.OrdinalIgnoreCase);

    public bool IsInitialized { get; private set; }

    public WindowsService(
       ISettingService settingService,
       IWinApiService winApiService)
    {
        this.settingService = settingService;
        this.winApiService = winApiService;
    }

    public async Task InitializeAsync()
    {
        if (IsInitialized)
        {
            return;
        }

        await settingService.InitializeAsync();

        if (settingService.ExcludedApplicationSettings?.Applications != null)
        {
            matchRulesForMouse = settingService.ExcludedApplicationSettings.Applications.Where(i => i.Mouse).ToList();
        }

        IsInitialized = true;
    }

    public bool DisableIfFullScreen()
    {
        var activeWindow = winApiService.GetActiveWindow();
        if (activeWindow != ActiveWindow.Empty && !string.IsNullOrWhiteSpace(activeWindow.Title) && !activeWindow.Title.Equals("Program Manager") && settingService.Settings.DisableForFullscreen && winApiService.IsFullscreen(activeWindow))
        {
            return true;
        }
        return false;
    }

    public bool IsExcludedApplication(string Title)
    {
        if (matchRulesForMouse != null)
        {
            var isMatched = false;
            foreach (var rule in matchRulesForMouse)
            {
                if (string.IsNullOrWhiteSpace(rule.Keyword))
                {
                    continue;
                }

                switch (rule.MatchRule)
                {
                    case MatchRule.Contains:
                        isMatched = Title.Contains(rule.Keyword, StringComparison.OrdinalIgnoreCase);
                        break;

                    case MatchRule.Exact:
                        isMatched = Title == rule.Keyword;
                        break;

                    case MatchRule.Wildcard:
                        isMatched = WildcardMatch(rule.Keyword, Title, false);
                        break;
                }

                if (isMatched)
                {
                    break;
                }
            }

            return isMatched;
        }

        return false;
    }

    public void Dispose()
    {
        IsInitialized = false;
    }

    private bool WildcardMatch(string pattern, string input, bool caseSensitive = false)
    {
        if (!regexCache.TryGetValue(pattern, out var regex))
        {
            var escaped = Regex.Escape(pattern);
            escaped = escaped.Replace(@"\*", ".*?").Replace(@"\?", ".");
            var opts = caseSensitive ? RegexOptions.None : RegexOptions.IgnoreCase;
            regex = new Regex(escaped, RegexOptions.Compiled | opts);
            regexCache[pattern] = regex;
        }

        return regex.IsMatch(input);
    }
}