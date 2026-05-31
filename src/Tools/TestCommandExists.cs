using Tools.ViewModels.Pages;

public class TestCommandExists
{
    public void TestMethod()
    {
        var vm = new WorkspacesViewModel(null!, null!);
        var cmd = vm.OpenSolutionCommand;
        var cmd2 = vm.OpenFolderCommand;
        var prop = vm.FilterText;

        // var vm2 = new ClipboardPasswordPageViewModel(null!);
        // var cmd3 = vm2.ClearPasswordAsyncCommand;
    }
}
