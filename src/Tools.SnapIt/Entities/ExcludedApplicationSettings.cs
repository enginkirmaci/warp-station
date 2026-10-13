namespace Tools.SnapIt.Entities;

public class ExcludedApplicationSettings
{
    public string Version = "2.0";
    public List<ExcludedApplication> Applications { get; set; } = [];
}