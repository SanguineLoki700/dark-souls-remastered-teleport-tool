using System.Text.Json;

static class Program
{
    public const string Version = "2.2";
    [STAThread]
    static void Main(string[] args)
    {
        var culture = System.Globalization.CultureInfo.GetCultureInfo("en-US");
        System.Globalization.CultureInfo.DefaultThreadCurrentCulture = culture;
        System.Globalization.CultureInfo.DefaultThreadCurrentUICulture = culture;
        System.Globalization.CultureInfo.CurrentCulture = culture;
        System.Globalization.CultureInfo.CurrentUICulture = culture;
        SupportLog.Event("Application started " + JsonSerializer.Serialize(SupportLog.EnvironmentDetails));
        AppDomain.CurrentDomain.UnhandledException += (_, e) =>
            SupportLog.Error("Unhandled application error", e.ExceptionObject as Exception ?? new Exception(Convert.ToString(e.ExceptionObject)));
        Application.ThreadException += (_, e) =>
        {
            SupportLog.Error("Unhandled interface error", e.Exception);
            MessageBox.Show("An unexpected error occurred. Open Diagnostics and save the support log.", "Dark Souls Teleport");
        };
        ApplicationConfiguration.Initialize();
        if (args.Length == 2 && args[0] == "--probe")
        {
            try
            {
                using var game = Game.Connect();
                var snapshot = game.Snapshot();
                File.WriteAllText(args[1], JsonSerializer.Serialize(new
                {
                    Connected = game.Connected, Pid = game.Id, Position = snapshot.Position,
                    Health = snapshot.Health, MaxHealth = snapshot.MaxHealth,
                    Online = game.HasRemoteTcp()
                }, Library.JsonOptions));
            }
            catch (Exception error) { File.WriteAllText(args[1], error.ToString()); Environment.ExitCode = 1; }
            return;
        }
        if (args.Length == 2 && args[0] == "--self-test")
        {
            try { Verify(args[1]); }
            catch (Exception error) { File.WriteAllText(args[1] + ".log", error.ToString()); Environment.ExitCode = 1; }
            return;
        }
        if (args.Length == 2 && args[0] == "--travel-test")
        {
            try { VerifyTravel(args[1]).GetAwaiter().GetResult(); }
            catch (Exception error) { File.WriteAllText(args[1], error.ToString()); Environment.ExitCode = 1; }
            return;
        }
        if (args.Length == 2 && args[0] == "--stress-test")
        {
            try { StressChecks.Run(args[1]).GetAwaiter().GetResult(); }
            catch (Exception error)
            {
                Directory.CreateDirectory(args[1]);
                File.WriteAllText(Path.Combine(args[1], "failure.txt"), error.ToString());
                Environment.ExitCode = 1;
            }
            return;
        }
        using var mutex = new Mutex(true, "Local\\DarkSoulsCheckpoints.SingleInstance", out bool created);
        if (!created)
        {
            MessageBox.Show("Dark Souls Remastered Teleport is already open.", "Dark Souls Teleport");
            return;
        }
        string libraryPath = args.Length == 2 && args[0] == "--data" ? Path.GetFullPath(args[1]) : Library.DefaultPath;
        Application.Run(new MainForm(new Library(libraryPath)));
    }

    static void Verify(string directory)
    {
        Directory.CreateDirectory(directory);
        var library = new Library(Path.Combine(directory, "library.json"));
        var point = new Position(102000, -52.5983f, -60.5875f, 53.9973f, 0.2393f);
        var item = new Checkpoint(Guid.NewGuid(), "Unicode çãõ", point, DateTimeOffset.Now);
        library.Change(data => { data.Quick = point; data.Locations.Add(item); });
        var loaded = new Library(Path.Combine(directory, "library.json"));
        loaded.Load();
        if (loaded.Data.Locations.Single() != item || loaded.Data.Quick != point)
            throw new Exception("Library round trip failed");
        string exported = Path.Combine(directory, "export.json");
        loaded.Export(exported);
        if (loaded.Import(exported) != 0) throw new Exception("Duplicate import");
        loaded.Change(data => data.Locations.Clear());
        if (loaded.Import(exported) != 1) throw new Exception("Import failed");
        try { Library.Parse("{\"Version\":99}"); throw new Exception("Accepted invalid version"); }
        catch (InvalidDataException) { }
        try { Library.Parse("{\"Locations\":[null]}"); throw new Exception("Accepted null entry"); }
        catch (InvalidDataException) { }
        string before = File.ReadAllText(Path.Combine(directory, "library.json"));
        try { loaded.Change(data => data.Locations.Add(item)); throw new Exception("Accepted duplicate id"); }
        catch (InvalidDataException) { }
        if (before != File.ReadAllText(Path.Combine(directory, "library.json")))
            throw new Exception("Invalid change modified disk");
        string legacy = Path.Combine(directory, "legacy.json");
        File.WriteAllText(legacy, "{\"Tower\":[1.25,-2.5,3]}");
        if (loaded.Import(legacy) != 1 || loaded.Import(legacy) != 1 ||
            loaded.Data.Locations.Count(item => item.Name.StartsWith("Tower")) != 2)
            throw new Exception("Coordinate dictionary import failed");
        try { throw new InvalidDataException("Self-test error for support log verification."); }
        catch (Exception error)
        {
            error.Data["SelfTest"] = "Full exception details";
            SupportLog.Error("Self-test support export", error);
        }
        string support = Path.Combine(directory, "support-test.log");
        SupportLog.Export(support, "Self-test snapshot");
        string report = File.ReadAllText(support);
        if (!report.Contains("System.IO.InvalidDataException") || !report.Contains("SelfTest = Full exception details"))
            throw new Exception("Support log did not retain full error details.");
        File.WriteAllText(Path.Combine(directory, "result.txt"),
            "PASS - save/load, quick checkpoint, UTF-8, import/export, duplicate handling, invalid-change preservation.");
    }
    static async Task VerifyTravel(string destination)
    {
        using var game = Game.Connect();
        var origin = game.Snapshot().Position;
        var target = origin with { X = origin.X + 0.35f };
        Position? arrival = null;
        try
        {
            await game.Travel(target);
            arrival = game.Snapshot().Position;
            if (Math.Abs(arrival.X - origin.X) < 0.15f)
                throw new Exception("The position did not change enough to verify travel.");
        }
        finally { await game.Travel(origin); }
        var returned = game.Snapshot().Position;
        File.WriteAllText(destination, JsonSerializer.Serialize(new
        {
            Result = "PASS", Origin = origin, Target = target, Arrival = arrival, Returned = returned
        }, Library.JsonOptions));
    }
}

