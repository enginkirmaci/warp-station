using System.Text.Json;
using Tools.Library.Entities;

namespace Tools.Library.Services
{
    public class SettingsService : ISettingsService
    {
        private const string SettingsFileName = "settings.json";
        private readonly string _settingsFilePath;
        private AppSettings? _cachedSettings;
        private readonly object _lock = new();

        public SettingsService()
        {
            // For unpackaged WinUI 3 app, BaseDirectory is reliable
            string baseDirectory = AppContext.BaseDirectory;
            _settingsFilePath = Path.Combine(baseDirectory, SettingsFileName);
        }

        public async Task<AppSettings> LoadSettingsAsync()
        {
            if (_cachedSettings != null)
            {
                return _cachedSettings;
            }

            AppSettings settings;
            try
            {
                if (File.Exists(_settingsFilePath))
                {
                    var json = File.ReadAllText(_settingsFilePath);
                    settings = JsonSerializer.Deserialize<AppSettings>(json, new JsonSerializerOptions
                    {
                        PropertyNameCaseInsensitive = true
                    }) ?? new AppSettings();
                }
                else
                {
                    settings = new AppSettings();
                }
            }
            catch (Exception ex)
            {
                Debug.WriteLine($"Error loading settings: {ex.Message}");
                settings = new AppSettings();
            }

            // Ensure nested properties are not null
            settings.EFToolsPage ??= new EFToolsPageSettings();
            settings.NugetLocal ??= new NugetLocalSettings();
            settings.Workspaces ??= new WorkspacesSettings();
            settings.ClipboardPassword ??= new ClipboardPasswordSettings();

            lock (_lock)
            {
                _cachedSettings = settings;
            }

            return settings;
        }

        public async Task<AppSettings> GetSettingsAsync()
        {
            if (_cachedSettings != null)
            {
                return _cachedSettings;
            }
            return await LoadSettingsAsync();
        }
    }
}