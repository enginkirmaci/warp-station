# Dev Tools

Compact WinUI 3 developer utilities collection.

## Quick start
 
Build:
```sh
dotnet build Tools.sln
```

Run (from IDE or CLI):
```sh
dotnet run --project Tools/Tools.csproj
```


## Installer (GitHub Actions)

This repository includes a GitHub Actions workflow that builds the application and generates a Windows installer using Inno Setup. The workflow is defined in `.github/workflows/build-installer.yml`.

How it works:
- Restores and builds the solution with .NET 10
- Publishes a self-contained x64 publish of `Tools` project
- Installs Inno Setup on the runner and compiles `setup.iss`
- Uploads the generated installer as a workflow artifact
- When triggered by a tag (for example `v1.0.0`) the workflow will also create a GitHub Release and attach the installer

Triggering a release (example):

```sh
git tag v1.0.0
git push origin v1.0.0
```

You can also run the workflow manually from the Actions tab (workflow dispatch).

Notes:
- The installer is produced without code signing.
- The Inno Setup script is `setup.iss` at the repository root.

## License

See repository LICENSE (MIT).