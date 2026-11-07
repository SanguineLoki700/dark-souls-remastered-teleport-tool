using System.Runtime.InteropServices;
using System.Text.Json;

sealed class MainForm : Form
{
    const int PageSize = 50;
    const string DefaultHint = "F5: Capture checkpoint    F9: Return    Select a record to travel.";
    readonly Library library;
    readonly SoulsList list = new();
    readonly TextBox name = new();
    readonly TextBox search = new();
    readonly SoulsLabel connection = new();
    readonly SoulsLabel quick = new();
    readonly SoulsLabel notice = new();
    readonly SoulsLabel count = new();
    readonly SoulsLabel empty = new();
    readonly SoulsLabel nameTitle = new();
    readonly SoulsLabel savedTitle = new();
    readonly SoulsLabel findTitle = new();
    readonly SoulsLabel pageNumber = new();
    readonly SoulsLabel footer = new();
    readonly SoulsButton capture = new();
    readonly SoulsButton back = new();
    readonly SoulsButton save = new();
    readonly SoulsButton travel = new();
    readonly SoulsButton edit = new();
    readonly SoulsButton remove = new();
    readonly SoulsButton manual = new();
    readonly SoulsButton previous = new();
    readonly SoulsButton next = new();
    readonly SoulsButton import = new();
    readonly SoulsButton export = new();
    readonly SoulsButton diagnostics = new();
    readonly SoulsButton reconnect = new();
    readonly System.Windows.Forms.Timer timer = new() { Interval = 500 };
    readonly System.Windows.Forms.Timer searchDelay = new() { Interval = 150 };
    List<Checkpoint> filteredRecords = new();
    readonly HashSet<int> hotkeys = new();

    readonly ToolTip tooltip = new();
    Game? game;
    Position? current;
    bool busy;
    bool storing;
    bool libraryReady;
    bool polling;
    int retries;
    int page;
    int pages = 1;
    float scale = 1;
    Exception? connectionError;
    Exception? actionError;
    string lastConnectionKey = "";
    Position? quickPosition;
    bool quickPending;
    bool quickCaptured;
    int filterVersion;
    string status = "○  Connecting to Dark Souls…";
    DateTimeOffset? noticeDeadline;

