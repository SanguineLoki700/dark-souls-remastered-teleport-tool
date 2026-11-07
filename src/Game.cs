using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using Microsoft.Win32.SafeHandles;
using System.Text.Json.Serialization;

sealed record Position(int Area, float X, float Y, float Z, float Angle)
{
    [JsonIgnore]
    public bool Valid => float.IsFinite(X) && float.IsFinite(Y) && float.IsFinite(Z) && float.IsFinite(Angle);
    [JsonIgnore]
    public string Coordinates => $"X {X:0.00}   Y {Y:0.00}   Z {Z:0.00}";
}

sealed class Game : IDisposable
{
    const string Supported = "A45AAA36DD2F6CC151670A639EA5547043CF38EA79FF4178B963C6ED71F98D7B";
    const int ReadAccess = 0x410;
    const int WriteAccess = 0x28;
    readonly Process process;
    readonly SafeProcessHandle reader;
    readonly long worldSlot;
    public IReadOnlyDictionary<string, string> Details { get; }
    public bool Connected => !process.HasExited;
    public int Id => process.Id;

    Game(Process process, SafeProcessHandle reader, long worldSlot, Dictionary<string, string> details)
    {
        this.process = process;
        this.reader = reader;
        this.worldSlot = worldSlot;
        Details = details;
    }

    public static Game Connect()
    {
        var candidates = Process.GetProcessesByName("DarkSoulsRemastered");
        if (candidates.Length != 1)
        {
            foreach (var candidate in candidates) candidate.Dispose();
            throw new GameException("game_not_running", "Dark Souls Remastered is not running.",
                "Open Dark Souls Remastered, then load your character.");
        }
        var process = candidates[0];
        SafeProcessHandle? handle = null;
        var details = new Dictionary<string, string> { ["ProcessId"] = process.Id.ToString() };
        try
        {
            var module = process.MainModule ?? throw new InvalidOperationException("The game is still starting.");
            details["GamePath"] = module.FileName ?? "Unknown";
            details["FileVersion"] = module.FileVersionInfo.FileVersion ?? "Unknown";
            details["FileVersionNumbers"] = $"{module.FileVersionInfo.FileMajorPart}.{module.FileVersionInfo.FileMinorPart}.{module.FileVersionInfo.FileBuildPart}.{module.FileVersionInfo.FilePrivatePart}";
            details["ImageBase"] = $"0x{module.BaseAddress.ToInt64():X}";
            details["ImageSize"] = module.ModuleMemorySize.ToString();
            using var file = File.OpenRead(module.FileName ?? throw new InvalidOperationException("The game executable is not available."));
            using var sha = SHA256.Create();
            string digest = Convert.ToHexString(sha.ComputeHash(file));
            details["ExecutableSHA256"] = digest;
            details["SupportedSHA256"] = Supported;
            if (digest != Supported)
                throw new GameException("unsupported_build", "This game build is not supported.",
                    "Save the support log and include it in your bug report.");
            handle = OpenProcess(ReadAccess, false, process.Id);
            if (handle.IsInvalid) throw new GameException("access_denied", "Windows blocked access to the game.",
                "If you started Dark Souls as administrator, start this tool as administrator too.", new Win32Exception());
            long instruction = module.BaseAddress.ToInt64() + 0x7C4626;
            var code = Read(handle, instruction, 14);
            if (code[0] != 0x48 || code[1] != 0x8B || code[2] != 0x05 ||
                !code.AsSpan(7).SequenceEqual(new byte[] { 0x48, 0x8B, 0x48, 0x68, 0x48, 0x85, 0xC9 }))
                throw new InvalidOperationException("The game code does not match the supported build.");
            long slot = instruction + 7 + BitConverter.ToInt32(code, 3);
            details["WorldPointerSlot"] = $"0x{slot:X}";
            SupportLog.Event("Game attached " + System.Text.Json.JsonSerializer.Serialize(details));
            return new Game(process, handle, slot, details);
        }
        catch (Exception error)
        {
            foreach (var item in details) error.Data[item.Key] = item.Value;
            handle?.Dispose();
            process.Dispose();
            throw;
        }
    }

    public (Position Position, int Health, int MaxHealth, long Control) Snapshot()
    {
        long world = Pointer(worldSlot);
        long player = Pointer(world + 0x68);
        long control = Pointer(player + 0x68);
        long position = Pointer(control + 0x28);
        int area = BitConverter.ToInt32(Read(reader, player + 0x358, 4));
        var hp = Read(reader, player + 0x3E8, 8);
        var pose = Read(reader, position, 28);
        var point = new Position(area, BitConverter.ToSingle(pose, 16), BitConverter.ToSingle(pose, 20),
            BitConverter.ToSingle(pose, 24), BitConverter.ToSingle(pose, 4));
        int health = BitConverter.ToInt32(hp);
        int max = BitConverter.ToInt32(hp, 4);
        if (!point.Valid || health <= 0 || max < health || max > 100000 ||
            Pointer(worldSlot) != world || Pointer(world + 0x68) != player ||
            Pointer(player + 0x68) != control || Pointer(control + 0x28) != position ||
            BitConverter.ToInt32(Read(reader, player + 0x358, 4)) != area)
            throw new GameException("player_loading", "The character is not ready yet.",
                "Load a character and wait for the loading screen to finish.");
        return (point, health, max, control);
    }

