using Tools.SnapIt.Contracts;
using Tools.SnapIt.Entities;

namespace Tools.SnapIt.Services.Abstractions;

public interface ISettingService : IInitialize
{
	Settings Settings { get; }
	ExcludedApplicationSettings ExcludedApplicationSettings { get; }
	IList<Layout> Layouts { get; }
	IList<SnapScreen> SnapScreens { get; }
	SnapScreen LatestActiveScreen { get; set; }
	SnapScreen SelectedSnapScreen { get; set; }

	Task LoadSettingsAsync();

	void ReInitialize();

	void LinkScreenLayout(SnapScreen snapScreen, Layout layout);
}