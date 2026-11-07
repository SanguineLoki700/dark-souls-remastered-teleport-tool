use image::{
    imageops::{self, FilterType},
    RgbaImage,
};
use std::{collections::HashMap, ptr::null_mut};
use windows_sys::Win32::{Foundation::RECT, Graphics::Gdi::*};
pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    r as u32 | ((g as u32) << 8) | ((b as u32) << 16)
}
pub const TEXT: u32 = 0x00d6e4e8;
pub const MUTED: u32 = 0x0099adb3;
pub const GOLD: u32 = 0x0073b7dc;
pub struct Bitmap {
    pub width: i32,
    pub height: i32,
    pixels: Vec<u8>,
}
impl Bitmap {
    pub fn new(img: RgbaImage) -> Self {
        let (width, height) = img.dimensions();
        let pixels = img
            .pixels()
            .flat_map(|p| {
                let a = p[3] as u16;
                [
                    (p[2] as u16 * a / 255) as u8,
                    (p[1] as u16 * a / 255) as u8,
                    (p[0] as u16 * a / 255) as u8,
                    255,
                ]
            })
            .collect();
        Self {
            width: width as i32,
            height: height as i32,
            pixels,
        }
    }
    pub unsafe fn draw(&self, dc: HDC, x: i32, y: i32, width: i32, height: i32) {
        self.crop(dc, (x, y, width, height), (0, 0, self.width, self.height))
    }
    pub unsafe fn crop(&self, dc: HDC, d: (i32, i32, i32, i32), s: (i32, i32, i32, i32)) {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width,
                biHeight: -self.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let old_stretch=SetStretchBltMode(dc,HALFTONE as i32);
        StretchDIBits(
            dc,
            d.0,
            d.1,
            d.2,
            d.3,
            s.0,
            s.1,
            s.2,
            s.3,
            self.pixels.as_ptr() as _,
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        if old_stretch!=0{SetStretchBltMode(dc,old_stretch);}
    }
}
fn load(data: &[u8]) -> RgbaImage {
    image::load_from_memory(data)
        .expect("Embedded artwork")
        .to_rgba8()
}
fn overlay(
    target: &mut RgbaImage,
    source: &RgbaImage,
    d: (u32, u32, u32, u32),
    s: (u32, u32, u32, u32),
    key: bool,
) {
    let mut crop = imageops::crop_imm(source, s.0, s.1, s.2, s.3).to_image();
    if key {
        for p in crop.pixels_mut() {
            if p[0] <= 20 && p[1] <= 20 && p[2] <= 20 {
                p[3] = 0
            }
        }
    }
    let resized = imageops::resize(&crop, d.2, d.3, FilterType::Lanczos3);
    imageops::overlay(target, &resized, d.0 as i64, d.1 as i64);
}

struct GameFrame {
    tl: Bitmap,
    tr: Bitmap,
    bl: Bitmap,
    br: Bitmap,
    left: Bitmap,
    right: Bitmap,
    top: Bitmap,
    bottom: Bitmap,
}
impl GameFrame {
    fn new() -> Self {
        Self {
            tl: Bitmap::new(load(include_bytes!("../assets/frame-tl.png"))),
            tr: Bitmap::new(load(include_bytes!("../assets/frame-tr.png"))),
            bl: Bitmap::new(load(include_bytes!("../assets/frame-bl.png"))),
            br: Bitmap::new(load(include_bytes!("../assets/frame-br.png"))),
            left: Bitmap::new(load(include_bytes!("../assets/frame-left.png"))),
            right: Bitmap::new(load(include_bytes!("../assets/frame-right.png"))),
            top: Bitmap::new(load(include_bytes!("../assets/frame-top.png"))),
            bottom: Bitmap::new(load(include_bytes!("../assets/frame-bottom.png"))),
        }
    }
    unsafe fn draw(&self, dc: HDC, r: RECT) {
        let w = r.right - r.left;
        let h = r.bottom - r.top;
        let ch = 16.min((h / 2 - 1).max(1));
        let cw = (ch * 82 / 96).min(w / 2).max(1);
        let line = 3.min(ch);
        self.top.draw(dc, r.left + cw, r.top, w - 2 * cw, line);
        self.bottom
            .draw(dc, r.left + cw, r.bottom - line, w - 2 * cw, line);
        self.left.draw(dc, r.left, r.top + ch, 2, h - 2 * ch);
        self.right.draw(dc, r.right - 2, r.top + ch, 2, h - 2 * ch);
        self.tl.draw(dc, r.left, r.top, cw, ch);
        self.tr.draw(dc, r.right - cw, r.top, cw, ch);
        self.bl.draw(dc, r.left, r.bottom - ch, cw, ch);
        self.br.draw(dc, r.right - cw, r.bottom - ch, cw, ch);
    }
}
pub struct Theme {
    pub background: Bitmap,
    pub modal: Bitmap,
    frame: GameFrame,
    bottom: Bitmap,
    divider: Bitmap,
    selection: Bitmap,
    tile: Bitmap,
    pub list_brush: HBRUSH,
    fonts: HashMap<i32, HFONT>,
    registration: *mut core::ffi::c_void,
}
impl Theme {
    pub unsafe fn new() -> Self {
        let cloth = load(include_bytes!("../assets/cloth-frame.png"));
        let logo = load(include_bytes!("../assets/logo.png"));
        let inventory = load(include_bytes!("../assets/inventory.jpg"));
        let selection = load(include_bytes!("../assets/system-menu.png"));
        let mut background = RgbaImage::new(600, 700);
        imageops::overlay(&mut background, &cloth, 0, 0);
        overlay(
            &mut background,
            &logo,
            (145, 24, 310, 43),
            (88, 148, 598, 83),
            true,
        );
        overlay(
            &mut background,
            &logo,
            (222, 68, 155, 14),
            (228, 241, 312, 30),
            true,
        );
        let mut modal = RgbaImage::new(490, 395);
        let sx = [0, 48, 552, 600];
        let sy = [0, 48, 652, 700];
        let dx = [0, 48, 442, 490];
        let dy = [0, 48, 347, 395];
        for row in 0..3 {
            for col in 0..3 {
                overlay(
                    &mut modal,
                    &cloth,
                    (
                        dx[col],
                        dy[row],
                        dx[col + 1] - dx[col],
                        dy[row + 1] - dy[row],
                    ),
                    (
                        sx[col],
                        sy[row],
                        sx[col + 1] - sx[col],
                        sy[row + 1] - sy[row],
                    ),
                    false,
                );
            }
        }
        let tile = imageops::crop_imm(&cloth, 80, 90, 256, 192).to_image();
        let selected = imageops::crop_imm(&selection, 620, 197, 125, 31).to_image();
        let font = include_bytes!("../assets/EBGaramond-Medium.ttf");
        let mut count = 0;
        let registration = AddFontMemResourceEx(
            font.as_ptr() as _,
            font.len() as u32,
            null_mut(),
            &mut count,
        );
        let bottom = Bitmap::new(imageops::resize(
            &imageops::crop_imm(&inventory, 310, 489, 900, 6).to_image(),
            900,
            3,
            FilterType::Lanczos3,
        ));
        let mut divider = RgbaImage::new(532, 7);
        overlay(
            &mut divider,
            &inventory,
            (0, 0, 248, 7),
            (166, 489, 514, 21),
            false,
        );
        overlay(
            &mut divider,
            &inventory,
            (248, 0, 35, 7),
            (680, 489, 70, 21),
            false,
        );
        overlay(
            &mut divider,
            &inventory,
            (283, 0, 249, 7),
            (750, 489, 480, 21),
            false,
        );
        let tile = Bitmap::new(tile);
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: tile.width,
                biHeight: -tile.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = null_mut();
        let bitmap = CreateDIBSection(null_mut(), &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if !bits.is_null() {
            std::ptr::copy_nonoverlapping(tile.pixels.as_ptr(), bits as *mut u8, tile.pixels.len());
        }
        let list_brush = CreatePatternBrush(bitmap);
        DeleteObject(bitmap);
        Self {
            background: Bitmap::new(background),
            modal: Bitmap::new(modal),
            frame: GameFrame::new(),
            bottom,
            divider: Bitmap::new(divider),
            list_brush,
            selection: Bitmap::new(selected),
            tile,
            fonts: HashMap::new(),
            registration,
        }
    }
    pub unsafe fn font(&mut self, height: i32) -> HFONT {
        *self.fonts.entry(height).or_insert_with(|| {
            CreateFontW(
                -height,
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                crate::game::wide("EB Garamond Medium").as_ptr(),
            )
        })
    }
    pub unsafe fn text(
        &mut self,
        dc: HDC,
        text: &str,
        r: RECT,
        size: i32,
        color: u32,
        center: bool,
    ) {
        self.text_aligned(
            dc,
            text,
            r,
            size,
            color,
            if center { DT_CENTER } else { DT_LEFT },
        );
    }
    pub unsafe fn text_right(&mut self, dc: HDC, text: &str, r: RECT, size: i32, color: u32) {
        self.text_aligned(dc, text, r, size, color, DT_RIGHT);
    }
    unsafe fn text_aligned(
        &mut self,
        dc: HDC,
        text: &str,
        r: RECT,
        size: i32,
        color: u32,
        align: u32,
    ) {
        if text.is_empty() || r.right <= r.left || r.bottom <= r.top || dc.is_null() {
            return;
        }
        let old = SelectObject(dc, self.font(size));
        SetBkMode(dc, TRANSPARENT as i32);
        SetTextColor(dc, color);
        let mut rect = r;
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        let length = buffer.len() as i32;
        buffer.push(0);
        DrawTextW(
            dc,
            buffer.as_mut_ptr(),
            length,
            &mut rect,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX | align,
        );
        SelectObject(dc, old);
    }
    pub unsafe fn leather(&self, dc: HDC, r: RECT) {
        let mut y = r.top;
        while y < r.bottom {
            let height = (r.bottom - y).min(192);
            let mut x = r.left;
            while x < r.right {
                let width = (r.right - x).min(256);
                self.tile
                    .crop(dc, (x, y, width, height), (0, 0, width, height));
                x += width;
            }
            y += height;
        }
    }
    pub unsafe fn selected(&self, dc: HDC, r: RECT) {
        self.selection
            .draw(dc, r.left, r.top, r.right - r.left, r.bottom - r.top);
    }
    pub unsafe fn button(
        &mut self,
        dc: HDC,
        r: RECT,
        active: bool,
        text: &str,
        size: i32,
        enabled: bool,
    ) {
        self.leather(dc, r);
        if active && enabled {
            self.selected(dc, r);
        }
        self.frame.draw(dc, r);
        self.text(dc, text, r, size, if enabled { TEXT } else { MUTED }, true);
    }
    pub unsafe fn divider(&self, dc: HDC, y: i32, width: i32) {
        self.divider.draw(dc, 34, y, width, 7);
    }
    pub unsafe fn line(&self, dc: HDC, r: RECT) {
        self.bottom
            .draw(dc, r.left, r.bottom - 3, r.right - r.left, 3);
    }
}
impl Drop for Theme {
    fn drop(&mut self) {
        unsafe {
            for f in self.fonts.values() {
                DeleteObject(*f);
            }
            DeleteObject(self.list_brush);
            if !self.registration.is_null() {
                RemoveFontMemResourceEx(self.registration);
            }
        }
    }
}
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}
