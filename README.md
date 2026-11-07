# Dark Souls Remastered Teleport

Save locations and return to them in Dark Souls Remastered.

## Download

Download the ZIP from [Releases](https://github.com/SanguineLoki700/dark-souls-remastered-teleport-tool/releases/latest), extract it, and run **Dark Souls Remastered Teleport.exe**. No Python or .NET installation needed.

Requires Windows 10/11 x64 and the supported Dark Souls Remastered 1.03.1 executable. Other game builds are reported in Diagnostics. Teleporting does not load another map.

## Use

- Load your character. The tool connects automatically.
- **F5** captures a checkpoint. **F9** returns to it.
- Enter a name and click **Save position** to keep a location.
- Select a location and click **Travel**, press Enter, or double-click it.
- Use **Edit**, **Remove**, **Find**, **Import**, and **Export** to manage locations. Each page shows 50 locations.
- **Manual coordinates** lets you enter a position directly.

Locations are saved in `%LOCALAPPDATA%\DarkSoulsCheckpoints\checkpoints.json`.

If something fails, open **Diagnostics** and use **Save support log**. Attach that log to your bug report.

Tested with 100,000 saved locations, including loading, import, export, search, and page navigation. Performance on other PCs may vary.

## Build

Install the .NET 6 SDK, then run from the repository folder:

```powershell
dotnet publish src/Checkpoint.csproj -c Release -r win-x64 --self-contained true -p:PublishSingleFile=true -p:EnableCompressionInSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true -p:DebugType=None -o publish
```

The interface uses [EB Garamond](https://github.com/octaviopardo/EBGaramond12), included under the [SIL Open Font License](FONT-LICENSE.txt). Dark Souls artwork belongs to its respective owners.
