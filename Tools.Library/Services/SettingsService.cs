using System.Text.Json;
using Tools.Library.Entities; // Already present, ensure it's correct if auto-format moved it

namespace Tools.Library.Services
{
    public class SettingsService : ISettingsService
    {
        private const string SettingsFileName = "settings.json";

        // Adjust the path relative to the execution directory as needed.
        // Assuming settings.json is copied to the output directory.
        private readonly string _settingsFilePath;

        private AppSettings? _cachedSettings;
        private readonly object _lock = new object();

        public SettingsService()
        {
            // Determine the path to settings.json relative to the application's base directory
            string baseDirectory = AppContext.BaseDirectory;
            // Navigate up from bin/Debug/netX.X-windows/ to the project root where settings.json might be,
            // or assume it's copied to the output directory. For simplicity, let's assume it's alongside the executable.
            // A more robust solution might involve configuration or searching specific paths.
            // For now, let's look for it in the base directory and one level up (project root during development).
            _settingsFilePath = Path.Combine(baseDirectory, SettingsFileName);
            if (!File.Exists(_settingsFilePath))
            {
                // Try one level up (common during development from bin/Debug)
                var parentDir = Directory.GetParent(baseDirectory)?.FullName;
                if (parentDir != null)
                {
                    var parentPath = Path.Combine(parentDir, SettingsFileName);
                    if (File.Exists(parentPath))
                    {
                        _settingsFilePath = parentPath;
                    }
                    else
                    {
                        // Try two levels up (another common scenario)
                        var grandParentDir = Directory.GetParent(parentDir)?.FullName;
                        if (grandParentDir != null)
                        {
                            var grandParentPath = Path.Combine(grandParentDir, SettingsFileName);
                            if (File.Exists(grandParentPath))
                            {
                                _settingsFilePath = grandParentPath;
                            }
                            else
                            {
                                // Fallback to base directory path even if file doesn't exist there yet.
                                // The LoadSettingsAsync will handle the file not found scenario.
                                _settingsFilePath = Path.Combine(baseDirectory, SettingsFileName);
                            }
                        }
                    }
                }
            }
            // If still not found, check the original location requested by the user
            if (!File.Exists(_settingsFilePath))
            {
                _settingsFilePath = Path.Combine(baseDirectory, "..", "..", "..", "ViewModels", "Pages", SettingsFileName); // Relative path from output dir
                _settingsFilePath = Path.GetFullPath(_settingsFilePath); // Normalize the path
            }
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
                if (!File.Exists(_settingsFilePath))
                {
                    // Log or handle the missing file scenario. Return default settings.
                    Console.WriteLine($"Warning: Settings file not found at {_settingsFilePath}. Using default settings.");
                    settings = new AppSettings(); // Create default settings
                }
                else
                {
                    var json = File.ReadAllText(_settingsFilePath);
                    settings = JsonSerializer.Deserialize<AppSettings>(json, new JsonSerializerOptions
                    {
                        PropertyNameCaseInsensitive = true
                    }) ?? new AppSettings(); // Use default if deserialization fails
                }
            }
            catch (Exception ex)
            {
                // Log the exception
                Console.WriteLine($"Error loading settings from {_settingsFilePath}: {ex.Message}");
                settings = new AppSettings(); // Fallback to default settings on error
            }

            // Ensure nested properties are not null if the top-level key exists but is empty in JSON
            settings.EFToolsPage ??= new EFToolsPageSettings();
            settings.NugetLocal ??= new NugetLocalSettings();
            settings.Workspaces ??= new WorkspacesSettings();

            lock (_lock)
            {
                _cachedSettings = settings;
            }
            return _cachedSettings;
        }

        public AppSettings GetSettings()
        {
            // Ensure settings are loaded before returning
            if (_cachedSettings == null)
            {
                // This is a synchronous call, ideally LoadSettingsAsync should be called first during startup.
                // For simplicity here, we load synchronously if not already cached.
                // Consider making this async or ensuring LoadSettingsAsync is called during app initialization.
                LoadSettingsAsync().ConfigureAwait(false).GetAwaiter().GetResult();
            }
            // The lock ensures thread safety, and the null-forgiving operator is safe
            // because LoadSettingsAsync initializes _cachedSettings.
            lock (_lock)
            {
                return _cachedSettings!;
            }
        }
    }
}