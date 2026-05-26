using Tools.SnapIt.Common.Contracts;
using Tools.SnapIt.Common.Entities;
using Tools.SnapIt.Common.Graphics;

namespace Tools.SnapIt.Application.Contracts;

public interface IWindowManager : IInitialize
{
    void Show();

    void Hide();

    Dictionary<int, Rectangle> GetSnapAreaRectangles(SnapScreen snapScreen);
}