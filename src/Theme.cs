using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.Reflection;
using System.Runtime.InteropServices;

static class Theme
{
    public static readonly Color Ink = Color.FromArgb(14, 14, 12);

    public static readonly Color BrightGold = Color.FromArgb(220, 183, 115);
    public static readonly Color Text = Color.FromArgb(232, 228, 214);
    public static readonly Color Muted = Color.FromArgb(179, 173, 153);
    public static readonly Image Inventory = Resource("inventory.jpg");

    public static readonly Image Logo = Resource("logo.png");
    public static readonly Icon AppIcon = LoadIcon();
    static readonly Image Selection = Resource("system-menu.png");
    static readonly Image Cloth = Resource("cloth-frame.png");
    static readonly System.Drawing.Text.PrivateFontCollection FontCollection = new();
    static IntPtr fontData;
    static IntPtr fontRegistration;
    static readonly FontFamily Typeface = LoadTypeface();
    static readonly Dictionary<int, Font> RenderFonts = new();

    static FontFamily LoadTypeface()
    {
        using var stream = ResourceStream("EBGaramond-Medium.ttf");
        using var bytes = new MemoryStream();
        stream.CopyTo(bytes);
        byte[] data = bytes.ToArray();
        fontData = Marshal.AllocHGlobal(data.Length);
        Marshal.Copy(data, 0, fontData, data.Length);
        FontCollection.AddMemoryFont(fontData, data.Length);
        uint count = 0;
        fontRegistration = AddFontMemResourceEx(fontData, (uint)data.Length, IntPtr.Zero, ref count);
        AppDomain.CurrentDomain.ProcessExit += (_, _) =>
        {
            foreach (var font in RenderFonts.Values) font.Dispose();
            FontCollection.Dispose();
            if (fontRegistration != IntPtr.Zero) RemoveFontMemResourceEx(fontRegistration);
            Marshal.FreeHGlobal(fontData);
        };
        return FontCollection.Families[0];
    }
    public static Font CreateFont(float pixels) =>
        new(Typeface, Math.Max(12, pixels), FontStyle.Regular, GraphicsUnit.Pixel);
    static Font RenderFont(float pixels)
    {
        int size = Math.Max(12, (int)Math.Round(pixels * 1.3f));
        if (!RenderFonts.TryGetValue(size, out var font))
        {
            font = CreateFont(size);
            RenderFonts[size] = font;
        }
        return font;
    }
    [DllImport("gdi32.dll")]
    static extern IntPtr AddFontMemResourceEx(IntPtr data, uint length, IntPtr reserved, ref uint count);
    [DllImport("gdi32.dll")]
    static extern bool RemoveFontMemResourceEx(IntPtr handle);

    static readonly Bitmap LeatherTile = Extract(Cloth, new Rectangle(80, 90, 256, 192));

    static Stream ResourceStream(string name)
    {
        var assembly = Assembly.GetExecutingAssembly();
        return assembly.GetManifestResourceStream(
            assembly.GetManifestResourceNames().Single(value => value.EndsWith("." + name, StringComparison.Ordinal)))!;
    }
    static Icon LoadIcon()
    {
        using var stream = ResourceStream("app.ico");
        using var source = new Icon(stream);
        return (Icon)source.Clone();
    }
    public static Image Resource(string name)
    {
        using var stream = ResourceStream(name);
        using var original = Image.FromStream(stream);
        return new Bitmap(original);
    }
    static Bitmap Extract(Image source, Rectangle rectangle)
    {
        var bitmap = new Bitmap(rectangle.Width, rectangle.Height);
        using var graphics = Graphics.FromImage(bitmap);
        graphics.DrawImage(source, new Rectangle(Point.Empty, rectangle.Size), rectangle, GraphicsUnit.Pixel);
        return bitmap;
    }
    public static void Leather(Graphics graphics, Rectangle bounds)
    {
        using var brush = new TextureBrush(LeatherTile, WrapMode.TileFlipXY);
        graphics.FillRectangle(brush, bounds);
    }
    static void Crop(Graphics g, Image source, Rectangle destination, Rectangle crop)
    {
        if (destination.Width > 0 && destination.Height > 0)
            g.DrawImage(source, destination, crop, GraphicsUnit.Pixel);
    }

    public static void Background(Graphics g, Rectangle r)
    {
        g.Clear(Color.Black);
        int[] sourceX = { 0, 48, 552, 600 };
        int[] sourceY = { 0, 48, 652, 700 };
        int[] targetX = { r.Left, r.Left + 48, r.Right - 48, r.Right };
        int[] targetY = { r.Top, r.Top + 48, r.Bottom - 48, r.Bottom };
        for (int row = 0; row < 3; row++)
        for (int column = 0; column < 3; column++)
            Crop(g, Cloth, Rectangle.FromLTRB(targetX[column], targetY[row], targetX[column + 1], targetY[row + 1]),
                Rectangle.FromLTRB(sourceX[column], sourceY[row], sourceX[column + 1], sourceY[row + 1]));
    }

