use std::sync::atomic::{AtomicBool, Ordering};
static UI_TEST: AtomicBool = AtomicBool::new(false);
use crate::{
    app::{Command, Event, View},
    game::{wide, Position},
    storage::Location,
    theme::{rect, rgb, Theme, GOLD, MUTED, TEXT},
};
use std::{
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::mpsc::{Receiver, Sender},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::LibraryLoader::*,
    UI::{
        Controls::{Dialogs::*, *},
        HiDpi::*,
        Input::KeyboardAndMouse::*,
        WindowsAndMessaging::*,
    },
};
const CAP: i32 = 101;
const BACK: i32 = 102;
const MANUAL: i32 = 103;
const SAVE: i32 = 104;
const TRAVEL: i32 = 105;
const EDIT: i32 = 106;
const REMOVE: i32 = 107;
const PREV: i32 = 108;
const NEXT: i32 = 109;
const IMPORT: i32 = 110;
const EXPORT: i32 = 111;
const DIAG: i32 = 112;
const RECONNECT: i32 = 113;
const NAME: i32 = 201;
const FIND: i32 = 202;
const LIST: i32 = 203;
fn buttons() -> [(i32, &'static str, RECT); 13] {
    [
        (CAP, "Capture checkpoint  ·  F5", rect(33, 143, 262, 42)),
        (BACK, "Return  ·  F9", rect(305, 143, 262, 42)),
        (MANUAL, "Manual coordinates…", rect(397, 211, 170, 22)),
        (SAVE, "Save position", rect(436, 233, 131, 42)),
        (TRAVEL, "Travel", rect(33, 543, 172, 38)),
        (EDIT, "Edit", rect(213, 543, 172, 38)),
        (REMOVE, "Remove", rect(393, 543, 174, 38)),
        (PREV, "<", rect(33, 589, 37, 29)),
        (NEXT, ">", rect(167, 589, 37, 29)),
        (IMPORT, "Import", rect(382, 589, 91, 29)),
        (EXPORT, "Export", rect(477, 589, 90, 29)),
        (DIAG, "Diagnostics", rect(360, 652, 101, 28)),
        (RECONNECT, "Reconnect", rect(465, 652, 102, 28)),
    ]
}
struct Ui {
    hwnd: HWND,
    theme: Theme,
    view: View,
    commands: Sender<Command>,
    events: Receiver<Event>,
    name: HWND,
    find: HWND,
    list: HWND,
    hover: HWND,
    hover_row: Option<usize>,
    selected: Option<String>,
    hotkey_warning: bool,
    rows_key: String,
    visual_key: String,
    brush: HBRUSH,
    search: String,
    search_ticks: u32,
}
fn text(hwnd: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(hwnd).max(0) as usize;
        let mut t = vec![0u16; n + 1];
        let n = GetWindowTextW(hwnd, t.as_mut_ptr(), t.len() as i32);
        String::from_utf16_lossy(&t[..n as usize])
    }
}
unsafe fn child(parent: HWND, class: &str, label: &str, id: i32, r: RECT, style: u32) -> HWND {
    let hwnd = CreateWindowExW(
        0,
        wide(class).as_ptr(),
        wide(label).as_ptr(),
        WS_CHILD | WS_VISIBLE | style,
        r.left,
        r.top,
        r.right - r.left,
        r.bottom - r.top,
        parent,
        id as usize as _,
        GetModuleHandleW(null()),
        null(),
    );
    if !hwnd.is_null() && class == "BUTTON" {
        SetWindowSubclass(hwnd, Some(button_proc), 1, 0);
    }
    if !hwnd.is_null() && class == "LISTBOX" {
        SetWindowSubclass(hwnd, Some(list_proc), 2, 0);
    }
    hwnd
}
unsafe fn enable(hwnd: HWND, on: bool) {
    if (IsWindowEnabled(hwnd) != 0) != on {
        EnableWindow(hwnd, on as i32);
    }
}

type SubclassProc =
    Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM, usize, usize) -> LRESULT>;
// windows-sys exposes these helpers under its larger Shell feature. The UI only
// needs the three common-controls subclass functions, without adding that feature.
#[link(name = "comctl32")]
unsafe extern "system" {
    fn SetWindowSubclass(hwnd: HWND, proc: SubclassProc, id: usize, data: usize) -> i32;
    fn DefSubclassProc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT;
    fn RemoveWindowSubclass(hwnd: HWND, proc: SubclassProc, id: usize) -> i32;
}

unsafe extern "system" fn button_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    match msg {
        // Dialog navigation promotes focused buttons to default push buttons.
        // Keep our owner-drawn type while preserving its keyboard behavior.
        BM_SETSTYLE => DefSubclassProc(
            hwnd,
            msg,
            (w & !(BS_TYPEMASK as usize)) | BS_OWNERDRAW as usize,
            l,
        ),
        WM_STYLECHANGING if w as i32 == GWL_STYLE => {
            let style = &mut *(l as *mut STYLESTRUCT);
            style.styleNew = (style.styleNew & !(BS_TYPEMASK as u32)) | BS_OWNERDRAW as u32;
            DefSubclassProc(hwnd, msg, w, l)
        }
        WM_ERASEBKGND => 1,
        WM_SETCURSOR if (l as u32 & 65535) == HTCLIENT && IsWindowEnabled(hwnd) != 0 => {
            SetCursor(LoadCursorW(null_mut(), IDC_HAND));
            1
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(button_proc), id);
            DefSubclassProc(hwnd, msg, w, l)
        }
        _ => DefSubclassProc(hwnd, msg, w, l),
    }
}

unsafe fn select_all_edit(hwnd: HWND) -> bool {
    let mut class = [0u16; 16];
    let length = GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
    if length != 4
        || !class[..4]
            .iter()
            .zip("EDIT".encode_utf16())
            .all(|(left, right)| (*left as u8).eq_ignore_ascii_case(&(right as u8)))
    {
        return false;
    }
    SendMessageW(hwnd, EM_SETSEL, 0, -1);
    true
}

unsafe fn edit_shortcut(message: &MSG) -> bool {
    message.message == WM_KEYDOWN
        && message.wParam == b'A' as usize
        && GetKeyState(VK_CONTROL as i32) < 0
        && GetKeyState(VK_MENU as i32) >= 0
        && select_all_edit(message.hwnd)
}

unsafe fn buffered_paint(dc: HDC, bounds: RECT, paint: impl FnOnce(HDC, RECT)) {
    let width = bounds.right - bounds.left;
    let height = bounds.bottom - bounds.top;
    if width <= 0 || height <= 0 {
        return;
    }
    let memory = CreateCompatibleDC(dc);
    let bitmap = if memory.is_null() {
        null_mut()
    } else {
        CreateCompatibleBitmap(dc, width, height)
    };
    if memory.is_null() || bitmap.is_null() {
        if !memory.is_null() {
            DeleteDC(memory);
        }
        paint(dc, bounds);
        return;
    }
    let old = SelectObject(memory, bitmap);
    SetWindowOrgEx(memory, bounds.left, bounds.top, null_mut());
    paint(memory, bounds);
    BitBlt(
        dc,
        bounds.left,
        bounds.top,
        width,
        height,
        memory,
        bounds.left,
        bounds.top,
        SRCCOPY,
    );
    SelectObject(memory, old);
    DeleteObject(bitmap);
    DeleteDC(memory);
}

