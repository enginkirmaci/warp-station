using Prism.Ioc;

namespace Tools.Library.Extensions;

public static class ServiceCollectionExtensions
{
    public static IContainerRegistry AddTransientFromNamespace(
        this IContainerRegistry services,
        string namespaceName,
        params Assembly[] assemblies
    )
    {
        foreach (Assembly assembly in assemblies)
        {
            IEnumerable<Type> types = assembly.GetTypes()
                .Where(x =>
                    x.IsClass &&
                    x.Namespace!=null && x.Namespace!.StartsWith(namespaceName, StringComparison.InvariantCultureIgnoreCase)
                );

            foreach (Type? type in types)
            {
                if (!services.IsRegistered(type))
                {
                    services.Register(type);
                }
            }
        }

        return services;
    }
}