    public async Task Travel(Position point)
    {
        if (!point.Valid) throw new InvalidOperationException("Enter valid coordinates.");
        var initial = Snapshot();

        using var writer = OpenProcess(WriteAccess, false, Id);
        if (writer.IsInvalid) throw new GameException("access_denied", "Windows blocked the position update.",
            "If you started Dark Souls as administrator, start this tool as administrator too.", new Win32Exception());
        var current = Snapshot();
        if (current.Control != initial.Control)
            throw new InvalidOperationException("The character changed while its position was being read.");
        long control = current.Control;
        if (Read(reader, control + 0x108, 1)[0] != 0)
            throw new InvalidOperationException("A teleport is already in progress. Try again in a moment.");
        var payload = new byte[32];
        BitConverter.TryWriteBytes(payload.AsSpan(0), point.X);
        BitConverter.TryWriteBytes(payload.AsSpan(4), point.Y);
        BitConverter.TryWriteBytes(payload.AsSpan(8), point.Z);
        BitConverter.TryWriteBytes(payload.AsSpan(12), 1f);
        BitConverter.TryWriteBytes(payload.AsSpan(20), MathF.IEEERemainder(point.Angle, MathF.Tau));
        Write(writer, control + 0x110, payload);
        current = Snapshot();
        if (current.Control != control ||
            Read(reader, control + 0x108, 1)[0] != 0)
            throw new InvalidOperationException("The character changed. Try again.");
        Write(writer, control + 0x108, new byte[] { 1 });

        for (int attempt = 0; attempt < 50; attempt++)
        {
            await Task.Delay(40);
            var after = Snapshot();
            if (after.Control != control) break;
            float dx = after.Position.X - point.X;
            float dy = after.Position.Y - point.Y;
            float dz = after.Position.Z - point.Z;
            if (Read(reader, control + 0x108, 1)[0] == 0 && dx * dx + dy * dy + dz * dz <= 0.25f)
                return;
        }
        throw new InvalidOperationException("The game did not confirm the teleport.");
    }

    long Pointer(long address)
    {
        long result = BitConverter.ToInt64(Read(reader, address, 8));
        if (result < 65536 || result > 0x7FFFFFFFFFFF)
            throw new GameException("player_loading", "The character is not ready yet.",
                "Load a character and wait for the loading screen to finish.");
        return result;
    }

    public bool HasRemoteTcp() => HasRemoteTcp(2, 24, 20, 0, 16) || HasRemoteTcp(23, 56, 52, 48, 44);

    bool HasRemoteTcp(int family, int rowSize, int pidOffset, int stateOffset, int portOffset)
    {
        int length = 0;
        if (GetExtendedTcpTable(IntPtr.Zero, ref length, false, family, 5, 0) != 122 || length < 4)
            throw new InvalidOperationException("The game connection status could not be read.");
        for (int attempt = 0; attempt < 3; attempt++)
        {
            int capacity = length;
            var buffer = Marshal.AllocHGlobal(capacity);
            try
            {
                uint result = GetExtendedTcpTable(buffer, ref length, false, family, 5, 0);
                if (result == 122) continue;
                int rows = result == 0 ? Marshal.ReadInt32(buffer) : -1;
                if (rows < 0 || rows > (capacity - 4) / rowSize) break;
                for (int index = 0; index < rows; index++)
                {
                    var row = IntPtr.Add(buffer, 4 + index * rowSize);
                    int state = Marshal.ReadInt32(row, stateOffset);
                    if (Marshal.ReadInt32(row, pidOffset) == Id &&
                        (Marshal.ReadInt32(row, portOffset) & 0xFFFF) != 0 && state >= 3 && state <= 8)
                        return true;
                }
                return false;
            }
            finally { Marshal.FreeHGlobal(buffer); }
        }
        throw new InvalidOperationException("The game connection status could not be read.");
    }

    static byte[] Read(SafeProcessHandle handle, long address, int size)
    {
        var data = new byte[size];
        if (!ReadProcessMemory(handle, (IntPtr)address, data, (UIntPtr)size, out var count) || count.ToUInt64() != (ulong)size)
        {
            var error = new GameException("memory_unavailable", "Player data is not available.",
                "Wait for the character to load, then try Reconnect.");
            error.Data["ReadAddress"] = $"0x{address:X}";
            error.Data["ReadSize"] = size;
            error.Data["Win32Error"] = Marshal.GetLastWin32Error();
            throw error;
        }
        return data;
    }

    static void Write(SafeProcessHandle handle, long address, byte[] data)
    {
        if (!WriteProcessMemory(handle, (IntPtr)address, data, (UIntPtr)data.Length, out var count) ||
            count.ToUInt64() != (ulong)data.Length)
        {
            var error = new GameException("write_failed", "The game did not accept the position update.",
                "Try Reconnect. Save the support log if the problem returns.");
            error.Data["WriteAddress"] = $"0x{address:X}";
            error.Data["WriteSize"] = data.Length;
            error.Data["Win32Error"] = Marshal.GetLastWin32Error();
            throw error;
        }
    }

    public void Dispose() { reader.Dispose(); process.Dispose(); }

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern SafeProcessHandle OpenProcess(int access, bool inherit, int pid);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool ReadProcessMemory(SafeProcessHandle handle, IntPtr address, byte[] data, UIntPtr length, out UIntPtr count);
    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool WriteProcessMemory(SafeProcessHandle handle, IntPtr address, byte[] data, UIntPtr length, out UIntPtr count);
    [DllImport("iphlpapi.dll")]
    static extern uint GetExtendedTcpTable(IntPtr table, ref int size, bool sort, int family, int tableClass, uint reserved);
}





