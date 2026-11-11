use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::{DynamicImage, ExtendedColorType, ImageEncoder, RgbaImage};
use std::io::Cursor;
use std::path::Path;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard, RegisterClipboardFormatW,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};

use crate::i18n;

const MAX_GIF_FRAMES: usize = 400;
const MAX_GIF_PIXELS: i64 = 45_000_000;
const PREVIEW_MAX_SIDE: i32 = 1800;

pub struct PreviewImage {
    pub frames: Vec<Vec<u8>>,
    pub delays: Vec<i32>,
    pub width: i32,
    pub height: i32,
    pub render_width: i32,
    pub render_height: i32,
    pub file_bytes: u64,
    pub format: String,
}

pub fn load_preview(path: &Path) -> Option<PreviewImage> {
    let bytes = std::fs::read(path).ok()?;
    let file_bytes = bytes.len() as u64;
    let format = format_name(path);
    let is_gif = path
        .extension()
        .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case("gif"))
        .unwrap_or(false);

    if is_gif {
        if let Some(image) = decode_gif(&bytes, file_bytes, &format) {
            return Some(image);
        }
    }

    let mut dynamic = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;

    let orientation = jpeg_orientation(&bytes);
    if orientation != 1 {
        dynamic = match orientation {
            3 => DynamicImage::ImageRgba8(image::imageops::rotate180(&dynamic.to_rgba8())),
            6 => DynamicImage::ImageRgba8(image::imageops::rotate90(&dynamic.to_rgba8())),
            8 => DynamicImage::ImageRgba8(image::imageops::rotate270(&dynamic.to_rgba8())),
            _ => dynamic,
        };
    }

    let mut rgba = dynamic.to_rgba8();
    let (width, height) = (rgba.width() as i32, rgba.height() as i32);
    if width > PREVIEW_MAX_SIDE || height > PREVIEW_MAX_SIDE {
        let scale = PREVIEW_MAX_SIDE as f32 / width.max(height) as f32;
        let new_width = ((width as f32 * scale).round() as u32).max(1);
        let new_height = ((height as f32 * scale).round() as u32).max(1);
        rgba = image::imageops::resize(
            &rgba,
            new_width,
            new_height,
            image::imageops::FilterType::Lanczos3,
        );
    }

    let render_width = rgba.width() as i32;
    let render_height = rgba.height() as i32;
    Some(PreviewImage {
        frames: vec![rgba_to_bgra(&rgba)],
        delays: vec![100],
        width,
        height,
        render_width,
        render_height,
        file_bytes,
        format,
    })
}