    public static void DrawLogo(Graphics g, Rectangle bounds)
    {
        using var attributes = new ImageAttributes();
        attributes.SetColorKey(Color.Black, Color.FromArgb(20, 20, 20));
        var title = new Rectangle(bounds.Left + (bounds.Width - 310) / 2, bounds.Top + 7, 310, 43);
        g.DrawImage(Logo, title, 88, 148, 598, 83, GraphicsUnit.Pixel, attributes);
        var remastered = new Rectangle(bounds.Left + (bounds.Width - 155) / 2, bounds.Top + 51, 155, 14);
        g.DrawImage(Logo, remastered, 228, 241, 312, 30, GraphicsUnit.Pixel, attributes);
        DrawText(g, "T E L E P O R T", new Rectangle(bounds.Left, bounds.Top + 62, bounds.Width, 27),
            18, BrightGold, ContentAlignment.MiddleCenter);
    }

    public static void DrawText(Graphics g, string text, Rectangle bounds, float pixels, Color color,
        ContentAlignment alignment = ContentAlignment.MiddleLeft, bool ellipsis = true)
    {
        if (bounds.Width <= 0 || bounds.Height <= 0 || string.IsNullOrEmpty(text)) return;
        using var format = new StringFormat(StringFormat.GenericTypographic)
        {
            Alignment = alignment is ContentAlignment.MiddleCenter or ContentAlignment.TopCenter ?
                StringAlignment.Center : alignment == ContentAlignment.MiddleRight ? StringAlignment.Far : StringAlignment.Near,
            LineAlignment = StringAlignment.Center,
            Trimming = ellipsis ? StringTrimming.EllipsisCharacter : StringTrimming.None,
            FormatFlags = StringFormatFlags.NoWrap
        };
        using var brush = new SolidBrush(color);
        var saved = g.Save();
        g.SetClip(bounds, CombineMode.Intersect);
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
        g.DrawString(text, RenderFont(pixels), brush, bounds, format);
        g.Restore(saved);
    }

    public static void Button(Graphics g, Rectangle r, bool active)
    {
        Leather(g, r);
        if (active)
            Crop(g, Selection, r, new Rectangle(620, 197, 125, 31));
        Crop(g, Inventory, new Rectangle(r.Left, r.Top, r.Width, 3), new Rectangle(310, 351, 900, 6));
        Crop(g, Inventory, new Rectangle(r.Left, r.Bottom - 3, r.Width, 3), new Rectangle(310, 489, 900, 6));
    }

    public static void Divider(Graphics g, Rectangle r)
    {
        int ornament = Math.Min(35, r.Width);
        int half = (r.Width - ornament) / 2;
        Crop(g, Inventory, new Rectangle(r.Left, r.Top, half, r.Height), new Rectangle(166, 489, 514, 21));
        Crop(g, Inventory, new Rectangle(r.Left + half, r.Top, ornament, r.Height), new Rectangle(680, 489, 70, 21));
        Crop(g, Inventory, new Rectangle(r.Left + half + ornament, r.Top, r.Width - half - ornament, r.Height),
            new Rectangle(750, 489, 480, 21));
    }
}

sealed class SoulsLabel : Control
{
    public float TextPixels { get; set; } = 14;
    public ContentAlignment TextAlign { get; set; } = ContentAlignment.MiddleLeft;
    public SoulsLabel()
    {
        SetStyle(ControlStyles.UserPaint | ControlStyles.SupportsTransparentBackColor |
            ControlStyles.OptimizedDoubleBuffer, true);
        BackColor = Color.Transparent;
        ForeColor = Theme.Text;
        AccessibleRole = AccessibleRole.StaticText;
        TabStop = false;
    }
    protected override void OnPaint(PaintEventArgs e) =>
        Theme.DrawText(e.Graphics, Text, ClientRectangle, TextPixels, ForeColor, TextAlign);
    protected override void OnTextChanged(EventArgs e) { base.OnTextChanged(e); Invalidate(); }
}

sealed class SoulsButton : Button
{
    bool hover;
    bool pressed;
    public float TextPixels { get; set; } = 14;
    public SoulsButton()
    {
        FlatStyle = FlatStyle.Flat;
        FlatAppearance.BorderSize = 0;
        BackColor = Theme.Ink;
        ForeColor = Theme.Text;
        Cursor = Cursors.Hand;
        SetStyle(ControlStyles.UserPaint | ControlStyles.OptimizedDoubleBuffer, true);
    }
    protected override void OnMouseEnter(EventArgs e) { hover = true; Invalidate(); base.OnMouseEnter(e); }
    protected override void OnMouseLeave(EventArgs e) { hover = false; pressed = false; Invalidate(); base.OnMouseLeave(e); }
    protected override void OnMouseDown(MouseEventArgs e) { pressed = true; Invalidate(); base.OnMouseDown(e); }
    protected override void OnMouseUp(MouseEventArgs e) { pressed = false; Invalidate(); base.OnMouseUp(e); }
    protected override void OnLostFocus(EventArgs e) { pressed = false; hover = false; Invalidate(); base.OnLostFocus(e); }
    protected override void OnMouseCaptureChanged(EventArgs e)
    {
        if (!Capture) pressed = false;
        Invalidate();
        base.OnMouseCaptureChanged(e);
    }
    protected override void OnPaint(PaintEventArgs e)
    {
        bool pointerInside = ClientRectangle.Contains(PointToClient(Cursor.Position));
        Theme.Button(e.Graphics, ClientRectangle, Enabled && (pressed || hover && pointerInside));
        Theme.DrawText(e.Graphics, Text, ClientRectangle, TextPixels,
            Enabled ? Theme.Text : Theme.Muted, ContentAlignment.MiddleCenter);
    }
}

