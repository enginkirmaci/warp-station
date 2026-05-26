using SharpHook;
using Tools.SnapIt.Common.Contracts;

namespace Tools.SnapIt.Services.Contracts;

public interface IGlobalHookService : IInitialize
{
    SimpleGlobalHook? Hook { get; set; }
}