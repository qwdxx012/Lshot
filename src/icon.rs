use image::{DynamicImage, Rgba, RgbaImage};
use windows::core::PCWSTR;
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, DestroyIcon, LoadImageW, HICON, ICONINFO, IMAGE_ICON, LR_LOADFROMFILE,
};

use crate::win;

const BASE_ICON_SIZE: i32 = 16;
const BASE_WINDOW_ICON_SIZE: i32 = 32;
const GLYPH_ALPHA_THRESHOLD: u8 = 24;
const TILE_COLOR: [u8; 3] = [0x17, 0x18, 0x1B];
const TRAY_PNG: &[u8] = include_bytes!("../assets/tray.png");
const WINDOW_PNG: &[u8] = include_bytes!("../assets/installer.png");

pub fn load_application_icon() -> HICON {
    load_icon(BASE_ICON_SIZE, "logo.png", TRAY_PNG, true)
}

pub fn load_window_icon() -> HICON {
    load_icon(BASE_WINDOW_ICON_SIZE, "installer.png", WINDOW_PNG, false)
}

fn load_icon(base_size: i32, png_name: &str, embedded: &[u8], tile: bool) -> HICON {
    let target = icon_size(base_size);
    let base = crate::settings::exe_directory();
    let png = base.join(png_name);
    if png.exists() {
        if let Ok(image) = image::open(&png) {
            if let Some(icon) = icon_from_image(&image, target, tile) {
                return icon;
            }
        }
    }

    if let Ok(image) = image::load_from_memory(embedded) {
        if let Some(icon) = icon_from_image(&image, target, tile) {
            return icon;
        }
    }

    let ico = base.join("logo.ico");
    if ico.exists() {
        if let Some(icon) = load_icon_file(&ico, target) {
            return icon;
        }
    }

    if let Some(icon) = icon_from_image(&fallback_image(), target, tile) {
        return icon;
    }

    HICON::default()
}

fn icon_size(base_size: i32) -> i32 {
    let scale = win::system_dpi() as f32 / 96.0;
    ((base_size as f32 * scale).round() as i32).max(base_size)
}

fn load_icon_file(path: &std::path::Path, size: i32) -> Option<HICON> {
    let wide_path = win::wide(&path.to_string_lossy());
    unsafe {
        let handle = LoadImageW(
            None,
            PCWSTR(wide_path.as_ptr()),
            IMAGE_ICON,
            size,
            size,
            LR_LOADFROMFILE,
        )
        .ok()?;
        if handle.is_invalid() {
            None
        } else {
            Some(HICON(handle.0))
        }
    }
}

fn icon_from_image(source: &DynamicImage, target: i32, tile: bool) -> Option<HICON> {
    let glyph = glyph_with_alpha(source)?;
    let (left, top, width, height) = glyph_bounds(&glyph)?;
    let glyph = image::imageops::crop_imm(&glyph, left, top, width, height).to_image();
    let canvas = compose_icon(&glyph, target, tile);
    create_hicon(&canvas)
}

// Artwork may come either with a real alpha channel or, like the legacy logo, as
// white artwork on an opaque black field; the latter needs a luminance mask.
fn glyph_with_alpha(source: &DynamicImage) -> Option<RgbaImage> {
    let image = source.to_rgba8();
    let has_alpha = image.pixels().any(|pixel| pixel.0[3] < 250);
    let mut glyph = RgbaImage::new(image.width(), image.height());
    for (x, y, pixel) in image.enumerate_pixels() {
        let data = pixel.0;
        let luminance = data[0].max(data[1]).max(data[2]);
        let alpha = if has_alpha { data[3] } else { luminance };
        let alpha = if alpha < GLYPH_ALPHA_THRESHOLD { 0 } else { alpha };
        glyph.put_pixel(x, y, Rgba([data[0], data[1], data[2], alpha]));
    }
    Some(glyph)
}

fn glyph_bounds(source: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut min_x = source.width();
    let mut min_y = source.height();
    let mut max_x: i64 = -1;
    let mut max_y: i64 = -1;
    for (x, y, pixel) in source.enumerate_pixels() {
        if pixel.0[3] > GLYPH_ALPHA_THRESHOLD {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x as i64);
            max_y = max_y.max(y as i64);
        }
    }
    if max_x < 0 {
        return None;
    }
    Some((
        min_x,
        min_y,
        (max_x - min_x as i64 + 1) as u32,
        (max_y - min_y as i64 + 1) as u32,
    ))
}

