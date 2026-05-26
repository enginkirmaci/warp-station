using System.Collections.Generic;

namespace Tools.SnapIt.Common.Entities;

public class ChangeLogItem
{
    public string Header { get; set; }
    public List<string> Lines { get; set; }
}