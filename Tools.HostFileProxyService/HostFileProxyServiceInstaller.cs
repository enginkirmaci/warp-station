//using System.Collections;
//using System.ComponentModel;
//using System.Configuration.Install;
//using System.ServiceProcess;

//namespace Tools.HostFileProxyService
//{
//    [RunInstaller(true)]
//    public partial class HostFileProxyServiceInstaller : Installer
//    {
//        private const string serviceName = "Windows.HostFile.Proxy";

//        public HostFileProxyServiceInstaller()
//        {
//            var processInstaller = new ServiceProcessInstaller();
//            var serviceInstaller = new ServiceInstaller();

//            //set the privileges
//            processInstaller.Account = ServiceAccount.LocalSystem;

//            serviceInstaller.DisplayName = serviceName;
//            serviceInstaller.Description = serviceName.Replace('.', ' ');
//            serviceInstaller.StartType = ServiceStartMode.Automatic;

//            serviceInstaller.ServiceName = serviceName;
//            Installers.Add(processInstaller);
//            Installers.Add(serviceInstaller);
//        }

//        protected override void OnAfterInstall(IDictionary savedState)
//        {
//            base.OnAfterInstall(savedState);

//            using (ServiceController serviceController = new ServiceController(serviceName))
//            {
//                serviceController.Start();
//            }
//        }
//    }
//}