fn compose_icon(glyph: &RgbaImage, target: i32, tile: bool) -> RgbaImage {
    let size = target.max(8) as u32;
    let fill = if tile { 0.86 } else { 0.94 };
    let inner = ((size as f32) * fill).round().max(4.0) as u32;
    let scale = (inner as f32 / glyph.width().max(1) as f32)
        .min(inner as f32 / glyph.height().max(1) as f32);
    let glyph_width = ((glyph.width() as f32 * scale).round() as u32).max(1);
    let glyph_height = ((glyph.height() as f32 * scale).round() as u32).max(1);
    let resized = image::imageops::resize(
        glyph,
        glyph_width,
        glyph_height,
        image::imageops::FilterType::Lanczos3,
    );

    let mut canvas = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 0]));
    if tile {
        let radius = (size as f32 * 0.22).round().max(2.0);
        for y in 0..size {
            for x in 0..size {
                let alpha = rounded_rect_alpha(x, y, size, radius);
                if alpha > 0.0 {
                    canvas.put_pixel(
                        x,
                        y,
                        Rgba([
                            TILE_COLOR[0],
                            TILE_COLOR[1],
                            TILE_COLOR[2],
                            (alpha * 255.0).round() as u8,
                        ]),
                    );
                }
            }
        }
    }

    let offset_x = ((size - glyph_width) / 2) as i64;
    let offset_y = ((size - glyph_height) / 2) as i64;
    for (x, y, pixel) in resized.enumerate_pixels() {
        let alpha = pixel.0[3] as f32 / 255.0;
        if alpha <= 0.0 {
            continue;
        }
        let px = x as i64 + offset_x;
        let py = y as i64 + offset_y;
        if px < 0 || py < 0 || px >= size as i64 || py >= size as i64 {
            continue;
        }
        let base = canvas.get_pixel(px as u32, py as u32).0;
        let base_alpha = base[3] as f32 / 255.0;
        let out_alpha = alpha + base_alpha * (1.0 - alpha);
        let blend = |foreground: u8, background: u8| -> u8 {
            let value = (foreground as f32 * alpha
                + background as f32 * base_alpha * (1.0 - alpha))
                / out_alpha.max(f32::EPSILON);
            value.clamp(0.0, 255.0).round() as u8
        };
        canvas.put_pixel(
            px as u32,
            py as u32,
            Rgba([
                blend(pixel.0[0], base[0]),
                blend(pixel.0[1], base[1]),
                blend(pixel.0[2], base[2]),
                (out_alpha * 255.0).round() as u8,
            ]),
        );
    }
    canvas
}

fn rounded_rect_alpha(x: u32, y: u32, size: u32, radius: f32) -> f32 {
    let half = size as f32 / 2.0;
    let center_x = x as f32 + 0.5 - half;
    let center_y = y as f32 + 0.5 - half;
    let qx = center_x.abs() - (half - radius);
    let qy = center_y.abs() - (half - radius);
    let outside = (qx.max(0.0) * qx.max(0.0) + qy.max(0.0) * qy.max(0.0)).sqrt();
    let distance = outside + qx.max(qy).min(0.0) - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn create_hicon(image: &RgbaImage) -> Option<HICON> {
    let width = image.width() as i32;
    let height = image.height() as i32;
    if width <= 0 || height <= 0 {
        return None;
    }

    unsafe {
        let screen = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND::default());
        let memory = windows::Win32::Graphics::Gdi::CreateCompatibleDC(screen);
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
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let color_bitmap = match CreateDIBSection(memory, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(value) => value,
            Err(_) => {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(memory);
                windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND::default(), screen);
                return None;
            }
        };
        if color_bitmap.is_invalid() || bits.is_null() {
            let _ = windows::Win32::Graphics::Gdi::DeleteDC(memory);
            windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND::default(), screen);
            return None;
        }

        let destination = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
        for (index, pixel) in image.pixels().enumerate() {
            let offset = index * 4;
            destination[offset] = pixel.0[2];
            destination[offset + 1] = pixel.0[1];
            destination[offset + 2] = pixel.0[0];
            destination[offset + 3] = pixel.0[3];
        }

        let mask = CreateBitmap(width, height, 1, 1, None);
        let icon_info = ICONINFO {
            fIcon: true.into(),
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: mask,
            hbmColor: color_bitmap,
        };
        let icon = CreateIconIndirect(&icon_info).ok();

        let _ = DeleteObject(color_bitmap);
        let _ = DeleteObject(mask);
        let _ = windows::Win32::Graphics::Gdi::DeleteDC(memory);
        windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND::default(), screen);

        icon.filter(|handle| !handle.is_invalid())
    }
}

pub fn destroy(icon: HICON) {
    if !icon.is_invalid() {
        unsafe {
            let _ = DestroyIcon(icon);
        }
    }
}

fn fallback_image() -> DynamicImage {
    const SCALE: u32 = 8;
    const GRID: u32 = 32;
    let mut image = RgbaImage::from_pixel(GRID * SCALE, GRID * SCALE, Rgba([0, 0, 0, 255]));

    let mut fill = |x: u32, y: u32, width: u32, height: u32| {
        for py in y * SCALE..(y + height) * SCALE {
            for px in x * SCALE..(x + width) * SCALE {
                image.put_pixel(px, py, Rgba([255, 255, 255, 255]));
            }
        }
    };

    let mut x = 1;
    while x <= 27 {
        fill(x, 1, 2, 2);
        fill(x, 29, 2, 2);
        x += 4;
    }
    let mut y = 1;
    while y <= 27 {
        fill(1, y, 2, 2);
        fill(29, y, 2, 2);
        y += 4;
    }

    fill(10, 8, 4, 13);
    fill(10, 17, 10, 4);

    fill(18, 17, 2, 2);
    fill(20, 19, 2, 2);
    fill(22, 21, 2, 2);
    fill(26, 17, 2, 2);
    fill(24, 19, 2, 2);
    fill(22, 21, 2, 2);
    fill(18, 22, 4, 4);
    fill(19, 23, 2, 2);
    fill(25, 22, 4, 4);
    fill(26, 23, 2, 2);

    DynamicImage::ImageRgba8(image)
}