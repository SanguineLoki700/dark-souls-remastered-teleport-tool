using System.Diagnostics;
using System.Text.Json;

static class StressChecks
{
    public static async Task Run(string directory)
    {
        Directory.CreateDirectory(directory);
        const int count = 100000;
        using var game = Game.Connect();
        var point = game.Snapshot().Position;
        string path = Path.Combine(directory, "checkpoints.json");
        var original = new Library(path);
        var clock = Stopwatch.StartNew();
        original.Change(data =>
        {
            data.Quick = point;
            data.Locations = Enumerable.Range(0, count).Select(index => new Checkpoint(
                Guid.NewGuid(), $"Location {index:D6}", point, DateTimeOffset.UtcNow)).ToList();
        });
        long createMs = clock.ElapsedMilliseconds;
        var reloaded = new Library(path);
        clock.Restart();
        var loading = Task.Run(reloaded.Load);
        var reading = Task.Run(() => game.Snapshot());
        var readResult = await reading;
        long connectionReadMs = clock.ElapsedMilliseconds;
        await loading;
        long loadMs = clock.ElapsedMilliseconds;
        if (readResult.Health <= 0 || reloaded.Data.Locations.Count != count || reloaded.Data.Quick != point)
            throw new Exception("Large library load or independent game read failed.");
        string exported = Path.Combine(directory, "export.json");
        clock.Restart();
        reloaded.Export(exported);
        if (reloaded.Import(exported) != 0) throw new Exception("Native import duplicated records.");
        long nativeRoundTripMs = clock.ElapsedMilliseconds;
        string legacy = Path.Combine(directory, "coordinates.json");
        using (var stream = File.Create(legacy))
        using (var writer = new Utf8JsonWriter(stream))
        {
            writer.WriteStartObject();
            for (int index = 0; index < count; index++)
            {
                writer.WritePropertyName($"Location {index:D6}");
                writer.WriteStartArray();
                writer.WriteNumberValue(point.X);
                writer.WriteNumberValue(point.Y);
                writer.WriteNumberValue(point.Z);
                writer.WriteEndArray();
            }
            writer.WriteEndObject();
        }
        var imported = new Library(Path.Combine(directory, "imported.json"));
        clock.Restart();
        if (imported.Import(legacy) != count || imported.Data.Locations.Count != count)
            throw new Exception("Large coordinate dictionary import failed.");
        long dictionaryImportMs = clock.ElapsedMilliseconds;
        clock.Restart();
        reloaded.Change(data => data.Quick = point);
        long quickSaveMs = clock.ElapsedMilliseconds;
        if (reloaded.Data.Locations.Count != count) throw new Exception("Quick capture changed record count.");
        var visible = reloaded.Data.Locations.OrderBy(item => item.Name).Skip(count - 50).Take(50).ToList();
        if (visible.Count != 50 || visible[^1].Name != "Location 099999")
            throw new Exception("Last page lost records.");
        File.WriteAllText(Path.Combine(directory, "result.json"), JsonSerializer.Serialize(new
        {
            Result = "PASS", Records = count, JsonBytes = new FileInfo(path).Length,
            Pages = count / 50, RowsPerPage = visible.Count, PlayerHealth = readResult.Health,
            CreateMs = createMs, IndependentGameReadMs = connectionReadMs, LoadMs = loadMs,
            NativeRoundTripMs = nativeRoundTripMs, DictionaryImportMs = dictionaryImportMs,
            QuickSaveMs = quickSaveMs
        }, Library.JsonOptions));
    }
}