unsafe extern "system" fn list_proc(
    hwnd: HWND,
    msg: u32,
    w: WPARAM,
    l: LPARAM,
    id: usize,
    _: usize,
) -> LRESULT {
    let parent = GetParent(hwnd);
    let pointer = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut Ui;
    if pointer.is_null() {
        return DefSubclassProc(hwnd, msg, w, l);
    }
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut bounds = zeroed();
            GetClientRect(hwnd, &mut bounds);
            buffered_paint(dc, bounds, |memory, r| (*pointer).paint_list(memory, r));
            EndPaint(hwnd, &ps);
            0
        }
        WM_PRINTCLIENT => {
            let mut bounds = zeroed();
            GetClientRect(hwnd, &mut bounds);
            (*pointer).paint_list(w as HDC, bounds);
            0
        }
        WM_MOUSEMOVE => {
            let ui = &mut *pointer;
            let point = POINT {
                x: (l as u16 as i16) as i32,
                y: ((l >> 16) as u16 as i16) as i32,
            };
            ui.set_row_hover(ui.row_at(point));
            let mut tracking = TRACKMOUSEEVENT {
                cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            TrackMouseEvent(&mut tracking);
            DefSubclassProc(hwnd, msg, w, l)
        }
        WM_MOUSELEAVE => {
            (*pointer).set_row_hover(None);
            DefSubclassProc(hwnd, msg, w, l)
        }
        WM_MOUSEWHEEL | WM_VSCROLL => {
            let result = DefSubclassProc(hwnd, msg, w, l);
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            ScreenToClient(hwnd, &mut point);
            let ui = &mut *pointer;
            ui.set_row_hover(ui.row_at(point));
            InvalidateRect(hwnd, null(), 0);
            result
        }
        WM_SETCURSOR if (l as u32 & 65535) == HTCLIENT => {
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            ScreenToClient(hwnd, &mut point);
            SetCursor(LoadCursorW(
                null_mut(),
                if (*pointer).row_at(point).is_some() {
                    IDC_HAND
                } else {
                    IDC_ARROW
                },
            ));
            1
        }
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(list_proc), id);
            DefSubclassProc(hwnd, msg, w, l)
        }
        _ => DefSubclassProc(hwnd, msg, w, l),
    }
}
impl Ui {
    unsafe fn row_at(&self, point: POINT) -> Option<usize> {
        let packed = (point.x as u16 as u32 | ((point.y as u16 as u32) << 16)) as isize;
        let hit = SendMessageW(self.list, LB_ITEMFROMPOINT, 0, packed) as u32;
        let index = (hit & 65535) as usize;
        if (hit >> 16) != 0 || index >= self.view.rows.len() {
            return None;
        }
        let mut bounds = zeroed();
        if SendMessageW(
            self.list,
            LB_GETITEMRECT,
            index,
            &mut bounds as *mut RECT as isize,
        ) == LB_ERR as isize
            || PtInRect(&bounds, point) == 0
        {
            return None;
        }
        Some(index)
    }

    unsafe fn set_row_hover(&mut self, hover: Option<usize>) {
        if hover == self.hover_row {
            return;
        }
        let previous = self.hover_row;
        self.hover_row = hover;
        for index in [previous, hover].into_iter().flatten() {
            let mut bounds = zeroed();
            if SendMessageW(
                self.list,
                LB_GETITEMRECT,
                index,
                &mut bounds as *mut RECT as isize,
            ) != LB_ERR as isize
            {
                InvalidateRect(self.list, &bounds, 0);
            }
        }
    }

    unsafe fn paint_row(&mut self, dc: HDC, r: RECT, index: usize) {
        self.theme.leather(dc, r);
        if let Some(row) = self.view.rows.get(index) {
            let selected = self.selected.as_ref() == Some(&row.id);
            if selected || self.hover_row == Some(index) {
                self.theme.selected(dc, r);
            }
            self.theme.line(dc, r);
            let mut label = r;
            label.left += if selected { 22 } else { 12 };
            if selected {
                self.theme
                    .text(dc, ">", rect(r.left + 5, r.top, 14, 29), 19, GOLD, false);
            }
            self.theme.text(dc, &row.name, label, 19, TEXT, false);
        } else if self.view.rows.is_empty() {
            self.theme
                .text(dc, "No locations saved", r, 20, MUTED, true);
        }
    }