    public MainForm(Library library)
    {
        this.library = library;
        Text = "Dark Souls Remastered Teleport";
        Icon = Theme.AppIcon;
        ClientSize = new Size(596, 700);
        MinimumSize = new Size(536, 639);
        StartPosition = FormStartPosition.CenterScreen;
        AutoScaleMode = AutoScaleMode.None;
        BackColor = Theme.Ink;
        ForeColor = Theme.Text;
        Font = Theme.CreateFont(18);
        DoubleBuffered = true;
        KeyPreview = true;

        AddLabel(connection, "", ContentAlignment.MiddleCenter);
        AddLabel(quick, "No quick checkpoint captured", ContentAlignment.MiddleCenter);
        AddLabel(nameTitle, "Location name (optional)");
        AddLabel(savedTitle, "Saved locations");
        AddLabel(findTitle, "Find");
        AddLabel(count, "", ContentAlignment.MiddleRight);
        AddLabel(pageNumber, "", ContentAlignment.MiddleCenter);
        AddLabel(empty, "", ContentAlignment.MiddleCenter);
        AddLabel(notice, DefaultHint);
        AddLabel(footer, $"SanguineLoki700  ·  {Program.Version}");

        AddButton(capture, "Capture checkpoint  ·  F5", CaptureMark);
        AddButton(back, "Return  ·  F9", () => _ = Travel(quickPosition));
        AddButton(manual, "Manual coordinates…", () => _ = Run(Manual));
        AddButton(save, "Save position", () => _ = Run(Save));
        AddButton(travel, "Travel", () => _ = Travel(list.Selected?.Position));
        AddButton(edit, "Edit", () => _ = Run(Edit));
        AddButton(remove, "Remove", () => _ = Run(Remove));
        AddButton(previous, "<", () => ChangePage(-1));
        AddButton(next, ">", () => ChangePage(1));
        AddButton(import, "Import", () => _ = Run(Import));
        AddButton(export, "Export", () => _ = Run(Export));
        AddButton(diagnostics, "Diagnostics", Diagnostics);
        AddButton(reconnect, "Reconnect", Reconnect);
        foreach (var input in new[] { name, search })
        {
            input.BorderStyle = BorderStyle.FixedSingle;
            input.BackColor = Color.FromArgb(34, 35, 30);
            input.ForeColor = Theme.Text;
            Controls.Add(input);
        }
        name.AccessibleName = "Location name";
        search.AccessibleName = "Find locations";
        name.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter) { e.SuppressKeyPress = true; _ = Run(Save); }
        };
        search.TextChanged += (_, _) => { searchDelay.Stop(); searchDelay.Start(); };
        searchDelay.Tick += async (_, _) => { searchDelay.Stop(); page = 0; await RefreshList(); };
        list.SelectedIndexChanged += (_, _) => UpdateActions();
        list.DoubleClick += (_, _) => _ = Travel(list.Selected?.Position);
        list.KeyDown += (_, e) =>
        {
            if (e.KeyCode == Keys.Enter) { e.SuppressKeyPress = true; _ = Travel(list.Selected?.Position); }
            if (e.KeyCode == Keys.Escape) { list.ClearSelected(); e.SuppressKeyPress = true; }
        };
        Controls.Add(list);
        empty.BringToFront();
        Resize += (_, _) => LayoutCompact();
        LayoutCompact();
        UpdateActions();

        Shown += async (_, _) =>
        {
            foreach (var pair in new[] { (51, Keys.F5), (52, Keys.F9) })
                if (RegisterHotKey(Handle, pair.Item1, 0x4000, (uint)pair.Item2)) hotkeys.Add(pair.Item1);
            timer.Start();
            Message("Loading saved locations…");
            var loading = Task.Run(library.Load);
            await Poll();
            try
            {
                await loading;
                if (IsDisposed) return;
                libraryReady = true;
                await RefreshList();
                if (!quickCaptured) quickPosition = library.Data.Quick;
                RefreshQuick();
                _ = FlushQuick();
                Message(DefaultHint);
            }
            catch (Exception error)
            {
                if (IsDisposed) return;
                Message("The locations file could not be loaded. Open Diagnostics.", true);
                actionError = error;
                SupportLog.Error("Location action", error);
            }
        };
        timer.Tick += async (_, _) =>
        {
            if (noticeDeadline.HasValue && DateTimeOffset.UtcNow >= noticeDeadline.Value)
            {
                notice.Text = DefaultHint;
                notice.ForeColor = Theme.Text;
                noticeDeadline = null;
            }
            await Poll();
        };
        FormClosed += (_, _) =>
        {
            timer.Dispose();
            searchDelay.Dispose();
            tooltip.Dispose();
            foreach (int id in hotkeys) UnregisterHotKey(Handle, id);
            game?.Dispose();
        };
    }

    void AddLabel(SoulsLabel label, string text, ContentAlignment alignment = ContentAlignment.MiddleLeft)
    {
        label.Text = text;
        label.TextAlign = alignment;
        label.ForeColor = Theme.Text;
        Controls.Add(label);
    }
    void AddButton(SoulsButton button, string text, Action action)
    {
        button.Text = text;
        button.Click += (_, _) => action();
        Controls.Add(button);
    }
    void Place(Control control, float x, float y, float width, float height, float fontSize = 14)
    {
        control.SetBounds((int)Math.Round(x * scale), (int)Math.Round(y * scale),
            (int)Math.Round(width * scale), (int)Math.Round(height * scale));
        if (control is SoulsLabel label) { label.TextPixels = fontSize * scale; label.Invalidate(); }
        if (control is SoulsButton button) { button.TextPixels = fontSize * scale; button.Invalidate(); }
        if (control is TextBox)
        {
            var old = control.Font;
            control.Font = Theme.CreateFont(18 * scale);
            if (!ReferenceEquals(old, Font)) old.Dispose();
        }
    }

    void LayoutCompact()
    {
        if (ClientSize.Width <= 0 || ClientSize.Height <= 0) return;
        scale = Math.Min(Math.Min(ClientSize.Width / 600f, ClientSize.Height / 660f), 1.4f);
        float w = ClientSize.Width / scale;
        float h = ClientSize.Height / scale;
        SuspendLayout();
        Place(connection, 34, 109, w - 68, 25, 13);
        Place(capture, 33, 143, w / 2 - 38, 42);
        Place(back, w / 2 + 5, 143, w / 2 - 38, 42);
        Place(quick, 36, 190, w - 72, 23, 12);
        Place(nameTitle, 36, 211, w - 240, 27, 14);
        Place(manual, w - 203, 211, 170, 22, 12);
        Place(name, 36, 237, w - 208, 34);
        Place(save, w - 164, 233, 131, 42);
        Place(savedTitle, 36, 279, w - 180, 28, 16);
        Place(count, w - 196, 281, 160, 26, 12);
        Place(findTitle, 36, 308, 47, 31, 13);
        Place(search, 83, 309, w - 119, 30);
        float rowsHeight = Math.Max(29, (float)Math.Floor((h - 516) / 29) * 29);
        Place(list, 36, 350, w - 72, rowsHeight);
        list.TextPixels = 14 * scale;
        list.ItemHeight = Math.Max(20, (int)Math.Round(29 * scale));
        Place(empty, 50, 350 + (h - 516) / 2 - 40, w - 100, 80, 14);
        float third = (w - 82) / 3;
        Place(travel, 33, h - 157, third, 38, 15);
        Place(edit, 41 + third, h - 157, third, 38, 15);
        Place(remove, 49 + third * 2, h - 157, third, 38, 15);
        Place(previous, 33, h - 111, 37, 29, 18);
        Place(pageNumber, 77, h - 111, 82, 29, 12);
        Place(next, 167, h - 111, 37, 29, 18);
        Place(import, w - 218, h - 111, 91, 29, 12);
        Place(export, w - 123, h - 111, 90, 29, 12);
        Place(notice, 36, h - 83, w - 72, 39, 11);
        Place(footer, 36, h - 48, w - 282, 28, 9);
        Place(diagnostics, w - 240, h - 48, 101, 28, 12);
        Place(reconnect, w - 135, h - 48, 102, 28, 12);
        ResumeLayout();
        Invalidate(true);
    }

    protected override void OnPaintBackground(PaintEventArgs e)
    {
        var saved = e.Graphics.Save();
        e.Graphics.ScaleTransform(scale, scale);
        int width = (int)Math.Ceiling(ClientSize.Width / scale);
        int height = (int)Math.Ceiling(ClientSize.Height / scale);
        Theme.Background(e.Graphics, new Rectangle(0, 0, width, height));
        Theme.DrawLogo(e.Graphics, new Rectangle(34, 17, width - 68, 92));
        Theme.Divider(e.Graphics, new Rectangle(34, 136, width - 68, 8));
        Theme.Divider(e.Graphics, new Rectangle(34, 275, width - 68, 8));
        Theme.Divider(e.Graphics, new Rectangle(34, 340, width - 68, 8));
        Theme.Divider(e.Graphics, new Rectangle(34, height - 165, width - 68, 8));
        Theme.Divider(e.Graphics, new Rectangle(34, height - 52, width - 68, 7));
        e.Graphics.Restore(saved);
    }

    async Task Run(Func<Task> action)
    {
        if (busy || storing || !libraryReady) return;
        try { await action(); }
        catch (Exception error) { ReportError("Location action", error); }
    }
    void ReportError(string context, Exception error)
    {
        actionError = error;
        SupportLog.Error(context, error);
        Message(SupportLog.UserMessage(error), true);
    }
    async Task Commit(Action operation)
    {
        storing = true;
        UpdateActions();
        bool saved = false;
        try { await Task.Run(operation); saved = true; }
        finally
        {
            storing = false;
            if (!IsDisposed)
            {
                UpdateActions();
                if (saved) _ = FlushQuick();
            }
        }
    }
    async Task FlushQuick()
    {
        if (!quickPending || !libraryReady || storing || quickPosition == null) return;
        var point = quickPosition;
        quickPending = false;
        try { await Commit(() => library.Change(data => data.Quick = point)); }
        catch (Exception error)
        {
            quickPending = true;
            actionError = error;
            SupportLog.Error("Save quick checkpoint", error);
            if (!IsDisposed) Message("Checkpoint captured, but it could not be saved to disk. Open Diagnostics.", true);
        }
    }
    void Message(string text, bool error = false)
    {
        SupportLog.Event(text);
        notice.Text = text;
        tooltip.SetToolTip(notice, text);
        notice.ForeColor = error ? Color.FromArgb(235, 163, 119) : Theme.Text;
        noticeDeadline = text == DefaultHint ? null : DateTimeOffset.UtcNow.AddSeconds(error ? 30 : 8);
    }
    async Task Poll()
    {
        if (polling || busy || IsDisposed) return;
        polling = true;
        try
        {
            if (game == null || !game.Connected)
            {
                game?.Dispose();
                game = null;
                current = null;
                if (retries++ % 4 == 0)
                {
                    game = await Task.Run(Game.Connect);
                    if (IsDisposed) { game.Dispose(); game = null; return; }
                }
            }
            if (game != null)
            {
                current = game.Snapshot().Position;
                if (connectionError != null) SupportLog.Event("Character ready");
                connectionError = null;
                lastConnectionKey = "";
                status = "●  Connected and ready";
                connection.ForeColor = Color.FromArgb(196, 209, 166);
            }
            else status = "○  Waiting for Dark Souls / player…";
        }
        catch (GameException error) when (error.Code == "game_not_running")
        {
            RecordConnectionError(error);
            current = null;
            status = "○  Waiting for Dark Souls / player…";
            connection.ForeColor = Theme.Muted;
        }
        catch (Exception error)
        {
            RecordConnectionError(error);
            current = null;
            status = "○  Connection unavailable · open Diagnostics";
            connection.ForeColor = Color.FromArgb(230, 187, 130);
        }
        finally
        {
            polling = false;
            if (!IsDisposed)
            {
                connection.Text = status;
                UpdateActions();
                if (hotkeys.Count < 2) tooltip.SetToolTip(capture, "This shortcut is in use by another program. Use the button.");
            }
        }
    }
    void RecordConnectionError(Exception error)
    {
        connectionError = error;
        string key = error.GetType().Name + error.Message;
        if (lastConnectionKey != key) SupportLog.Error("Game connection", error);
        lastConnectionKey = key;
    }

    void UpdateActions()
    {
        capture.Enabled = current != null && !busy;
        save.Enabled = current != null && libraryReady && !busy && !storing;
        manual.Enabled = libraryReady && !busy && !storing;
        back.Enabled = current != null && quickPosition != null && !busy;
        travel.Enabled = current != null && list.Selected != null && !busy;
        edit.Enabled = remove.Enabled = list.Selected != null && libraryReady && !busy && !storing;
        import.Enabled = export.Enabled = libraryReady && !busy && !storing;
        previous.Enabled = page > 0;
        next.Enabled = page + 1 < pages;
    }
    Position CapturePosition() =>
        game?.Snapshot().Position ?? throw new InvalidOperationException("Load a character in the game.");

    void CaptureMark()
    {
        if (busy) return;
        try
        {
            quickPosition = CapturePosition();
            quickCaptured = true;
            quickPending = true;
            RefreshQuick();
            UpdateActions();
            Message("Checkpoint captured. Press F9 to return.");
            _ = FlushQuick();
        }
        catch (Exception error) { ReportError("Capture checkpoint", error); }
    }
    string UniqueName(string title, Guid? except = null)
    {
        if (string.IsNullOrWhiteSpace(title)) title = "Local " + (library.Data.Locations.Count + 1);
        var names = library.Data.Locations.Where(item => item.Id != except)
            .Select(item => item.Name).ToHashSet(StringComparer.OrdinalIgnoreCase);
        string result = title.Trim();
        int suffix = 1;
        while (names.Contains(result))
            result = $"{title.Trim()} ({suffix++})";
        return result;
    }
    async Task Save()
    {
        var checkpoint = new Checkpoint(Guid.NewGuid(), UniqueName(name.Text), CapturePosition(), DateTimeOffset.Now);
        await Commit(() => library.Change(data => data.Locations.Add(checkpoint)));
        if (IsDisposed) return;
        name.Clear();
        search.Clear();
        await RefreshList(checkpoint.Id);
        Message("Location saved.");
    }
    async Task Travel(Position? point)
    {
        if (busy || point == null) return;
        busy = true;
        UpdateActions();
        try
        {
            await ApplyTravel(point);
            Message("Teleport complete.");
        }
        catch (Exception error) { ReportError("Teleport", error); }
        finally { busy = false; await Poll(); }
    }
    void RefreshQuick()
    {
        quick.Text = quickPosition?.Coordinates ?? "No quick checkpoint captured";
        quick.ForeColor = Theme.Muted;
    }
    

    async Task RefreshList(Guid? select = null, bool rebuild = true)
    {
        searchDelay.Stop();
        if (rebuild)
        {
            int version = ++filterVersion;
            var records = library.Data.Locations;
            string query = search.Text.Trim();
            var result = await Task.Run(() => records.Where(item =>
                item.Name.Contains(query, StringComparison.OrdinalIgnoreCase))
                .OrderBy(item => item.Name, StringComparer.CurrentCultureIgnoreCase).ToList());
            if (IsDisposed || version != filterVersion) return;
            filteredRecords = result;
        }
        var filtered = filteredRecords;
        pages = Math.Max(1, (filtered.Count + PageSize - 1) / PageSize);
        if (select.HasValue) page = Math.Max(0, filtered.FindIndex(item => item.Id == select) / PageSize);
        page = Math.Clamp(page, 0, pages - 1);
        list.SetRows(filtered.Skip(page * PageSize).Take(PageSize));
        if (select.HasValue) list.SelectedIndex = list.Rows.FindIndex(item => item.Id == select);
        empty.Visible = list.Rows.Count == 0;
        empty.Text = filtered.Count == 0 && search.Text.Trim().Length > 0 ?
            "No matching locations." : "No locations saved\n\nSave a position above or import your records.";
        count.Text = $"{filtered.Count:N0} / {library.Data.Locations.Count:N0}";
        pageNumber.Text = $"{page + 1:N0} / {pages:N0}";
        UpdateActions();
    }
    void ChangePage(int delta)
    {
        page = Math.Clamp(page + delta, 0, pages - 1);
        _ = RefreshList(rebuild: false);
    }
    async Task Edit()
    {
        var selected = list.Selected;
        if (selected == null) return;
        using var editor = new PointEditor(selected.Name, selected.Position, CapturePosition, TravelEditor);
        if (editor.ShowDialog(this) != DialogResult.OK) return;
        string title = UniqueName(editor.LocationName, selected.Id);
        var point = editor.Position;
        await Commit(() => library.Change(data =>
        {
            int index = data.Locations.FindIndex(item => item.Id == selected.Id);
            data.Locations[index] = selected with { Name = title, Position = point };
        }));
        if (IsDisposed) return;
        await RefreshList(selected.Id);
        Message("Location updated.");
    }
    async Task TravelEditor(Position point)
    {
        if (busy) throw new InvalidOperationException("Wait for the current teleport to finish.");
        busy = true;
        UpdateActions();
        try
        {
            await ApplyTravel(point);
            Message("Teleport complete.");
        }
        finally { busy = false; await Poll(); }
    }
    Task ApplyTravel(Position point)
    {
        if (game == null) throw new InvalidOperationException("Open the game and load your character.");
        return game.Travel(point);
    }
    async Task Manual()
    {
        Position initial = current ?? quickPosition ?? new Position(0, 0, 0, 0, 0);
        using var editor = new PointEditor(UniqueName(name.Text), initial, CapturePosition, TravelEditor);
        if (editor.ShowDialog(this) != DialogResult.OK) return;
        var checkpoint = new Checkpoint(Guid.NewGuid(), UniqueName(editor.LocationName), editor.Position, DateTimeOffset.Now);
        await Commit(() => library.Change(data => data.Locations.Add(checkpoint)));
        if (IsDisposed) return;
        search.Clear();
        await RefreshList(checkpoint.Id);
        Message("Coordinates saved.");
    }
    async Task Remove()
    {
        var selected = list.Selected;
        if (selected == null) return;
        if (MessageBox.Show(this, $"Remove “{selected.Name}”?", "Remove location",
            MessageBoxButtons.YesNo, MessageBoxIcon.Question) != DialogResult.Yes) return;
        await Commit(() => library.Change(data => data.Locations.RemoveAll(item => item.Id == selected.Id)));
        if (IsDisposed) return;
        await RefreshList();
        Message("Location removed.");
    }
    async Task Import()
    {
        using var dialog = new OpenFileDialog { Filter = "Locations (*.json)|*.json", Title = "Import locations" };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        string path = dialog.FileName;
        int added = 0;
        Message("Importing locations…");
        await Commit(() => added = library.Import(path));
        if (IsDisposed) return;
        await RefreshList();
        Message($"{added:N0} locations imported.");
    }
    async Task Export()
    {
        using var dialog = new SaveFileDialog
        {
            Filter = "Locations (*.json)|*.json", FileName = "dark-souls-locations.json",
            Title = "Export locations", AddExtension = true, DefaultExt = "json"
        };
        if (dialog.ShowDialog(this) != DialogResult.OK) return;
        string path = dialog.FileName;
        await Commit(() => library.Export(path));
        if (!IsDisposed) Message("Locations exported.");
    }
    string DiagnosticText()
    {
        var text = new System.Text.StringBuilder(current != null ?
            "Connected to Dark Souls Remastered." : "Not connected to a loaded character.");
        if (connectionError != null)
            text.Append("\r\n\r\n").Append(SupportLog.UserMessage(connectionError))
                .Append("\r\n").Append(SupportLog.Hint(connectionError));
        string locationWord = library.Data.Locations.Count == 1 ? "location" : "locations";
        text.Append("\r\n\r\n").Append(libraryReady ?
            $"{library.Data.Locations.Count:N0} saved {locationWord} loaded." : actionError != null ?
            "Saved locations could not be loaded." : "Saved locations are loading.");
        if (actionError != null)
            text.Append("\r\n\r\nLast error\r\n").Append(SupportLog.UserMessage(actionError))
                .Append("\r\n").Append(SupportLog.Hint(actionError));
        if (hotkeys.Count < 2)
            text.Append("\r\n\r\nA shortcut is in use by another program. Close the other teleport tool or use the buttons.");
        return text.ToString();
    }
    string DiagnosticSnapshot() => JsonSerializer.Serialize(new
    {
        Environment = SupportLog.EnvironmentDetails,
        Status = status, Game = game?.Details, CurrentPosition = current,
        LocationsFile = library.FilePath, Locations = library.Data.Locations.Count,
        LibraryLoaded = libraryReady, StorageBusy = storing,
        RegisteredHotkeys = hotkeys.OrderBy(item => item).ToArray(),
        ConnectionError = connectionError?.ToString(), LastActionError = actionError?.ToString()
    }, Library.JsonOptions);

    void Diagnostics()
    {
        using var window = new Form
        {
            Text = "Dark Souls Teleport · Diagnostics", ClientSize = new Size(520, 390),
            Icon = Theme.AppIcon,
            StartPosition = FormStartPosition.CenterParent, BackColor = Theme.Ink, ForeColor = Theme.Text,
            FormBorderStyle = FormBorderStyle.FixedDialog, MaximizeBox = false, MinimizeBox = false
        };
        var body = new RichTextBox
        {
            Multiline = true, ReadOnly = true, WordWrap = true, ScrollBars = RichTextBoxScrollBars.Vertical,
            DetectUrls = false,
            BackColor = Theme.Ink, ForeColor = Theme.Text, BorderStyle = BorderStyle.None,
            Font = Theme.CreateFont(20), Text = DiagnosticText(),
            Bounds = new Rectangle(25, 24, 470, 275), TabStop = false
        };
        var feedback = new SoulsLabel { Bounds = new Rectangle(25, 302, 470, 25), TextPixels = 13 };
        var reconnectButton = new SoulsButton { Text = "Reconnect", Bounds = new Rectangle(25, 338, 133, 34) };
        reconnectButton.Click += (_, _) => Reconnect();
        var saveReport = new SoulsButton { Text = "Save support log", Bounds = new Rectangle(170, 338, 180, 34) };
        saveReport.Click += async (_, _) =>
        {
            using var dialog = new SaveFileDialog
            {
                Filter = "Support log (*.log)|*.log", FileName = "dark-souls-teleport-support.log",
                DefaultExt = "log", AddExtension = true
            };
            if (dialog.ShowDialog(window) != DialogResult.OK) return;
            string path = dialog.FileName;
            string snapshot = DiagnosticSnapshot();
            saveReport.Enabled = false;
            try
            {
                await Task.Run(() => SupportLog.Export(path, snapshot));
                if (!window.IsDisposed) feedback.Text = "Support log saved.";
            }
            catch (Exception error)
            {
                SupportLog.Error("Export support log", error);
                if (!window.IsDisposed) feedback.Text = "Could not save the log. Choose another folder.";
            }
            finally { if (!window.IsDisposed) saveReport.Enabled = true; }
        };
        var close = new SoulsButton { Text = "Close", Bounds = new Rectangle(362, 338, 133, 34), DialogResult = DialogResult.Cancel };
        window.Controls.AddRange(new Control[] { body, feedback, reconnectButton, saveReport, close });
        using var refresh = new System.Windows.Forms.Timer { Interval = 500 };
        refresh.Tick += (_, _) => { string text = DiagnosticText(); if (body.Text != text) body.Text = text; };
        refresh.Start();
        window.CancelButton = close;
        window.ShowDialog(this);
    }
    void Reconnect()
    {
        if (busy || polling) return;
        game?.Dispose();
        game = null;
        retries = 0;
        _ = Poll();
    }
    protected override bool ProcessCmdKey(ref System.Windows.Forms.Message msg, Keys keyData)
    {
        if (keyData == Keys.PageUp) { ChangePage(-1); return true; }
        if (keyData == Keys.PageDown) { ChangePage(1); return true; }
        return base.ProcessCmdKey(ref msg, keyData);
    }
    protected override void WndProc(ref System.Windows.Forms.Message m)
    {
        if (m.Msg == 0x312 && game != null && current != null && !busy)
        {
            IntPtr foreground = GetForegroundWindow();
            GetWindowThreadProcessId(foreground, out uint pid);
            if (pid == game.Id || foreground == Handle)
            {
                if (m.WParam.ToInt32() == 51) CaptureMark();
                if (m.WParam.ToInt32() == 52) _ = Travel(quickPosition);
            }
        }
        base.WndProc(ref m);
    }
    [DllImport("user32.dll")] static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint key);
    [DllImport("user32.dll")] static extern bool UnregisterHotKey(IntPtr window, int id);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
}



