namespace Tools.Library.Services.Abstractions;

/// <summary>
/// Marker for ViewModels of tool-drawer components that need the payload their open
/// was made with. The main window delivers the drawer service's current context once
/// per open, right after the component view is resolved and hosted — this is the
/// interface it checks the hosted ViewModel against. Tools opened from the dropdown
/// have no context; drawer-hosted dialogs (Add Repositories, Repo Settings) receive
/// their request state and completion source through it. Implement
/// <see cref="IToolDrawerContextReceiver{TContext}"/> to receive the payload typed.
/// </summary>
public interface IToolDrawerContextReceiver
{
}

/// <summary>
/// A tool-drawer component that receives the open's payload as
/// <typeparamref name="TContext"/> instead of type-checking an untyped delivery.
/// The delivery stays unconditional: an open without this receiver's payload type
/// (the tools-dropdown opens, which carry none) delivers <c>null</c> and the receiver
/// decides what that means — seed without a repo, or ignore.
/// </summary>
/// <typeparam name="TContext">The payload type this receiver understands.</typeparam>
public interface IToolDrawerContextReceiver<TContext> : IToolDrawerContextReceiver
    where TContext : class
{
    /// <summary>Called once per drawer open with the payload passed to <c>Open</c>.</summary>
    /// <param name="context">
    /// The open's context payload when it is a <typeparamref name="TContext"/>;
    /// <c>null</c> when the open carried none of this receiver's type.
    /// </param>
    Task OnDrawerContextAsync(TContext? context);
}