    unsafe fn paint_list(&mut self, dc: HDC, bounds: RECT) {
        self.theme.leather(dc, bounds);
        let top = SendMessageW(self.list, LB_GETTOPINDEX, 0, 0).max(0) as usize;
        let count = SendMessageW(self.list, LB_GETCOUNT, 0, 0).max(0) as usize;
        for index in top..count {
            let mut item: RECT = zeroed();
            if SendMessageW(
                self.list,
                LB_GETITEMRECT,
                index,
                &mut item as *mut RECT as isize,
            ) == LB_ERR as isize
                || item.top >= bounds.bottom
            {
                break;
            }
            self.paint_row(dc, item, index);
        }
    }
    unsafe fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
    unsafe fn initialize(&mut self) {
        for (id, label, r) in buttons() {
            child(
                self.hwnd,
                "BUTTON",
                label,
                id,
                r,
                BS_OWNERDRAW as u32 | WS_TABSTOP,
            );
        }
        self.name = child(
            self.hwnd,
            "EDIT",
            "",
            NAME,
            rect(36, 237, 392, 31),
            ES_AUTOHSCROLL as u32 | WS_TABSTOP | WS_BORDER,
        );
        self.find = child(
            self.hwnd,
            "EDIT",
            "",
            FIND,
            rect(83, 309, 481, 30),
            ES_AUTOHSCROLL as u32 | WS_TABSTOP | WS_BORDER,
        );
        self.list = child(
            self.hwnd,
            "LISTBOX",
            "",
            LIST,
            rect(36, 350, 528, 174),
            LBS_NOTIFY as u32
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32
                | LBS_NOINTEGRALHEIGHT as u32
                | WS_VSCROLL
                | WS_TABSTOP,
        );
        SendMessageW(self.list, LB_SETITEMHEIGHT, 0, 29);
        for h in [self.name, self.find, self.list] {
            SendMessageW(h, WM_SETFONT, self.theme.font(19) as usize, 1);
        }
        if !UI_TEST.load(Ordering::Relaxed) {
            SetFocus(self.name);
            SetTimer(self.hwnd, 1, 50, None);
            let f5 = RegisterHotKey(self.hwnd, 51, MOD_NOREPEAT, VK_F5 as u32);
            let f9 = RegisterHotKey(self.hwnd, 52, MOD_NOREPEAT, VK_F9 as u32);
            self.hotkey_warning = f5 == 0 || f9 == 0;
        }
    }
    unsafe fn update(&mut self) {
        let mut changed = false;
        while let Ok(Event::Snapshot(view)) = self.events.try_recv() {
            self.view = view;
            changed = true;
        }
        self.sync_rows(changed);
        if changed {
            let b = !self.view.busy;
            for id in [CAP, SAVE] {
                enable(GetDlgItem(self.hwnd, id), b && self.view.connected);
            }
            enable(
                GetDlgItem(self.hwnd, BACK),
                b && self.view.connected && self.view.has_quick,
            );
            enable(
                GetDlgItem(self.hwnd, TRAVEL),
                b && self.view.connected && self.selected.is_some(),
            );
            for id in [EDIT, REMOVE] {
                enable(GetDlgItem(self.hwnd, id), b && self.selected.is_some());
            }
            for id in [IMPORT, EXPORT, MANUAL] {
                enable(GetDlgItem(self.hwnd, id), b);
            }
            enable(GetDlgItem(self.hwnd, PREV), self.view.page > 0);
            enable(
                GetDlgItem(self.hwnd, NEXT),
                self.view.page + 1 < self.view.pages,
            );
            let visual_key = format!(
                "{}|{}|{}|{}|{}|{}",
                self.view.status,
                self.view.quick,
                self.view.total,
                self.view.filtered,
                self.view.page,
                self.view.notice
            );
            if visual_key != self.visual_key {
                self.visual_key = visual_key;
                InvalidateRect(self.hwnd, null(), 0);
            }
        }
        let mut point = POINT { x: 0, y: 0 };
        GetCursorPos(&mut point);
        let found = WindowFromPoint(point);
        let h = if !found.is_null()
            && GetParent(found) == self.hwnd
            && (CAP..=RECONNECT).contains(&GetDlgCtrlID(found))
        {
            found
        } else {
            null_mut()
        };
        if h != self.hover {
            let old = self.hover;
            self.hover = h;
            if !old.is_null() {
                InvalidateRect(old, null(), 0);
            }
            if !h.is_null() {
                InvalidateRect(h, null(), 0);
            }
        }
        let query = text(self.find);
        if query != self.search {
            self.search = query;
            self.search_ticks = 3;
        } else if self.search_ticks > 0 {
            self.search_ticks -= 1;
            if self.search_ticks == 0 {
                self.send(Command::Filter(self.search.clone()));
            }
        }
    }
    unsafe fn sync_rows(&mut self, changed: bool) {
        let count = SendMessageW(self.list, LB_GETCOUNT, 0, 0);
        if !changed && count == self.view.rows.len().max(1) as isize {
            return;
        }
        let key = format!(
            "{}:{}:{}",
            self.view.page,
            self.view.filtered,
            self.view
                .rows
                .iter()
                .map(|x| format!("{}:{}", x.id, x.name))
                .collect::<Vec<_>>()
                .join("\n")
        );
        if key != self.rows_key || count != self.view.rows.len().max(1) as isize {
            self.rows_key = key;
            self.hover_row = None;
            SendMessageW(self.list, WM_SETREDRAW, 0, 0);
            SendMessageW(self.list, LB_RESETCONTENT, 0, 0);
            for row in &self.view.rows {
                SendMessageW(
                    self.list,
                    LB_ADDSTRING,
                    0,
                    wide(&row.name).as_ptr() as isize,
                );
            }
            let index = self
                .selected
                .as_ref()
                .and_then(|id| self.view.rows.iter().position(|x| &x.id == id))
                .or_else(|| (!self.view.rows.is_empty()).then_some(0));
            if self.view.rows.is_empty() {
                SendMessageW(
                    self.list,
                    LB_ADDSTRING,
                    0,
                    wide("No locations saved").as_ptr() as isize,
                );
            }
            if let Some(index) = index {
                self.selected = Some(self.view.rows[index].id.clone());
                SendMessageW(self.list, LB_SETCURSEL, index, 0);
            } else {
                self.selected = None;
            }
            SendMessageW(self.list, WM_SETREDRAW, 1, 0);
            RedrawWindow(self.list, null(), null_mut(), RDW_INVALIDATE | RDW_NOERASE);
        }
    }
    unsafe fn paint(&mut self, dc: HDC) {
        self.theme.background.draw(dc, 0, 0, 600, 700);
        self.theme
            .text(dc, "T E L E P O R T", rect(34, 79, 532, 29), 25, GOLD, true);
        for y in [136, 275, 340, 535, 648] {
            self.theme.divider(dc, y, 532);
        }
        self.theme.text(
            dc,
            &self.view.status,
            rect(34, 109, 532, 25),
            18,
            if self.view.connected {
                rgb(200, 215, 167)
            } else {
                MUTED
            },
            true,
        );
        self.theme.text(
            dc,
            &self.view.quick,
            rect(36, 190, 528, 23),
            16,
            MUTED,
            true,
        );
        self.theme.text(
            dc,
            "Location name (optional)",
            rect(36, 211, 360, 27),
            19,
            TEXT,
            false,
        );
        self.theme.text(
            dc,
            "Saved locations",
            rect(36, 279, 400, 28),
            22,
            TEXT,
            false,
        );
        self.theme.text_right(
            dc,
            &format!("{} / {}", self.view.filtered, self.view.total),
            rect(425, 281, 139, 26),
            16,
            TEXT,
        );
        self.theme
            .text(dc, "Find", rect(36, 308, 47, 31), 18, TEXT, false);
        self.theme.text(
            dc,
            &format!("{} / {}", self.view.page + 1, self.view.pages),
            rect(77, 589, 82, 29),
            16,
            TEXT,
            true,
        );
        self.theme.text(
            dc,
            &self.view.notice,
            rect(36, 620, 528, 25),
            14,
            TEXT,
            false,
        );
        self.theme.text(
            dc,
            &format!("SanguineLoki700  ·  {}", crate::VERSION),
            rect(36, 652, 310, 28),
            12,
            TEXT,
            false,
        );
    }
    unsafe fn command(&mut self, id: i32) {
        match id {
            CAP => self.send(Command::Capture),
            BACK => self.send(Command::Return),
            SAVE => {
                self.send(Command::Save(text(self.name)));
                SetWindowTextW(self.name, wide("").as_ptr());
                SetWindowTextW(self.find, wide("").as_ptr());
            }
            TRAVEL => {
                if let Some(id) = &self.selected {
                    self.send(Command::Travel(id.clone()));
                }
            }
            MANUAL => popup(self, false, false),
            EDIT => {
                if self.selected.is_some() {
                    popup(self, true, false)
                }
            }
            REMOVE => {
                if let Some(row) = self
                    .selected
                    .as_ref()
                    .and_then(|id| self.view.rows.iter().find(|x| &x.id == id))
                {
                    if MessageBoxW(
                        self.hwnd,
                        wide(&format!("Remove “{}”?", row.name)).as_ptr(),
                        wide("Remove location").as_ptr(),
                        MB_YESNO | MB_ICONQUESTION,
                    ) == IDYES
                    {
                        self.send(Command::Remove(row.id.clone()));
                    }
                }
            }
            PREV => self.send(Command::Page(self.view.page.saturating_sub(1))),
            NEXT => self.send(Command::Page(self.view.page + 1)),
            IMPORT => {
                if let Some(p) = file_dialog(self.hwnd, false, false) {
                    self.send(Command::Import(p));
                }
            }
            EXPORT => {
                if let Some(p) = file_dialog(self.hwnd, true, false) {
                    self.send(Command::Export(p));
                }
            }
            DIAG => popup(self, false, true),
            RECONNECT => self.send(Command::Reconnect),
            _ => {}
        }
    }
}
unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        let ui = &mut *(create.lpCreateParams as *mut Ui);
        ui.hwnd = hwnd;
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    let ui = &mut *ptr;
    match msg {
        WM_CREATE => {
            ui.initialize();
            0
        }
        WM_SETCURSOR => {
            if (l as u32 & 65535) == HTCLIENT
                && (101..=113).contains(&GetDlgCtrlID(w as HWND))
                && IsWindowEnabled(w as HWND) != 0
            {
                SetCursor(LoadCursorW(null_mut(), IDC_HAND));
                1
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }
        WM_ERASEBKGND => 1,
        WM_PRINTCLIENT => {
            ui.paint(w as HDC);
            0
        }
        WM_PAINT => {
            let mut ps = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            let memory = CreateCompatibleDC(dc);
            let bitmap = CreateCompatibleBitmap(dc, 600, 700);
            let old = SelectObject(memory, bitmap);
            ui.paint(memory);
            BitBlt(dc, 0, 0, 600, 700, memory, 0, 0, SRCCOPY);
            SelectObject(memory, old);
            DeleteObject(bitmap);
            DeleteDC(memory);
            EndPaint(hwnd, &ps);
            0
        }
        WM_TIMER => {
            ui.update();
            0
        }
        WM_HOTKEY => {
            if w == 51 {
                ui.command(CAP)
            } else if w == 52 {
                ui.command(BACK)
            }
            0
        }
        WM_COMMAND => {
            let id = (w & 65535) as i32;
            let notification = (w >> 16) as u32;
            if id == LIST && notification == LBN_SELCHANGE {
                let i = SendMessageW(ui.list, LB_GETCURSEL, 0, 0);
                ui.selected = ui.view.rows.get(i as usize).map(|x| x.id.clone());
                ui.update();
                for id in [TRAVEL, EDIT, REMOVE] {
                    enable(GetDlgItem(hwnd, id), ui.selected.is_some() && !ui.view.busy);
                }
                InvalidateRect(ui.list, null(), 0);
            } else if id == LIST && notification == LBN_DBLCLK {
                ui.command(TRAVEL)
            } else if notification == BN_CLICKED {
                ui.command(id);
            }
            0
        }
        WM_DRAWITEM => {
            let draw = &*(l as *const DRAWITEMSTRUCT);
            let r = draw.rcItem;
            if draw.CtlID == LIST as u32 {
                buffered_paint(draw.hDC, r, |dc, bounds| {
                    ui.paint_row(dc, bounds, draw.itemID as usize);
                });
            } else {
                let label = text(draw.hwndItem);
                let id = draw.CtlID as i32;
                let size = if [MANUAL, PREV, NEXT, IMPORT, EXPORT, DIAG, RECONNECT].contains(&id) {
                    16
                } else {
                    20
                };
                ui.theme.button(
                    draw.hDC,
                    r,
                    ui.hover == draw.hwndItem || (draw.itemState & ODS_SELECTED) != 0,
                    &label,
                    size,
                    IsWindowEnabled(draw.hwndItem) != 0,
                );
            }
            1
        }
        WM_MEASUREITEM => {
            let measure = &mut *(l as *mut MEASUREITEMSTRUCT);
            measure.itemHeight = 29;
            1
        }
        WM_CTLCOLORLISTBOX => {
            SetTextColor(w as HDC, TEXT);
            SetBkMode(w as HDC, TRANSPARENT as i32);
            ui.theme.list_brush as isize
        }
        WM_CTLCOLORBTN => {
            SetTextColor(w as HDC, TEXT);
            SetBkColor(w as HDC, rgb(10, 10, 9));
            ui.theme.list_brush as isize
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            SetTextColor(w as HDC, TEXT);
            SetBkColor(w as HDC, rgb(34, 35, 30));
            ui.brush as isize
        }
        WM_CLOSE => {
            DestroyWindow(hwnd);
            0
        }
        WM_DESTROY => {
            UnregisterHotKey(hwnd, 51);
            UnregisterHotKey(hwnd, 52);
            KillTimer(hwnd, 1);
            ui.send(Command::Stop);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe fn file_dialog(parent: HWND, save: bool, log: bool) -> Option<std::path::PathBuf> {
    let mut buffer = vec![0u16; 32768];
    if save {
        let s = wide(if log {
            "dark-souls-teleport-support.log"
        } else {
            "dark-souls-locations.json"
        });
        buffer[..s.len()].copy_from_slice(&s);
    }
    let filter = wide(if log {
        "Support log (*.log)\0*.log\0\0"
    } else {
        "Locations (*.json)\0*.json\0\0"
    });
    let folder = wide(&crate::support::folder().to_string_lossy());
    let extension = wide(if log { "log" } else { "json" });
    let mut ofn: OPENFILENAMEW = zeroed();
    ofn.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    ofn.hwndOwner = parent;
    ofn.lpstrFile = buffer.as_mut_ptr();
    ofn.nMaxFile = buffer.len() as u32;
    ofn.lpstrFilter = filter.as_ptr();
    ofn.lpstrInitialDir = folder.as_ptr();
    ofn.lpstrDefExt = extension.as_ptr();
    ofn.Flags = OFN_EXPLORER
        | OFN_NOCHANGEDIR
        | OFN_PATHMUSTEXIST
        | if save {
            OFN_OVERWRITEPROMPT
        } else {
            OFN_FILEMUSTEXIST
        };
    let ok = if save {
        GetSaveFileNameW(&mut ofn)
    } else {
        GetOpenFileNameW(&mut ofn)
    };
    if ok != 0 {
        Some(
            String::from_utf16_lossy(&buffer[..buffer.iter().position(|x| *x == 0).unwrap_or(0)])
                .into(),
        )
    } else {
        None
    }
}
struct Popup {
    owner: HWND,
    theme: Theme,
    commands: Sender<Command>,
    current: Option<Position>,
    diagnostic_text: String,
    initial_name: String,
    brush: HBRUSH,
    hover: HWND,
    hwnd: HWND,
    diagnostics: bool,
    record: Option<Location>,
    point: Position,
    closed: bool,
    inputs: [HWND; 4],
    feedback: String,
}
impl Popup {
    unsafe fn input(&mut self, index: usize, label: &str, value: &str, r: RECT) {
        self.inputs[index] = child(
            self.hwnd,
            "EDIT",
            value,
            301 + index as i32,
            r,
            ES_AUTOHSCROLL as u32 | WS_BORDER | WS_TABSTOP,
        );
        SendMessageW(
            self.inputs[index],
            WM_SETFONT,
            self.theme.font(19) as usize,
            1,
        );
        let _ = label;
    }
    unsafe fn initialize(&mut self) {
        if self.diagnostics {
            let input = child(
                self.hwnd,
                "EDIT",
                &self.diagnostic_text,
                310,
                rect(30, 35, 430, 260),
                ES_READONLY as u32 | ES_MULTILINE as u32 | ES_AUTOVSCROLL as u32,
            );
            SendMessageW(input, WM_SETFONT, self.theme.font(19) as usize, 1);
            let lines = SendMessageW(input, EM_GETLINECOUNT, 0, 0);
            ShowScrollBar(input, SB_VERT, (lines > 12) as i32);
            for (id, label, r) in [
                (320, "Save support log", rect(30, 322, 180, 38)),
                (321, "Reconnect", rect(220, 322, 110, 38)),
                (322, "Close", rect(340, 322, 120, 38)),
            ] {
                child(
                    self.hwnd,
                    "BUTTON",
                    label,
                    id,
                    r,
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                );
            }
        } else {
            let title = self
                .record
                .as_ref()
                .map(|x| x.name.clone())
                .unwrap_or_else(|| self.initial_name.clone());
            self.input(0, "Location name", &title, rect(30, 56, 430, 29));
            for (i, value) in [self.point.x, self.point.y, self.point.z]
                .iter()
                .enumerate()
            {
                self.input(
                    i + 1,
                    "",
                    &value.to_string(),
                    rect(30 + i as i32 * 150, 136, 130, 29),
                );
            }
            for (id, label, r) in [
                (319, "Use current player position", rect(30, 189, 430, 35)),
                (320, "Save", rect(30, 322, 130, 38)),
                (321, "Teleport", rect(180, 322, 130, 38)),
                (322, "Close", rect(330, 322, 130, 38)),
            ] {
                child(
                    self.hwnd,
                    "BUTTON",
                    label,
                    id,
                    r,
                    BS_OWNERDRAW as u32 | WS_TABSTOP,
                );
            }
            if !UI_TEST.load(Ordering::Relaxed) {
                SetFocus(self.inputs[0]);
            }
        }
    }
    unsafe fn paint(&mut self, dc: HDC) {
        if self.diagnostics {
            FillRect(dc, &rect(0, 0, 490, 395), self.brush);
        } else {
            self.theme.modal.draw(dc, 0, 0, 490, 395);
        }
        if !self.diagnostics {
            self.theme.text(
                dc,
                "Location name (optional)",
                rect(30, 27, 430, 24),
                20,
                TEXT,
                false,
            );
            for (i, t) in ["X", "Y", "Z"].iter().enumerate() {
                self.theme.text(
                    dc,
                    t,
                    rect(30 + i as i32 * 150, 105, 130, 25),
                    20,
                    TEXT,
                    false,
                );
            }
        }
        self.theme.text(
            dc,
            &self.feedback,
            if self.diagnostics {
                rect(30, 298, 430, 22)
            } else {
                rect(30, 265, 430, 48)
            },
            17,
            MUTED,
            false,
        );
    }
    unsafe fn position(&self) -> Result<Position, String> {
        let mut p = self.point;
        let mut values = [0f32; 3];
        for (i, value) in values.iter_mut().enumerate() {
            *value = text(self.inputs[i + 1])
                .trim()
                .parse::<f32>()
                .map_err(|_| "Enter valid numbers for X, Y and Z.")?;
        }
        p.x = values[0];
        p.y = values[1];
        p.z = values[2];
        if !p.valid() {
            return Err("Enter valid coordinates.".into());
        }
        Ok(p)
    }
    unsafe fn command(&mut self, id: i32) {
        if id == 322 {
            close_popup(self.hwnd);
            return;
        }
        if self.diagnostics {
            if id == 320 {
                if let Some(path) = file_dialog(self.hwnd, true, true) {
                    let _ = self.commands.send(Command::SupportLog(path));
                    self.feedback = "Support log requested.".into();
                }
            } else if id == 321 {
                let _ = self.commands.send(Command::Reconnect);
                self.feedback = "Reconnecting…".into();
            }
        } else {
            match id {
                319 => {
                    if let Some(point) = self.current {
                        self.point = point;
                        for (i, value) in [point.x, point.y, point.z].iter().enumerate() {
                            SetWindowTextW(self.inputs[i + 1], wide(&value.to_string()).as_ptr());
                        }
                        self.feedback = "Current position loaded.".into();
                    }
                }
                320 | 321 => match self.position() {
                    Err(e) => self.feedback = e,
                    Ok(point) => {
                        if id == 321 {
                            let _ = self.commands.send(Command::TravelPosition(point));
                            self.feedback = "Teleport requested.".into();
                        } else {
                            if let Some(mut row) = self.record.clone() {
                                row.name = text(self.inputs[0]);
                                row.position = point;
                                let _ = self.commands.send(Command::Edit(row));
                            } else {
                                let _ = self
                                    .commands
                                    .send(Command::ManualSave(text(self.inputs[0]), point));
                            }
                            close_popup(self.hwnd);
                            return;
                        }
                    }
                },
                _ => {}
            }
        }
        InvalidateRect(self.hwnd, null(), 0);
    }
}
unsafe fn close_popup(hwnd: HWND) {
    let owner = GetWindow(hwnd, GW_OWNER);
    let reactivate = GetForegroundWindow() == hwnd;
    if !owner.is_null() && IsWindow(owner) != 0 {
        EnableWindow(owner, 1);
        if reactivate && !UI_TEST.load(Ordering::Relaxed) {
            SetActiveWindow(owner);
        }
    }
    DestroyWindow(hwnd);
    if IsWindow(owner) != 0 {
        RedrawWindow(
            owner,
            null(),
            null_mut(),
            RDW_INVALIDATE | RDW_NOERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
        );
    }
    if reactivate && !UI_TEST.load(Ordering::Relaxed) && IsWindow(owner) != 0 {
        if IsIconic(owner) != 0 {
            ShowWindow(owner, SW_RESTORE);
        }
        SetForegroundWindow(owner);
        let focus = GetNextDlgTabItem(owner, null_mut(), 0);
        if !focus.is_null() {
            SetFocus(focus);
        }
    }
}
unsafe extern "system" fn popup_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let c = &*(l as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, c.lpCreateParams as isize);
        (*(c.lpCreateParams as *mut Popup)).hwnd = hwnd;
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Popup;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    let p = &mut *ptr;

    match msg {
        WM_CREATE => {
            p.initialize();
            SetTimer(hwnd, 2, 50, None);
            0
        }
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let mut ps = zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            p.paint(dc);
            EndPaint(hwnd, &ps);
            0
        }
        WM_PRINTCLIENT => {
            p.paint(w as HDC);
            0
        }
        WM_COMMAND => {
            if ((w >> 16) as u32) == BN_CLICKED {
                p.command((w & 65535) as i32);
            }
            0
        }
        WM_DRAWITEM => {
            let d = &*(l as *const DRAWITEMSTRUCT);
            p.theme.button(
                d.hDC,
                d.rcItem,
                p.hover == d.hwndItem || (d.itemState & ODS_SELECTED) != 0,
                &text(d.hwndItem),
                19,
                true,
            );
            1
        }
        WM_CTLCOLORBTN => {
            SetTextColor(w as HDC, TEXT);
            SetBkColor(w as HDC, rgb(10, 10, 9));
            p.theme.list_brush as isize
        }
        WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC => {
            SetTextColor(w as HDC, TEXT);
            SetBkColor(
                w as HDC,
                if p.diagnostics {
                    rgb(14, 14, 12)
                } else {
                    rgb(34, 35, 30)
                },
            );
            p.brush as isize
        }
        WM_CLOSE => {
            close_popup(hwnd);
            0
        }
        WM_DESTROY => {
            p.closed = true;
            KillTimer(hwnd, 2);
            EnableWindow(p.owner, 1);
            0
        }
        WM_NCDESTROY => {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let result = DefWindowProcW(hwnd, msg, w, l);
            DeleteObject(p.brush);
            drop(Box::from_raw(ptr));
            result
        }
        WM_TIMER => {
            let mut point = POINT { x: 0, y: 0 };
            GetCursorPos(&mut point);
            let found = WindowFromPoint(point);
            let hover = if GetParent(found) == hwnd {
                found
            } else {
                null_mut()
            };
            if hover != p.hover {
                let previous = p.hover;
                p.hover = hover;
                for h in [previous, hover] {
                    if !h.is_null() {
                        InvalidateRect(h, null(), 0);
                    }
                }
            }
            0
        }
        WM_SETCURSOR => {
            if (l as u32 & 65535) == HTCLIENT && GetDlgCtrlID(w as HWND) >= 319 {
                SetCursor(LoadCursorW(null_mut(), IDC_HAND));
                1
            } else {
                DefWindowProcW(hwnd, msg, w, l)
            }
        }

        _ => DefWindowProcW(hwnd, msg, w, l),
    }
}
unsafe fn popup(ui: &mut Ui, editing: bool, diagnostics: bool) {
    let _ = open_popup(ui, editing, diagnostics);
}
unsafe fn open_popup(ui: &mut Ui, editing: bool, diagnostics: bool) -> HWND {
    let record = if editing {
        ui.selected
            .as_ref()
            .and_then(|id| ui.view.rows.iter().find(|x| &x.id == id))
            .cloned()
    } else {
        None
    };
    let point = record
        .as_ref()
        .map(|x| x.position)
        .or(ui.view.current)
        .unwrap_or(Position {
            area: 0,
            x: 0.,
            y: 0.,
            z: 0.,
            angle: 0.,
            map: None,
        });
    let p = Box::new(Popup {
        owner: ui.hwnd,
        theme: Theme::new(),
        commands: ui.commands.clone(),
        current: ui.view.current,
        diagnostic_text: format!(
            "{}{}",
            ui.view.diagnostics,
            if ui.hotkey_warning {
                "\r\n\r\nA shortcut is in use by another program. Close the other teleport tool or use the buttons."
            } else {
                ""
            }
        ),
        initial_name: text(ui.name),
        brush: CreateSolidBrush(if diagnostics {
            rgb(14, 14, 12)
        } else {
            rgb(34, 35, 30)
        }),
        hover: null_mut(),
        hwnd: null_mut(),
        diagnostics,
        record,
        point,
        closed: false,
        inputs: [null_mut(); 4],
        feedback: if diagnostics {
            ""
        } else {
            "Changes are saved when you click Save."
        }
        .into(),
    });
    let mut outer = rect(0, 0, 490, 395);
    AdjustWindowRectEx(
        &mut outer,
        WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
        0,
        WS_EX_DLGMODALFRAME,
    );
    let mut parent: RECT = zeroed();
    GetWindowRect(ui.hwnd, &mut parent);
    let hwnd = CreateWindowExW(
        WS_EX_DLGMODALFRAME,
        wide("SoulsTeleportPopup").as_ptr(),
        wide(if diagnostics {
            "Dark Souls Teleport · Diagnostics"
        } else {
            "Dark Souls Teleport · Coordinates"
        })
        .as_ptr(),
        WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN,
        (parent.left + parent.right - outer.right + outer.left) / 2,
        (parent.top + parent.bottom - outer.bottom + outer.top) / 2,
        outer.right - outer.left,
        outer.bottom - outer.top,
        ui.hwnd,
        null_mut(),
        GetModuleHandleW(null()),
        Box::into_raw(p) as _,
    );
    if !hwnd.is_null() && !UI_TEST.load(Ordering::Relaxed) {
        EnableWindow(ui.hwnd, 0);
        ShowWindow(hwnd, SW_SHOW);
    }
    hwnd
}

pub fn run() -> Result<(), String> {
    run_inner(false)
}
pub fn dialog_test() -> Result<(), String> {
    UI_TEST.store(true, Ordering::Relaxed);
    run_inner(true)
}
struct InstanceGuard(Vec<HANDLE>);
impl InstanceGuard {
    unsafe fn new(testing: bool) -> Result<Self, String> {
        let names: &[&str] = if testing {
            &["Local\\DarkSoulsCheckpoints.DialogTest"]
        } else {
            &[
                "Local\\DarkSoulsCheckpoints.SingleInstance",
                "Local\\DarkSoulsCheckpoints.SingleInstance.Rust",
            ]
        };
        let mut guard = Self(vec![]);
        for name in names {
            let handle =
                windows_sys::Win32::System::Threading::CreateMutexW(null(), 0, wide(name).as_ptr());
            if handle.is_null() {
                return Err(format!(
                    "Could not open the tool: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let existing = GetLastError() == ERROR_ALREADY_EXISTS;
            guard.0.push(handle);
            if existing {
                return Err(
                    "Dark Souls Remastered Teleport is already open. Close the other copy first."
                        .into(),
                );
            }
        }
        Ok(guard)
    }
}
impl Drop for InstanceGuard {
    fn drop(&mut self) {
        for handle in &self.0 {
            unsafe {
                CloseHandle(*handle);
            }
        }
    }
}
fn run_inner(testing: bool) -> Result<(), String> {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        let instance = GetModuleHandleW(null());
        let _instance = InstanceGuard::new(testing)?;
        for (name, proc) in [
            (
                "SoulsTeleport",
                Some(window_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
            (
                "SoulsTeleportPopup",
                Some(popup_proc as unsafe extern "system" fn(_, _, _, _) -> _),
            ),
        ] {
            let class_name = wide(name);
            let class = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: proc,
                hInstance: instance,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hIcon: LoadIconW(instance, 1usize as _),
                lpszClassName: class_name.as_ptr(),
                ..zeroed()
            };
            if RegisterClassW(&class) == 0 {
                return Err(format!(
                    "Could not create the interface: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        let (test_events, events) = std::sync::mpsc::channel();
        let (commands, unused_commands) = std::sync::mpsc::channel();
        let (commands, events) = if testing {
            (commands, events)
        } else {
            crate::app::spawn()
        };
        let _keep_test_commands = unused_commands;
        let mut ui = Box::new(Ui {
            hwnd: null_mut(),
            theme: Theme::new(),
            view: View::default(),
            commands,
            events,
            name: null_mut(),
            find: null_mut(),
            list: null_mut(),
            hover: null_mut(),
            hover_row: None,
            selected: None,
            hotkey_warning: false,
            rows_key: String::new(),
            visual_key: String::new(),
            brush: CreateSolidBrush(rgb(34, 35, 30)),
            search: String::new(),
            search_ticks: 0,
        });
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut r = rect(0, 0, 600, 700);
        AdjustWindowRectEx(&mut r, style, 0, 0);
        let width = r.right - r.left;
        let height = r.bottom - r.top;
        let hwnd = CreateWindowExW(
            0,
            wide("SoulsTeleport").as_ptr(),
            wide("Dark Souls Remastered Teleport").as_ptr(),
            style,
            (GetSystemMetrics(SM_CXSCREEN) - width) / 2,
            (GetSystemMetrics(SM_CYSCREEN) - height) / 2,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            ui.as_mut() as *mut _ as _,
        );
        if hwnd.is_null() {
            return Err(format!(
                "Could not open the interface: {}",
                std::io::Error::last_os_error()
            ));
        }
        if testing {
            if InstanceGuard::new(true).is_ok() {
                return Err("The single-instance guard allowed a second copy.".into());
            }
            for (id, _, _) in buttons() {
                check_button_background(hwnd, id)?;
            }
            check_edit_selection(ui.name)?;
            check_edit_selection(ui.find)?;
            let point = Position {
                area: 102000,
                x: 1.,
                y: 2.,
                z: 3.,
                angle: 0.,
                map: Some(0x0a020000),
            };
            for (total, page, visible) in [
                (2, 0, 2),
                (3, 0, 3),
                (100000, 0, 50),
                (100000, 1999, 50),
                (1, 0, 1),
                (0, 0, 0),
            ] {
                let rows = (0..visible)
                    .map(|i| Location {
                        id: format!("row-{}", page * 50 + i),
                        name: format!("Location {:06}", page * 50 + i),
                        position: point,
                        created: String::new(),
                    })
                    .collect();
                let view = View {
                    rows,
                    total,
                    filtered: total,
                    page,
                    pages: total.div_ceil(50).max(1),
                    ..View::default()
                };
                test_events
                    .send(Event::Snapshot(view))
                    .map_err(|e| e.to_string())?;
                ui.update();
                if SendMessageW(ui.list, LB_GETCOUNT, 0, 0) != visible.max(1) as isize {
                    return Err("The saved-location list lost rows after an update.".into());
                }
                if visible > 0 {
                    let mut name = vec![0u16; 128];
                    SendMessageW(ui.list, LB_GETTEXT, visible - 1, name.as_mut_ptr() as isize);
                    let len = name.iter().position(|x| *x == 0).unwrap_or(name.len());
                    if String::from_utf16_lossy(&name[..len])
                        != format!("Location {:06}", page * 50 + visible - 1)
                    {
                        return Err("The saved-location list displayed the wrong page.".into());
                    }
                }
                if visible > 1 {
                    let selected = ui.selected.clone();
                    SendMessageW(ui.list, WM_MOUSEMOVE, 0, 10 | (43 << 16));
                    if ui.hover_row != Some(1) || ui.selected != selected {
                        return Err("Hovering a saved location changed its selection or lost its highlight.".into());
                    }
                    SendMessageW(ui.list, WM_MOUSELEAVE, 0, 0);
                    if ui.hover_row.is_some() || ui.selected != selected {
                        return Err("Leaving the saved-location list did not restore its persistent selection.".into());
                    }
                }
                SendMessageW(ui.list, LB_RESETCONTENT, 0, 0);
                ui.update();
                if SendMessageW(ui.list, LB_GETCOUNT, 0, 0) != visible.max(1) as isize {
                    return Err(
                        "The saved-location list did not recover a cleared native control.".into(),
                    );
                }
            }
            ui.view.diagnostics="Connected to Dark Souls Remastered.\r\n\r\n1 saved location loaded.\r\n\r\nA shortcut is in use by another program. Close the other teleport tool or use the buttons.".into();
            for i in 0..40 {
                crate::support::log(format!("Dialog regression {i}"));
                let diagnostic = i % 2 == 0;
                let dialog = open_popup(&mut ui, false, diagnostic);
                if dialog.is_null() {
                    return Err("Could not create a test dialog.".into());
                }
                let control = GetDlgItem(dialog, if diagnostic { 310 } else { 301 });
                if control.is_null() {
                    return Err("Dialog controls were not created.".into());
                }
                if diagnostic && text(control) != ui.view.diagnostics {
                    return Err("Diagnostics text was lost.".into());
                }
                if diagnostic {
                    check_edit_selection(control)?;
                } else {
                    for id in 301..=304 {
                        check_edit_selection(GetDlgItem(dialog, id))?;
                    }
                }
                for id in if diagnostic {
                    &[320, 321, 322][..]
                } else {
                    &[319, 320, 321, 322][..]
                } {
                    check_button_background(dialog, *id)?;
                }
                if i < 2 {
                    print_dialog(
                        dialog,
                        &crate::support::folder().join(if diagnostic {
                            "diagnostics-preview.png"
                        } else {
                            "coordinates-preview.png"
                        }),
                    )?;
                } else {
                    let dc = CreateCompatibleDC(null_mut());
                    SendMessageW(
                        dialog,
                        WM_PRINT,
                        dc as usize,
                        (PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize,
                    );
                    DeleteDC(dc);
                }
                EnableWindow(hwnd, 0);
                match i % 3 {
                    0 => {
                        SendMessageW(dialog, WM_CLOSE, 0, 0);
                    }
                    1 => {
                        let pointer = GetWindowLongPtrW(dialog, GWLP_USERDATA) as *mut Popup;
                        (*pointer).command(322);
                    }
                    _ => close_popup(dialog),
                }
                if IsWindow(dialog) != 0 || IsWindowEnabled(hwnd) == 0 || IsIconic(hwnd) != 0 {
                    return Err("Closing the dialog did not restore its owner window.".into());
                }
            }
            DestroyWindow(hwnd);
            DeleteObject(ui.brush);
            crate::support::log("PASS: saved-location list updates, 100000-location pagination, cleared-list recovery and 40 diagnostics/coordinates open-close cycles");
            return Ok(());
        }
        ShowWindow(hwnd, SW_SHOW);
        let mut msg = zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            if edit_shortcut(&msg) {
                continue;
            }
            let root = GetAncestor(msg.hwnd, GA_ROOT);
            if root != hwnd && GetWindow(root, GW_OWNER) == hwnd {
                if msg.message == WM_KEYDOWN && msg.wParam == VK_ESCAPE as usize {
                    close_popup(root);
                    continue;
                }
                if msg.message == WM_KEYDOWN && msg.wParam == VK_RETURN as usize {
                    let popup = GetWindowLongPtrW(root, GWLP_USERDATA) as *mut Popup;
                    if !popup.is_null() && !(*popup).diagnostics {
                        (*popup).command(320);
                        continue;
                    }
                }
                if IsDialogMessageW(root, &msg) != 0 {
                    continue;
                }
            }
            if msg.message == WM_KEYDOWN && root == hwnd {
                let key = msg.wParam as u16;
                if key == VK_RETURN {
                    if GetFocus() == ui.name {
                        ui.command(SAVE);
                        continue;
                    } else if GetFocus() == ui.list {
                        ui.command(TRAVEL);
                        continue;
                    }
                }
                if key == VK_NEXT {
                    ui.command(NEXT);
                    continue;
                }
                if key == VK_PRIOR {
                    ui.command(PREV);
                    continue;
                }
            }
            if IsDialogMessageW(hwnd, &msg) == 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        DeleteObject(ui.brush);
        Ok(())
    }
}

unsafe fn check_button_background(parent: HWND, id: i32) -> Result<(), String> {
    let button = GetDlgItem(parent, id);
    for requested in [BS_DEFPUSHBUTTON, BS_PUSHBUTTON] {
        SendMessageW(button, BM_SETSTYLE, requested as usize, 1);
        if GetWindowLongPtrW(button, GWL_STYLE) as u32 & BS_TYPEMASK as u32 != BS_OWNERDRAW as u32 {
            return Err(format!(
                "Dialog navigation replaced the custom style of button {id}."
            ));
        }
    }
    let dc = CreateCompatibleDC(null_mut());
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 300,
            biHeight: -50,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut pixels = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut pixels, null_mut(), 0);
    if bitmap.is_null() || pixels.is_null() {
        DeleteDC(dc);
        return Err("Could not create the button paint test surface.".into());
    }
    let old = SelectObject(dc, bitmap);
    let dark = CreateSolidBrush(rgb(20, 20, 18));
    FillRect(dc, &rect(0, 0, 300, 50), dark);
    DeleteObject(dark);
    let brush = SendMessageW(parent, WM_CTLCOLORBTN, dc as usize, button as isize) as HBRUSH;
    FillRect(dc, &rect(0, 0, 300, 50), brush);
    SendMessageW(button, WM_ERASEBKGND, dc as usize, 0);
    let color = GetPixel(dc, 4, 4);
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    if (color & 255) > 180 && ((color >> 8) & 255) > 180 && ((color >> 16) & 255) > 180 {
        return Err(format!(
            "Button {id} exposed a light system background before painting: #{color:06x}"
        ));
    }
    Ok(())
}

unsafe fn check_edit_selection(control: HWND) -> Result<(), String> {
    let original = text(control);
    if original.is_empty() {
        SetWindowTextW(control, wide("Select all regression").as_ptr());
    }
    let mut start = 0u32;
    let mut end = 0u32;
    let handled = select_all_edit(control);
    SendMessageW(
        control,
        EM_GETSEL,
        &mut start as *mut u32 as usize,
        &mut end as *mut u32 as isize,
    );
    let expected = GetWindowTextLengthW(control) as u32;
    if original.is_empty() {
        SetWindowTextW(control, wide("").as_ptr());
    }
    if !handled || start != 0 || end != expected {
        return Err("An editable or read-only text field did not select all its text.".into());
    }
    Ok(())
}

pub fn render_test(path: &std::path::Path) -> Result<(), String> {
    render_preview(path, false)
}
pub fn render_hover_test(path: &std::path::Path) -> Result<(), String> {
    render_preview(path, true)
}
fn render_preview(path: &std::path::Path, hover: bool) -> Result<(), String> {
    unsafe {
        let (tx, _) = std::sync::mpsc::channel();
        let (_, rx) = std::sync::mpsc::channel();
        let mut ui = Ui {
            hwnd: null_mut(),
            theme: Theme::new(),
            view: View {
                status: "●  Connected and ready".into(),
                connected: true,
                ..View::default()
            },
            commands: tx,
            events: rx,
            name: null_mut(),
            find: null_mut(),
            list: null_mut(),
            hover: null_mut(),
            hover_row: None,
            selected: None,
            hotkey_warning: false,
            rows_key: String::new(),
            visual_key: String::new(),
            brush: null_mut(),
            search: String::new(),
            search_ticks: 0,
        };
        let dc = CreateCompatibleDC(null_mut());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: 600,
                biHeight: -700,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            return Err("Could not create an offscreen preview.".into());
        }
        let old = SelectObject(dc, bitmap);
        ui.paint(dc);
        ui.theme.text(dc, "", rect(0, 0, 100, 25), 17, TEXT, false);
        for (id, label, r) in buttons() {
            let size = if [CAP, BACK, SAVE].contains(&id) {
                18
            } else if [MANUAL, IMPORT, EXPORT, DIAG, RECONNECT].contains(&id) {
                16
            } else {
                20
            };
            ui.theme
                .button(dc, r, hover && id == CAP, label, size, true);
        }
        ui.theme.leather(dc, rect(36, 350, 528, 174));
        ui.theme.text(
            dc,
            "No locations saved",
            rect(36, 410, 528, 30),
            20,
            MUTED,
            true,
        );
        for r in [rect(36, 237, 392, 31), rect(83, 309, 481, 30)] {
            let brush = CreateSolidBrush(rgb(34, 35, 30));
            FillRect(dc, &r, brush);
            DeleteObject(brush);
        }
        GdiFlush();
        let data = std::slice::from_raw_parts(bits as *const u8, 600 * 700 * 4);
        let mut img = image::RgbaImage::new(600, 700);
        for (p, b) in img.pixels_mut().zip(data.chunks_exact(4)) {
            *p = image::Rgba([b[2], b[1], b[0], 255]);
        }
        let result = img.save(path).map_err(|e| e.to_string());
        SelectObject(dc, old);
        DeleteObject(bitmap);
        DeleteDC(dc);
        result
    }
}

unsafe fn print_dialog(hwnd: HWND, path: &std::path::Path) -> Result<(), String> {
    let dc = CreateCompatibleDC(null_mut());
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: 490,
            biHeight: -395,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits = null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
    if bitmap.is_null() || bits.is_null() {
        DeleteDC(dc);
        return Err("Could not create dialog preview.".into());
    }
    let old = SelectObject(dc, bitmap);
    SendMessageW(
        hwnd,
        WM_PRINT,
        dc as usize,
        (PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize,
    );
    GdiFlush();
    let data = std::slice::from_raw_parts(bits as *const u8, 490 * 395 * 4);
    let mut img = image::RgbaImage::new(490, 395);
    for (p, b) in img.pixels_mut().zip(data.chunks_exact(4)) {
        *p = image::Rgba([b[2], b[1], b[0], 255]);
    }
    let result = img.save(path).map_err(|e| e.to_string());
    SelectObject(dc, old);
    DeleteObject(bitmap);
    DeleteDC(dc);
    result
}
