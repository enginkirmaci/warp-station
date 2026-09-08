using Microsoft.Extensions.DependencyInjection;
using Tools.Library.Services;
using Tools.Library.Services.Abstractions;
using Tools.Library.Services.OpenCode;

namespace Tools.Library.Extensions;

/// <summary>
/// Extension methods for service registration.
/// </summary>
public static class ServiceRegistrationExtensions
{
    /// <summary>
    /// Registers core library services.
    /// </summary>
    /// <param name="services">The service collection.</param>
    /// <returns>The service collection for chaining.</returns>
    public static IServiceCollection AddCoreServices(this IServiceCollection services)
    {
        services.AddSingleton<ISettingsService, SettingsService>();
        services.AddSingleton<IClipboardService, ClipboardService>();
        services.AddSingleton<INugetLocalService, NugetLocalService>();
        services.AddSingleton<IOpenCodeTemplateService, OpenCodeTemplateService>();
        services.AddSingleton<IOpenCodePromptService, OpenCodePromptService>();

        // Repos subsystem: scanner, cache store, and the singleton orchestrator
        services.AddSingleton<IRepoScanner, RepoScanner>();
        services.AddSingleton<IRepoCacheStore, RepoCacheStore>();
        services.AddSingleton<IRepoService, RepoService>();

        // Local git status checker: pushes branch/modified/ahead/behind onto repos
        services.AddSingleton<IGitStatusService, GitStatusService>();

        // GitHub column: open pull requests/issues via the gh CLI (gated on the
        // settings' Enable GitHub flag)
        services.AddSingleton<IGitHubService, GitHubService>();

        // Azure DevOps column: pull requests / work items / pipeline runs via the REST
        // API with a PAT (gated on the settings' Enable Azure DevOps flag + token)
        services.AddSingleton<IAzureDevOpsService, AzureDevOpsService>();

        // Both activity services behind their common contract, so consumers can
        // configure them in one loop (the page does it on settings load and save)
        services.AddSingleton<IRepoActivityService>(sp => sp.GetRequiredService<IGitHubService>());
        services.AddSingleton<IRepoActivityService>(sp => sp.GetRequiredService<IAzureDevOpsService>());

        // Process launcher and DevTools client for IPC
        services.AddSingleton<IProcessLauncher, ProcessLauncher>();
        services.AddSingleton<IDevToolsClient, DevToolsClient>();

        // Intent-level launch decisions (repo buttons, OpenCode launches): executable
        // resolution, argument shapes and the pipe-with-fallback launch behind one service
        services.AddSingleton<ITerminalLauncher, TerminalLauncher>();

        // opencode model list (one-shot 'opencode models' CLI runner)
        services.AddSingleton<IOpenCodeModelService, OpenCodeModelService>();

        // opencode one-shot prompt run (the Changes tab's commit-message wand)
        services.AddSingleton<IOpenCodeRunService, OpenCodeRunService>();

        // commit-message wand's prompt template (settings-folder MD, user-editable)
        services.AddSingleton<ICommitMessagePromptService, CommitMessagePromptService>();

        return services;
    }
}
