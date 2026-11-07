using System.Text.Json;

sealed record Checkpoint(Guid Id, string Name, Position Position, DateTimeOffset Created);
sealed class LibraryData
{
    public int Version { get; set; } = 1;
    public Position? Quick { get; set; }
    public List<Checkpoint> Locations { get; set; } = new();
}
sealed class Library
{
    public static readonly JsonSerializerOptions JsonOptions = new() { WriteIndented = true };
    readonly string path;
    public string FilePath => path;
    public LibraryData Data { get; private set; } = new();
    public Library(string path) { this.path = path; }
    public static string DefaultPath => Path.Combine(AppContext.BaseDirectory, "checkpoints.json");

    public string UniqueName(string title, Guid? except = null)
    {
        if (string.IsNullOrWhiteSpace(title)) title = "Location " + (Data.Locations.Count + 1);
        var names = Data.Locations.Where(item => item.Id != except)
            .Select(item => item.Name).ToHashSet(StringComparer.OrdinalIgnoreCase);
        string result = title.Trim();
        int suffix = 1;
        while (names.Contains(result)) result = $"{title.Trim()} ({suffix++})";
        return result;
    }

    public void Load()
    {
        if (!File.Exists(path)) return;
        using var stream = File.OpenRead(path);
        var result = JsonSerializer.Deserialize<LibraryData>(stream) ?? throw new InvalidDataException("The locations file is empty.");
        Validate(result);
        Data = result;
    }
    public static LibraryData Parse(string json)
    {
        var result = JsonSerializer.Deserialize<LibraryData>(json) ??
            throw new InvalidDataException("The locations file is empty.");
        Validate(result);
        return result;
    }
    static void Validate(LibraryData result)
    {
        if (result.Version != 1 || result.Locations == null ||
            (result.Quick != null && !result.Quick.Valid) ||
            result.Locations.Any(item => item == null || item.Id == Guid.Empty ||
                string.IsNullOrWhiteSpace(item.Name) || 
                item.Position == null || !item.Position.Valid) ||
            result.Locations.Select(item => item.Id).Distinct().Count() != result.Locations.Count)
            throw new InvalidDataException("This locations file is invalid or uses an unsupported format.");
    }
    public void Change(Action<LibraryData> change)
    {
        var candidate = new LibraryData
        {
            Version = Data.Version,
            Quick = Data.Quick,
            Locations = new List<Checkpoint>(Data.Locations)
        };
        change(candidate);
        Validate(candidate);
        SaveAtomic(path, candidate);
        Data = candidate;
    }
    public int Import(string source)
    {
        using var stream = File.OpenRead(source);
        using var document = JsonDocument.Parse(stream);
        var root = document.RootElement;
        var incoming = root.ValueKind == JsonValueKind.Object &&
            !root.TryGetProperty("Version", out _) && !root.TryGetProperty("Locations", out _) ?
            ParseCoordinateDictionary(root) : root.Deserialize<LibraryData>() ?? throw new InvalidDataException("The locations file is empty.");
        Validate(incoming);
        int count = 0;
        Change(data =>
        {
            var ids = data.Locations.Select(item => item.Id).ToHashSet();
            var names = data.Locations.Select(item => item.Name).ToHashSet(StringComparer.OrdinalIgnoreCase);
            var suffixes = new Dictionary<string, int>(StringComparer.OrdinalIgnoreCase);
            foreach (var item in incoming.Locations)
                if (ids.Add(item.Id))
                {
                    string title = item.Name;
                    suffixes.TryGetValue(item.Name, out int suffix);
                    while (!names.Add(title))
                        title = item.Name + " (" + ++suffix + ")";
                    suffixes[item.Name] = suffix;
                    data.Locations.Add(item with { Name = title });
                    count++;
                }
        });
        return count;
    }
    static LibraryData ParseCoordinateDictionary(JsonElement root)
    {
        var data = new LibraryData();
        foreach (var entry in root.EnumerateObject())
        {
            if (string.IsNullOrWhiteSpace(entry.Name) || entry.Value.ValueKind != JsonValueKind.Array ||
                entry.Value.GetArrayLength() != 3)
                throw new InvalidDataException("The file must contain location names and X, Y and Z coordinates.");
            var values = entry.Value.EnumerateArray().Select(value => value.GetSingle()).ToArray();
            var point = new Position(0, values[0], values[1], values[2], 0);
            if (!point.Valid) throw new InvalidDataException("The file contains invalid coordinates.");
            data.Locations.Add(new Checkpoint(Guid.NewGuid(), entry.Name, point, DateTimeOffset.Now));
        }
        return data;
    }
    public void Export(string destination)
    {
        if (Path.GetFullPath(destination).Equals(Path.GetFullPath(path), StringComparison.OrdinalIgnoreCase))
            throw new InvalidOperationException("Choose a file other than the active locations file.");
        SaveAtomic(destination, Data);
    }
    static void SaveAtomic(string destination, LibraryData data)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(destination))!);
        string temporary = destination + "." + Guid.NewGuid().ToString("N") + ".tmp";
        try
        {
            using (var stream = new FileStream(temporary, FileMode.CreateNew, FileAccess.Write, FileShare.None,
                4096, FileOptions.WriteThrough))
            {
                JsonSerializer.Serialize(stream, data, JsonOptions);
                stream.Flush(true);
            }
            File.Move(temporary, destination, true);
        }
        finally { if (File.Exists(temporary)) File.Delete(temporary); }
    }
}


