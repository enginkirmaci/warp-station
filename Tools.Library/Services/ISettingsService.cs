using Tools.Library.Entities; // Already present, ensure it's correct if auto-format moved it

namespace Tools.Library.Services
{
    public interface ISettingsService
    {
        Task<AppSettings> LoadSettingsAsync();

        Task<AppSettings> GetSettingsAsync();
    }
}