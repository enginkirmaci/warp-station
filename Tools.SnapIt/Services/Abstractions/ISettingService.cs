using Tools.SnapIt.Common.Contracts;
using Tools.SnapIt.Common.Entities;

namespace Tools.SnapIt.Services.Contracts;

public interface ISettingService : IInitialize
{
    Settings Settings { get; }
    ExcludedApplicationSettings ExcludedApplicationSettings { get; }
    ApplicationGroupSettings ApplicationGroupSettings { get; }
    IList<Layout> Layouts { get; }
    IList<SnapScreen> SnapScreens { get; }
    SnapScreen LatestActiveScreen { get; set; }
    SnapScreen SelectedSnapScreen { get; set; }

    Task LoadSettingsAsync();

    void ReInitialize();

    void Save();

    void SaveLayout(Layout layout);

    void SaveExcludedApps(List<ExcludedApplication> excludedApplications);

    void ExportLayout(Layout layout, string layoutPath);

    void DeleteLayout(Layout layout);

    Layout ImportLayout(string layoutPath);

    void LinkScreenLayout(SnapScreen snapScreen, Layout layout);

    Task<bool> GetStartupTaskStatusAsync();

    Task SetStartupTaskStatusAsync(bool isActive);
}
