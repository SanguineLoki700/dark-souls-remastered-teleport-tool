# Dark Souls Remastered Teleport

Save locations and teleport back to them, including across maps. The game loads the destination before placing your character.

Teleports within the same map move your character directly. A different map ID triggers loading.

Download the ZIP from [Releases](https://github.com/SanguineLoki700/dark-souls-remastered-teleport-tool/releases/latest), extract it, and run **Dark Souls Remastered Teleport.exe**. Windows 10/11 x64, Dark Souls Remastered 1.03.1. No Python or .NET installation needed.

- **F5** captures a checkpoint. **F9** returns to it.
- **Save position** keeps a location. Names are optional.
- Select a location and click **Travel**, press Enter, or double-click it.
- **Edit**, **Remove**, **Find**, **Import**, and **Export** manage your locations. There are 50 locations per page.
- **Manual coordinates** lets you enter a position directly.

`checkpoints.json` and the `logs` folder stay beside the executable. Keep them when updating or moving the tool. The previous version's locations file is supported.

For bug reports, open **Diagnostics**, click **Save support log**, and attach the log. Tested with 100,000 locations and live travel between separate maps.

The ZIP contains the executable and one LICENSES.txt file with the font and library notices.

Build on Windows with Rust and the MSVC build tools:

```powershell
cargo build --release --locked
```

The executable is in `target/release`. Font and library notices are in [LICENSES.txt](LICENSES.txt). Dark Souls artwork belongs to its respective owners.