fn decode_gif(bytes: &[u8], file_bytes: u64, format: &str) -> Option<PreviewImage> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(Cursor::new(bytes)).ok()?;
    let logical_width = decoder.width() as i32;
    let logical_height = decoder.height() as i32;

    let mut frames: Vec<gif::Frame> = Vec::new();
    while let Ok(Some(frame)) = decoder.read_next_frame() {
        frames.push(frame.clone());
        if frames.len() > MAX_GIF_FRAMES {
            return None;
        }
    }
    if frames.is_empty() {
        return None;
    }
    if frames.len() == 1 {
        return None;
    }

    let mut max_right = logical_width.max(1);
    let mut max_bottom = logical_height.max(1);
    for frame in &frames {
        max_right = max_right.max(frame.left as i32 + frame.width as i32);
        max_bottom = max_bottom.max(frame.top as i32 + frame.height as i32);
    }
    let width = max_right.max(1);
    let height = max_bottom.max(1);
    if (width as i64) * (height as i64) * (frames.len() as i64) > MAX_GIF_PIXELS {
        return None;
    }

    let stride = (width * 4) as usize;
    let mut canvas = vec![0u8; stride * height as usize];
    let mut composed: Vec<Vec<u8>> = Vec::with_capacity(frames.len());
    let mut delays: Vec<i32> = Vec::with_capacity(frames.len());

    let mut previous_disposal = gif::DisposalMethod::Keep;
    let mut previous_left = 0i32;
    let mut previous_top = 0i32;
    let mut previous_width = 0i32;
    let mut previous_height = 0i32;
    let mut has_previous = false;

    let mut restore: Option<Vec<u8>> = None;
    let mut restore_left = 0i32;
    let mut restore_top = 0i32;
    let mut restore_width = 0i32;
    let mut restore_height = 0i32;

    for frame in &frames {
        if has_previous {
            match previous_disposal {
                gif::DisposalMethod::Background => clear_rect(
                    &mut canvas,
                    stride,
                    width,
                    height,
                    previous_left,
                    previous_top,
                    previous_width,
                    previous_height,
                ),
                gif::DisposalMethod::Previous => {
                    if let Some(snapshot) = &restore {
                        restore_rect(
                            &mut canvas,
                            stride,
                            width,
                            height,
                            snapshot,
                            restore_left,
                            restore_top,
                            restore_width,
                            restore_height,
                        );
                    }
                }
                _ => {}
            }
        }

        let left = frame.left as i32;
        let top = frame.top as i32;
        let frame_width = frame.width as i32;
        let frame_height = frame.height as i32;
        restore = None;
        if frame.dispose == gif::DisposalMethod::Previous {
            restore_left = left;
            restore_top = top;
            restore_width = frame_width.min(width - left).max(0);
            restore_height = frame_height.min(height - top).max(0);
            if restore_width > 0 && restore_height > 0 {
                restore = Some(snapshot_rect(
                    &canvas,
                    stride,
                    width,
                    height,
                    restore_left,
                    restore_top,
                    restore_width,
                    restore_height,
                ));
            }
        }

        let draw_width = frame_width.min(width - left).max(0);
        let draw_height = frame_height.min(height - top).max(0);
        if draw_width > 0 && draw_height > 0 {
            let mut pixels = frame.buffer.to_vec();
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            blend_rect(
                &mut canvas,
                stride,
                left,
                top,
                draw_width,
                draw_height,
                &pixels,
                frame_width * 4,
            );
        }

        composed.push(canvas.clone());
        delays.push(gif_delay(frame.delay));

        previous_disposal = frame.dispose;
        previous_left = left;
        previous_top = top;
        previous_width = frame_width;
        previous_height = frame_height;
        has_previous = true;
    }

    Some(PreviewImage {
        frames: composed,
        delays,
        width,
        height,
        render_width: width,
        render_height: height,
        file_bytes,
        format: format.to_string(),
    })
}

fn gif_delay(centiseconds: u16) -> i32 {
    if centiseconds <= 1 {
        100
    } else {
        ((centiseconds as i32) * 10).clamp(30, 1000)
    }
}

