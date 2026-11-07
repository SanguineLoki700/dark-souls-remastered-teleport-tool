using System.Globalization;

sealed class PointEditor : Form
{
    readonly TextBox name = new();
    readonly TextBox[] coordinates = new TextBox[3];
    readonly SoulsLabel feedback = new();
    Position original;
    public string LocationName => name.Text.Trim();
    public Position Position
    {
        get
        {
            var values = coordinates.Select(box => Parse(box.Text)).ToArray();
            return original with { X = values[0], Y = values[1], Z = values[2] };
        }
    }
    static float Parse(string text)
    {
        bool valid = float.TryParse(text.Trim(), NumberStyles.Float, CultureInfo.CurrentCulture, out var value) ||
            float.TryParse(text.Trim(), NumberStyles.Float, CultureInfo.InvariantCulture, out value);
        if (!valid || !float.IsFinite(value)) throw new InvalidDataException("Enter valid numbers for X, Y and Z.");
        return value;
    }
    public PointEditor(string title, Position point, Func<Position> capture, Func<Position, Task> travel)
    {
        original = point;
        Text = "Dark Souls Teleport · Coordinates";
        Icon = Theme.AppIcon;
        ClientSize = new Size(490, 395);
        FormBorderStyle = FormBorderStyle.FixedDialog;
        StartPosition = FormStartPosition.CenterParent;
        MaximizeBox = MinimizeBox = false;
        BackColor = Theme.Ink;
        ForeColor = Theme.Text;
        Font = Theme.CreateFont(19);
        Label("Location name", 30, 27, 430, 24);
        name.SetBounds(30, 56, 430, 28);
        name.Text = title;
        name.BackColor = Color.FromArgb(34, 35, 30);
        name.ForeColor = Theme.Text;
        name.AccessibleName = "Location name";
        Controls.Add(name);
        for (int index = 0; index < 3; index++)
        {
            Label("XYZ"[index].ToString(), 30 + index * 150, 105, 130, 25);
            var input = new TextBox
            {
                BackColor = Color.FromArgb(34, 35, 30), ForeColor = Theme.Text,
                Bounds = new Rectangle(30 + index * 150, 136, 130, 28),
                AccessibleName = "XYZ"[index].ToString()
            };
            coordinates[index] = input;
            Controls.Add(input);
        }
        SetCoordinates(point);
        var current = new SoulsButton { Text = "Use current player position", Bounds = new Rectangle(30, 189, 430, 35) };
        current.Click += (_, _) =>
        {
            try { SetCoordinates(capture()); feedback.Text = "Current position loaded."; }
            catch (Exception error) { feedback.Text = error.Message; }
        };
        Controls.Add(current);
        feedback.SetBounds(30, 240, 430, 55);
        feedback.TextPixels = 13;
        feedback.ForeColor = Theme.Muted;
        feedback.Text = "Changes are saved when you click Save.";
        Controls.Add(feedback);
        var confirm = new SoulsButton { Text = "Save", Bounds = new Rectangle(30, 322, 130, 38) };
        confirm.Click += (_, _) => Save();
        var teleport = new SoulsButton { Text = "Teleport", Bounds = new Rectangle(180, 322, 130, 38) };
        teleport.Click += async (_, _) =>
        {
            try
            {
                var position = Position;
                teleport.Enabled = false;
                await travel(position);
                feedback.Text = "Teleport complete.";
            }
            catch (Exception error) { feedback.Text = error.Message; }
            finally { if (!IsDisposed) teleport.Enabled = true; }
        };
        var close = new SoulsButton { Text = "Close", Bounds = new Rectangle(330, 322, 130, 38), DialogResult = DialogResult.Cancel };
        Controls.AddRange(new Control[] { confirm, teleport, close });
        AcceptButton = confirm;
        CancelButton = close;
    }
    void Save()
    {
        try
        {
            _ = Position;
            if (LocationName.Length == 0) { name.Focus(); feedback.Text = "Enter a name for this location."; return; }
            DialogResult = DialogResult.OK;
        }
        catch (Exception error) { feedback.Text = error.Message; }
    }
    void SetCoordinates(Position position)
    {
        original = position;
        var values = new[] { position.X, position.Y, position.Z };
        for (int index = 0; index < values.Length; index++)
            coordinates[index].Text = values[index].ToString("G9", CultureInfo.CurrentCulture);
    }
    void Label(string text, int x, int y, int w, int h)
    {
        Controls.Add(new SoulsLabel { Text = text, Bounds = new Rectangle(x, y, w, h), TextPixels = 15 });
    }
    protected override void OnPaintBackground(PaintEventArgs e) => Theme.Background(e.Graphics, ClientRectangle);
    protected override bool ProcessCmdKey(ref System.Windows.Forms.Message msg, Keys keyData)
    {
        if (keyData == (Keys.Control | Keys.Enter)) { Save(); return true; }
        return base.ProcessCmdKey(ref msg, keyData);
    }
}

