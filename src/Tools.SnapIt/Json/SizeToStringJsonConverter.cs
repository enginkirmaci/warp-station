using Size = Tools.SnapIt.Graphics.Size;

namespace Tools.SnapIt.Json;

public class SizeToStringJsonConverter : FloatPairJsonConverter<Size>
{
    protected override Size Create() => new Size();

    protected override float GetFirst(Size value) => value.Width;

    protected override float GetSecond(Size value) => value.Height;

    protected override void SetFirst(Size value, float first) => value.Width = first;

    protected override void SetSecond(Size value, float second) => value.Height = second;
}
