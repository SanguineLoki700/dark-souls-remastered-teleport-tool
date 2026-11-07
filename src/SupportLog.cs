using System.Collections;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Text;

static class SupportLog
{
    static readonly object Gate = new();
    static readonly Queue<string> Fallback = new();
    static readonly string Folder = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "DarkSoulsCheckpoints", "logs");
    static string Current => Path.Combine(Folder, "teleport.log");

    public static void Event(string text) => Write("INFO", text);

    public static void Error(string action, Exception error)
    {
        var text = new StringBuilder(action).AppendLine().AppendLine(error.ToString());
        foreach (DictionaryEntry item in error.Data)
            text.Append(item.Key).Append(" = ").AppendLine(Convert.ToString(item.Value));
        Write("ERROR", text.ToString());
    }

    static void Write(string level, string text)
    {
        string entry = $"{DateTimeOffset.UtcNow:O} [{level}] {text}{Environment.NewLine}";
        lock (Gate)
        {
            try
            {
                Directory.CreateDirectory(Folder);
                if (File.Exists(Current) && new FileInfo(Current).Length >= 2 * 1024 * 1024)
                {
                    string previous = Path.Combine(Folder, "teleport.1.log");
                    string older = Path.Combine(Folder, "teleport.2.log");
                    if (File.Exists(previous)) File.Move(previous, older, true);
                    File.Move(Current, previous, true);
                }
                File.AppendAllText(Current, entry, Encoding.UTF8);
            }
            catch (IOException) { Retain(entry); }
            catch (UnauthorizedAccessException) { Retain(entry); }
        }
    }
    static void Retain(string entry)
    {
        Fallback.Enqueue(entry);
        while (Fallback.Count > 1000) Fallback.Dequeue();
    }

    public static void Export(string destination, string snapshot)
    {
        var text = new StringBuilder("Dark Souls Remastered Teleport support log\n\n")
            .AppendLine(snapshot).AppendLine().AppendLine("Recorded events");
        lock (Gate)
        {
            foreach (string file in new[] { "teleport.2.log", "teleport.1.log", "teleport.log" })
            {
                string path = Path.Combine(Folder, file);
                if (File.Exists(path)) text.AppendLine(File.ReadAllText(path));
            }
            foreach (string entry in Fallback) text.Append(entry);
        }
        File.WriteAllText(destination, text.ToString(), Encoding.UTF8);
    }

    public static object EnvironmentDetails
    {
        get
        {
            using var process = Process.GetCurrentProcess();
            return new
            {
                ToolVersion = Program.Version, Windows = RuntimeInformation.OSDescription,
                Architecture = RuntimeInformation.ProcessArchitecture.ToString(),
                Runtime = RuntimeInformation.FrameworkDescription, ToolPath = Environment.ProcessPath,
                MemoryMB = process.WorkingSet64 / (1024 * 1024)
            };
        }
    }

    public static string UserMessage(Exception error)
    {
        if (error is GameException) return error.Message;
        if (error is System.Text.Json.JsonException) return "This locations file is not valid JSON.";
        if (error is UnauthorizedAccessException) return "Windows denied access to the selected file or folder.";
        if (error is IOException) return "The selected file could not be read or written.";
        if (error is Win32Exception) return "Windows could not give the tool access to the game.";
        if (error is InvalidDataException || error is InvalidOperationException) return error.Message;
        return "The action failed. Open Diagnostics for details.";
    }
    public static string Hint(Exception error)
    {
        if (error is GameException issue) return issue.Hint;
        if (error is System.Text.Json.JsonException || error is InvalidDataException)
            return "Try a backup of your locations file. Save the support log if you need help.";
        if (error is IOException || error is UnauthorizedAccessException)
            return "Check the folder permissions or choose another folder.";
        return "Try Reconnect. If the problem returns, save the support log and send it with your bug report.";
    }
}

sealed class GameException : InvalidOperationException
{
    public string Hint { get; }
    public string Code { get; }
    public GameException(string code, string message, string hint, Exception? inner = null) : base(message, inner)
    {
        Code = code;
        Hint = hint;
        Data["Code"] = code;
    }
}

