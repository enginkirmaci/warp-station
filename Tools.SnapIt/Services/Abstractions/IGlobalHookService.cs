using SharpHook;
using Tools.SnapIt.Contracts;

namespace Tools.SnapIt.Services.Abstractions;

public interface IGlobalHookService : IInitialize
{
    SimpleGlobalHook? Hook { get; set; }
}