using Tools.SnapIt.Contracts;
using Tools.SnapIt.Entities;

namespace Tools.SnapIt.Services.Abstractions;

public interface IFileOperationService : IInitialize
{
	Task<T> LoadAsync<T>() where T : new();

	IList<Layout> GetLayouts();
}