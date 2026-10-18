//! Text rasteriser built on GDI: draws a string white-on-black into a DIB and hands back an 8-bit coverage mask.
//! The bundled Droid Sans Mono is registered privately for this process, so nothing is installed.

use std::collections::HashMap;
use windows_sys::Win32::Foundation::SIZE;
use windows_sys::Win32::Graphics::Gdi::*;

static FONT_BYTES: &[u8] = include_bytes!("../assets/DroidSansMono.ttf");
pub const BUNDLED_FACE: &str = "Droid Sans Mono";

pub struct Raster {
    /// Mask including `pad` pixels of margin on every side (room for the shadow).
    pub w: i32,
    pub h: i32,
    pub pad: i32,
    pub data: Vec<u8>,
}

impl Raster {
    pub fn content_w(&self) -> i32 {
        self.w - 2 * self.pad
    }
    pub fn content_h(&self) -> i32 {
        self.h - 2 * self.pad
    }
}

pub struct Rasterizer {
    dc: HDC,
    face: String,
    fonts: HashMap<i32, HFONT>,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

impl Rasterizer {
    pub fn new(face: &str) -> Rasterizer {
        unsafe {
            static REGISTER: std::sync::Once = std::sync::Once::new();
            REGISTER.call_once(|| {
                let mut n = 0u32;
                AddFontMemResourceEx(FONT_BYTES.as_ptr() as *const _, FONT_BYTES.len() as u32, std::ptr::null(), &mut n);
            });
            let face = if face.is_empty() { BUNDLED_FACE } else { face };
            Rasterizer { dc: CreateCompatibleDC(std::ptr::null_mut()), face: face.to_string(), fonts: HashMap::new() }
        }
    }

    fn font(&mut self, px: i32) -> HFONT {
        let face = wide(&self.face);
        *self.fonts.entry(px).or_insert_with(|| unsafe {
            CreateFontW(-px, 0, 0, 0, FW_NORMAL as i32, 0, 0, 0, DEFAULT_CHARSET as u32, OUT_TT_PRECIS as u32, CLIP_DEFAULT_PRECIS as u32, ANTIALIASED_QUALITY as u32, (FIXED_PITCH | FF_MODERN) as u32, face.as_ptr())
        })
    }

    pub fn raster(&mut self, text: &str, px: i32, pad: i32) -> Raster {
        let px = px.max(4);
        let font = self.font(px);
        let t = wide(text);
        let len = t.len() as i32 - 1;
        unsafe {
            let old = SelectObject(self.dc, font as _);
            let mut sz = SIZE { cx: 0, cy: 0 };
            GetTextExtentPoint32W(self.dc, t.as_ptr(), len, &mut sz);
            let (w, h) = (sz.cx.max(1) + 2 * pad, sz.cy.max(1) + 2 * pad);
            let mut bi: BITMAPINFO = std::mem::zeroed();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h; // top-down
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB;
            let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(self.dc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
            let oldbmp = SelectObject(self.dc, bmp as _);
            SetBkMode(self.dc, OPAQUE as i32);
            SetBkColor(self.dc, 0);
            SetTextColor(self.dc, 0x00FF_FFFF);
            TextOutW(self.dc, pad, pad, t.as_ptr(), len);
            GdiFlush();
            let px32 = std::slice::from_raw_parts(bits as *const u32, (w * h) as usize);
            let data = px32.iter().map(|p| (p & 0xFF) as u8).collect();
            SelectObject(self.dc, oldbmp);
            SelectObject(self.dc, old);
            DeleteObject(bmp as _);
            Raster { w, h, pad, data }
        }
    }
}