#[allow(clippy::too_many_arguments)]
fn blend_rect(
    canvas: &mut [u8],
    canvas_stride: usize,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
    source: &[u8],
    source_stride: i32,
) {
    let source_stride = source_stride as usize;
    for y in 0..height as usize {
        let source_row = y * source_stride;
        let destination_row = (top as usize + y) * canvas_stride + left as usize * 4;
        for x in 0..width as usize {
            let s = source_row + x * 4;
            let source_alpha = source[s + 3] as u32;
            if source_alpha == 0 {
                continue;
            }
            let d = destination_row + x * 4;
            if source_alpha == 255 {
                canvas[d] = source[s];
                canvas[d + 1] = source[s + 1];
                canvas[d + 2] = source[s + 2];
                canvas[d + 3] = 255;
                continue;
            }
            let destination_alpha = canvas[d + 3] as u32;
            let inverse = 255 - source_alpha;
            let denominator = source_alpha * 255 + destination_alpha * inverse;
            if denominator == 0 {
                continue;
            }
            for channel in 0..3 {
                canvas[d + channel] = ((source[s + channel] as u32 * source_alpha * 255
                    + canvas[d + channel] as u32 * destination_alpha * inverse
                    + denominator / 2)
                    / denominator) as u8;
            }
            canvas[d + 3] = ((denominator + 127) / 255) as u8;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn clear_rect(
    canvas: &mut [u8],
    stride: usize,
    canvas_width: i32,
    canvas_height: i32,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) {
    let clipped_left = left.clamp(0, canvas_width);
    let clipped_top = top.clamp(0, canvas_height);
    let clipped_width = width.max(0).min(canvas_width - clipped_left);
    let clipped_height = height.max(0).min(canvas_height - clipped_top);
    for y in 0..clipped_height as usize {
        let start = (clipped_top as usize + y) * stride + clipped_left as usize * 4;
        canvas[start..start + clipped_width as usize * 4].fill(0);
    }
}

#[allow(clippy::too_many_arguments)]
fn snapshot_rect(
    canvas: &[u8],
    stride: usize,
    canvas_width: i32,
    canvas_height: i32,
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) -> Vec<u8> {
    let clipped_left = left.clamp(0, canvas_width);
    let clipped_top = top.clamp(0, canvas_height);
    let clipped_width = width.max(0).min(canvas_width - clipped_left);
    let clipped_height = height.max(0).min(canvas_height - clipped_top);
    let row_bytes = clipped_width as usize * 4;
    let mut result = Vec::with_capacity(row_bytes * clipped_height as usize);
    for y in 0..clipped_height as usize {
        let start = (clipped_top as usize + y) * stride + clipped_left as usize * 4;
        result.extend_from_slice(&canvas[start..start + row_bytes]);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn restore_rect(
    canvas: &mut [u8],
    stride: usize,
    canvas_width: i32,
    canvas_height: i32,
    snapshot: &[u8],
    left: i32,
    top: i32,
    width: i32,
    height: i32,
) {
    let clipped_left = left.clamp(0, canvas_width);
    let clipped_top = top.clamp(0, canvas_height);
    let clipped_width = width.max(0).min(canvas_width - clipped_left);
    let clipped_height = height.max(0).min(canvas_height - clipped_top);
    let row_bytes = clipped_width as usize * 4;
    for y in 0..clipped_height as usize {
        let destination = (clipped_top as usize + y) * stride + clipped_left as usize * 4;
        let source = y * row_bytes;
        canvas[destination..destination + row_bytes]
            .copy_from_slice(&snapshot[source..source + row_bytes]);
    }
}

fn format_name(path: &Path) -> String {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    if extension.is_empty() {
        "IMAGE".to_string()
    } else {
        extension.to_uppercase()
    }
}

fn jpeg_orientation(bytes: &[u8]) -> u16 {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return 1;
    }
    let mut index = 2usize;
    while index + 4 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        if marker == 0xE1 {
            let length = ((bytes[index + 2] as usize) << 8) | bytes[index + 3] as usize;
            let start = index + 4;
            let end = (index + 2 + length).min(bytes.len());
            if end <= start + 6 || &bytes[start..start + 6] != b"Exif\0\0" {
                index += 2 + length;
                continue;
            }
            let tiff = &bytes[start + 6..end];
            return read_exif_orientation(tiff);
        }
        if marker == 0xDA || marker == 0xD9 {
            break;
        }
        let length = ((bytes[index + 2] as usize) << 8) | bytes[index + 3] as usize;
        index += 2 + length.max(2);
    }
    1
}

fn read_exif_orientation(tiff: &[u8]) -> u16 {
    if tiff.len() < 8 {
        return 1;
    }
    let little = match &tiff[0..2] {
        b"II" => true,
        b"MM" => false,
        _ => return 1,
    };
    let read_u16 = |offset: usize| -> u16 {
        if offset + 2 > tiff.len() {
            return 0;
        }
        if little {
            u16::from_le_bytes([tiff[offset], tiff[offset + 1]])
        } else {
            u16::from_be_bytes([tiff[offset], tiff[offset + 1]])
        }
    };
    let read_u32 = |offset: usize| -> u32 {
        if offset + 4 > tiff.len() {
            return 0;
        }
        if little {
            u32::from_le_bytes([
                tiff[offset],
                tiff[offset + 1],
                tiff[offset + 2],
                tiff[offset + 3],
            ])
        } else {
            u32::from_be_bytes([
                tiff[offset],
                tiff[offset + 1],
                tiff[offset + 2],
                tiff[offset + 3],
            ])
        }
    };
    let ifd = read_u32(4) as usize;
    if ifd + 2 > tiff.len() {
        return 1;
    }
    let count = read_u16(ifd) as usize;
    for entry in 0..count {
        let offset = ifd + 2 + entry * 12;
        if offset + 12 > tiff.len() {
            break;
        }
        if read_u16(offset) == 0x0112 {
            return read_u16(offset + 8);
        }
    }
    1
}

pub fn rgba_to_bgra(source: &RgbaImage) -> Vec<u8> {
    let mut result = source.as_raw().clone();
    for pixel in result.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    result
}

pub fn bgra_to_rgba(source: &[u8]) -> Vec<u8> {
    let mut result = source.to_vec();
    for pixel in result.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    result
}

pub fn thumbnail_bgra(bgra: &[u8], width: i32, height: i32) -> (Vec<u8>, i32, i32) {
    let scale = 1.0f32.min(360.0 / width.max(1) as f32).min(220.0 / height.max(1) as f32);
    if scale >= 1.0 {
        return (bgra.to_vec(), width, height);
    }
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    let rgba = RgbaImage::from_raw(width as u32, height as u32, bgra_to_rgba(bgra));
    let Some(rgba) = rgba else {
        return (bgra.to_vec(), width, height);
    };
    let resized = image::imageops::resize(
        &rgba,
        target_width,
        target_height,
        image::imageops::FilterType::Lanczos3,
    );
    (
        rgba_to_bgra(&resized),
        target_width as i32,
        target_height as i32,
    )
}

pub fn encode_png(bgra: &[u8], width: i32, height: i32) -> Result<Vec<u8>, String> {
    let rgba = bgra_to_rgba(bgra);
    let mut output = Vec::new();
    PngEncoder::new(&mut output)
        .write_image(
            &rgba,
            width as u32,
            height as u32,
            ExtendedColorType::Rgba8,
        )
        .map_err(|error| error.to_string())?;
    Ok(output)
}

pub fn encode_jpeg(bgra: &[u8], width: i32, height: i32, quality: i32) -> Result<Vec<u8>, String> {
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for pixel in bgra.chunks_exact(4) {
        rgb.push(pixel[2]);
        rgb.push(pixel[1]);
        rgb.push(pixel[0]);
    }
    let mut output = Vec::new();
    JpegEncoder::new_with_quality(&mut output, quality.clamp(1, 100) as u8)
        .write_image(
            &rgb,
            width as u32,
            height as u32,
            ExtendedColorType::Rgb8,
        )
        .map_err(|error| error.to_string())?;
    Ok(output)
}

pub fn save_image_file(
    bgra: &[u8],
    width: i32,
    height: i32,
    path: &Path,
    quality: i32,
) -> Result<(), String> {
    let extension = path
        .extension()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let data = if extension == "jpg" || extension == "jpeg" {
        encode_jpeg(bgra, width, height, quality)?
    } else {
        encode_png(bgra, width, height)?
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(path, data).map_err(|error| error.to_string())
}

pub struct Thumbnail {
    pub bgra: Vec<u8>,
    pub width: i32,
    pub height: i32,
    pub source_width: i32,
    pub source_height: i32,
}

pub fn load_thumbnail(path: &Path, max_width: i32, max_height: i32) -> Option<Thumbnail> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::GdiPlus::{
        BitmapData, CompositingModeSourceCopy, GdipBitmapLockBits, GdipBitmapUnlockBits,
        GdipCreateBitmapFromFile, GdipCreateBitmapFromScan0, GdipDeleteGraphics,
        GdipDisposeImage, GdipDrawImageRectI, GdipGetImageGraphicsContext, GdipGetImageHeight,
        GdipGetImageThumbnail, GdipGetImageWidth, GdipSetCompositingMode, GdipSetInterpolationMode,
        GdipSetPixelOffsetMode, GpBitmap, GpGraphics, GpImage, ImageLockModeRead,
        InterpolationModeHighQualityBicubic, PixelOffsetModeHighQuality, Rect,
    };

    const PIXEL_FORMAT_ARGB: i32 = 0x0026_200A;

    let wide = crate::win::wide(&path.to_string_lossy());
    unsafe {
        let mut image: *mut GpBitmap = std::ptr::null_mut();
        if GdipCreateBitmapFromFile(PCWSTR(wide.as_ptr()), &mut image).0 != 0 {
            return None;
        }
        let image_generic = image as *mut GpImage;
        let mut source_width = 0u32;
        let mut source_height = 0u32;
        if GdipGetImageWidth(image_generic, &mut source_width).0 != 0
            || GdipGetImageHeight(image_generic, &mut source_height).0 != 0
            || source_width == 0
            || source_height == 0
        {
            let _ = GdipDisposeImage(image_generic);
            return None;
        }

        let mut thumb: *mut GpImage = std::ptr::null_mut();
        let fit = (max_width as f32 / source_width as f32)
            .min(max_height as f32 / source_height as f32)
            .min(1.0);
        let request_width = ((source_width as f32 * fit).round() as u32).max(1);
        let request_height = ((source_height as f32 * fit).round() as u32).max(1);
        if GdipGetImageThumbnail(
            image_generic,
            request_width,
            request_height,
            &mut thumb,
            0,
            std::ptr::null_mut(),
        )
        .0 != 0
            || thumb.is_null()
        {
            let _ = GdipDisposeImage(image_generic);
            return None;
        }

        let mut thumb_width = 0u32;
        let mut thumb_height = 0u32;
        let _ = GdipGetImageWidth(thumb, &mut thumb_width);
        let _ = GdipGetImageHeight(thumb, &mut thumb_height);
        if thumb_width == 0 || thumb_height == 0 {
            let _ = GdipDisposeImage(thumb);
            let _ = GdipDisposeImage(image_generic);
            return None;
        }

        let width = thumb_width as i32;
        let height = thumb_height as i32;
        let mut target: *mut GpBitmap = std::ptr::null_mut();
        if GdipCreateBitmapFromScan0(width, height, width * 4, PIXEL_FORMAT_ARGB, None, &mut target).0 != 0 {
            let _ = GdipDisposeImage(thumb);
            let _ = GdipDisposeImage(image_generic);
            return None;
        }

        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipGetImageGraphicsContext(target as *mut GpImage, &mut graphics).0 != 0 {
            let _ = GdipDisposeImage(target as *mut GpImage);
            let _ = GdipDisposeImage(thumb);
            let _ = GdipDisposeImage(image_generic);
            return None;
        }
        let _ = GdipSetCompositingMode(graphics, CompositingModeSourceCopy);
        let _ = GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
        let _ = GdipSetPixelOffsetMode(graphics, PixelOffsetModeHighQuality);
        let _ = GdipDrawImageRectI(graphics, thumb, 0, 0, width, height);
        let _ = GdipDeleteGraphics(graphics);
        let _ = GdipDisposeImage(thumb);

        let rect = Rect {
            X: 0,
            Y: 0,
            Width: width,
            Height: height,
        };
        let mut locked = BitmapData::default();
        let mut bgra = Vec::new();
        if GdipBitmapLockBits(
            target,
            &rect,
            ImageLockModeRead.0 as u32,
            PIXEL_FORMAT_ARGB,
            &mut locked,
        )
        .0 == 0
        {
            let stride = locked.Stride;
            let row_bytes = (width * 4) as usize;
            if !locked.Scan0.is_null() && stride.unsigned_abs() as usize >= row_bytes {
                bgra = vec![0u8; row_bytes * height as usize];
                let base = locked.Scan0 as *const u8;
                for row in 0..height as usize {
                    let source_row = if stride >= 0 {
                        row
                    } else {
                        height as usize - 1 - row
                    };
                    std::ptr::copy_nonoverlapping(
                        base.add(source_row * stride.unsigned_abs() as usize),
                        bgra.as_mut_ptr().add(row * row_bytes),
                        row_bytes,
                    );
                }
            }
            let _ = GdipBitmapUnlockBits(target, &mut locked);
        }
        let _ = GdipDisposeImage(target as *mut GpImage);
        let _ = GdipDisposeImage(image_generic);

        if bgra.is_empty() {
            return None;
        }
        Some(Thumbnail {
            bgra,
            width,
            height,
            source_width: source_width as i32,
            source_height: source_height as i32,
        })
    }
}

pub fn load_png_bgra(path: &Path) -> Result<(Vec<u8>, i32, i32), String> {
    let image = image::open(path).map_err(|error| error.to_string())?.to_rgba8();
    let width = image.width() as i32;
    let height = image.height() as i32;
    Ok((rgba_to_bgra(&image), width, height))
}

pub fn set_clipboard_image(owner: HWND, bgra: &[u8], width: i32, height: i32) -> Result<(), String> {
    let png = encode_png(bgra, width, height)?;
    let mut last_error = String::from("The clipboard is busy.");

    for attempt in 0..4 {
        let opened = unsafe { OpenClipboard(owner).is_ok() };
        if !opened {
            last_error = "The clipboard is busy.".to_string();
            std::thread::sleep(std::time::Duration::from_millis(40));
            continue;
        }

        let result = (|| -> Result<(), String> {
            unsafe {
                EmptyClipboard().map_err(|error| error.to_string())?;

                let dib_size = std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>()
                    + (width * height * 4) as usize;
                let dib_handle = GlobalAlloc(GMEM_MOVEABLE, dib_size)
                    .map_err(|error| error.to_string())?;
                let dib_pointer = GlobalLock(dib_handle);
                if dib_pointer.is_null() {
                    return Err("The clipboard memory could not be locked.".to_string());
                }
                let header = dib_pointer as *mut windows::Win32::Graphics::Gdi::BITMAPINFOHEADER;
                *header = windows::Win32::Graphics::Gdi::BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>()
                        as u32,
                    biWidth: width,
                    biHeight: height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: windows::Win32::Graphics::Gdi::BI_RGB.0,
                    biSizeImage: (width * height * 4) as u32,
                    ..Default::default()
                };
                let pixels = (header as *mut u8)
                    .add(std::mem::size_of::<windows::Win32::Graphics::Gdi::BITMAPINFOHEADER>());
                for row in 0..height as usize {
                    let source_row = (height as usize - 1 - row) * width as usize * 4;
                    let destination_row = row * width as usize * 4;
                    std::ptr::copy_nonoverlapping(
                        bgra.as_ptr().add(source_row),
                        pixels.add(destination_row),
                        width as usize * 4,
                    );
                }
                let _ = GlobalUnlock(dib_handle);
                SetClipboardData(CF_DIB.0 as u32, HANDLE(dib_handle.0))
                    .map_err(|error| error.to_string())?;

                let png_format = RegisterClipboardFormatW(windows::core::w!("PNG"));
                if png_format != 0 {
                    let png_handle = GlobalAlloc(GMEM_MOVEABLE, png.len())
                        .map_err(|error| error.to_string())?;
                    let png_pointer = GlobalLock(png_handle);
                    if !png_pointer.is_null() {
                        std::ptr::copy_nonoverlapping(
                            png.as_ptr(),
                            png_pointer as *mut u8,
                            png.len(),
                        );
                        let _ = GlobalUnlock(png_handle);
                        let _ = SetClipboardData(png_format, HANDLE(png_handle.0));
                    }
                }
            }
            Ok(())
        })();

        unsafe {
            let _ = CloseClipboard();
        }

        match result {
            Ok(()) => return Ok(()),
            Err(error) => {
                last_error = error;
                if attempt < 3 {
                    std::thread::sleep(std::time::Duration::from_millis(40));
                }
            }
        }
    }
    Err(last_error)
}

pub fn get_clipboard_text(owner: HWND) -> Option<String> {
    unsafe {
        if OpenClipboard(owner).is_err() {
            return None;
        }
        let handle = GetClipboardData(CF_UNICODETEXT.0 as u32).ok();
        let result = handle.and_then(|handle| {
            if handle.is_invalid() {
                return None;
            }
            let pointer = GlobalLock(windows::Win32::Foundation::HGLOBAL(handle.0));
            if pointer.is_null() {
                return None;
            }
            let mut length = 0usize;
            let chars = pointer as *const u16;
            while *chars.add(length) != 0 {
                length += 1;
            }
            let value = String::from_utf16_lossy(std::slice::from_raw_parts(chars, length));
            let _ = GlobalUnlock(windows::Win32::Foundation::HGLOBAL(handle.0));
            Some(value)
        });
        let _ = CloseClipboard();
        result
    }
}

pub fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return format!("0 {}", i18n::t("UnitBytes"));
    }
    if bytes < 1024 {
        return format!("{} {}", bytes, i18n::t("UnitBytes"));
    }
    let kilobytes = bytes as f64 / 1024.0;
    if kilobytes < 1024.0 {
        return format!("{} {}", decimal(kilobytes), i18n::t("UnitKb"));
    }
    let megabytes = kilobytes / 1024.0;
    if megabytes < 1024.0 {
        return format!("{} {}", decimal(megabytes), i18n::t("UnitMb"));
    }
    format!(
        "{} {}",
        decimal(megabytes / 1024.0),
        i18n::t("UnitGb")
    )
}

pub fn format_buffer_size(bytes: u64) -> String {
    if bytes == 0 {
        return format!("0 {}", i18n::t("UnitKb"));
    }
    format_size(bytes)
}

fn decimal(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if (rounded - rounded.round()).abs() < 0.05 {
        format!("{}", rounded.round() as i64)
    } else {
        format!("{:.1}", rounded)
    }
}