sealed class SoulsList : ListBox
{
    public List<Checkpoint> Rows { get; private set; } = new();
    public float TextPixels { get; set; } = 14;
    public Checkpoint? Selected => SelectedIndex >= 0 && SelectedIndex < Rows.Count ? Rows[SelectedIndex] : null;
    public SoulsList()
    {
        DrawMode = DrawMode.OwnerDrawFixed;
        ItemHeight = 29;
        BackColor = Theme.Ink;
        ForeColor = Theme.Text;
        BorderStyle = BorderStyle.None;
        IntegralHeight = false;
        AccessibleName = "Saved locations";
        SetStyle(ControlStyles.UserPaint | ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer, true);
    }
    public void SetRows(IEnumerable<Checkpoint> items)
    {
        var selected = Selected?.Id;
        BeginUpdate();
        Rows = items.ToList();
        Items.Clear();
        Items.AddRange(Rows.Select(item => (object)item.Name).ToArray());
        int index = Rows.FindIndex(item => item.Id == selected);
        if (Rows.Count > 0) SelectedIndex = index >= 0 ? index : 0;
        EndUpdate();
        if (IsHandleCreated) ShowScrollBar(Handle, 1, false);
    }
    protected override void OnPaint(PaintEventArgs e)
    {
        Theme.Leather(e.Graphics, ClientRectangle);
        int top = Items.Count == 0 ? 0 : TopIndex;
        for (int index = top; index < Rows.Count && (index - top) * ItemHeight < Height; index++)
        {
            var bounds = new Rectangle(0, (index - top) * ItemHeight, ClientSize.Width, ItemHeight);
            OnDrawItem(new DrawItemEventArgs(e.Graphics, Font, bounds, index,
                index == SelectedIndex ? DrawItemState.Selected : DrawItemState.None));
        }
        int visible = Math.Max(1, Height / ItemHeight);
        if (Rows.Count > visible)
        {
            using var track = new Pen(Color.FromArgb(60, 49, 32), 1);
            using var thumb = new Pen(Color.FromArgb(172, 146, 100), 2);
            e.Graphics.DrawLine(track, Width - 3, 0, Width - 3, Height);
            int thumbHeight = Math.Max(12, Height * visible / Rows.Count);
            int thumbTop = (Height - thumbHeight) * top / Math.Max(1, Rows.Count - visible);
            e.Graphics.DrawLine(thumb, Width - 3, thumbTop, Width - 3, thumbTop + thumbHeight);
        }
    }
    protected override void OnSelectedIndexChanged(EventArgs e)
    {
        base.OnSelectedIndexChanged(e);
        if (IsHandleCreated) ShowScrollBar(Handle, 1, false);
        Invalidate();
    }
    protected override void OnMouseWheel(MouseEventArgs e)
    {
        if (Items.Count > 0)
        {
            int maximum = Math.Max(0, Items.Count - Math.Max(1, Height / ItemHeight));
            TopIndex = Math.Clamp(TopIndex + (e.Delta > 0 ? -3 : 3), 0, maximum);
        }
        if (e is HandledMouseEventArgs handled) handled.Handled = true;
        if (IsHandleCreated) ShowScrollBar(Handle, 1, false);
        Invalidate();
    }
    protected override void OnResize(EventArgs e)
    {
        base.OnResize(e);
        if (IsHandleCreated) ShowScrollBar(Handle, 1, false);
    }
    [DllImport("user32.dll")]
    static extern bool ShowScrollBar(IntPtr window, int bar, bool show);
    protected override void OnDrawItem(DrawItemEventArgs e)
    {
        if (e.Index < 0 || e.Index >= Rows.Count) return;
        bool selected = (e.State & DrawItemState.Selected) != 0;
        Theme.Button(e.Graphics, e.Bounds, selected);
        var text = e.Bounds;
        text.X += 22;
        text.Width -= 34;
        Theme.DrawText(e.Graphics, Rows[e.Index].Name, text, TextPixels, Theme.Text);
        if (selected)
            Theme.DrawText(e.Graphics, ">", new Rectangle(e.Bounds.X + 5, e.Bounds.Y, 14, e.Bounds.Height),
                TextPixels + 2, Theme.BrightGold);
    }
}



