use std::ffi::c_void;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
};

#[derive(Clone)]
pub struct Captured {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

pub fn capture(rect: RECT) -> Result<Captured, String> {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width <= 0 || height <= 0 {
        return Err("The capture region is empty.".to_string());
    }

    unsafe {
        let screen = windows::Win32::Graphics::Gdi::GetDC(None);
        if screen.is_invalid() {
            return Err("The screen device context could not be opened.".to_string());
        }
        let memory = CreateCompatibleDC(screen);
        if memory.is_invalid() {
            windows::Win32::Graphics::Gdi::ReleaseDC(None, screen);
            return Err("The capture surface could not be created.".to_string());
        }

        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: (width * height * 4) as u32,
            ..Default::default()
        };

        let mut bits: *mut c_void = std::ptr::null_mut();
        let bitmap = match CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(value) => value,
            Err(_) => {
                let _ = DeleteDC(memory);
                windows::Win32::Graphics::Gdi::ReleaseDC(None, screen);
                return Err("The capture bitmap could not be created.".to_string());
            }
        };
        if bitmap.is_invalid() || bits.is_null() {
            let _ = DeleteDC(memory);
            windows::Win32::Graphics::Gdi::ReleaseDC(None, screen);
            return Err("The capture bitmap could not be created.".to_string());
        }

        let old: HGDIOBJ = SelectObject(memory, bitmap);
        let copied = BitBlt(
            memory,
            0,
            0,
            width,
            height,
            screen,
            rect.left,
            rect.top,
            SRCCOPY,
        );

        let byte_count = (width * height * 4) as usize;
        let result = if copied.is_ok() {
            let slice = std::slice::from_raw_parts(bits as *const u8, byte_count);
            Ok(Captured {
                width,
                height,
                bgra: slice.to_vec(),
            })
        } else {
            Err("The screen could not be captured.".to_string())
        };

        let _ = SelectObject(memory, old);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(memory);
        windows::Win32::Graphics::Gdi::ReleaseDC(None, screen);
        result
    }
}
