using Tools.SnapIt.Entities;
using Tools.SnapIt.Graphics;

namespace Tools.SnapIt.Contracts;

public interface IWindowManager : IInitialize
{
    void Show();

    void Hide();
}