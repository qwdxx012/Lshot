use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::Graphics::GdiPlus::*;

const PIXEL_FORMAT_ARGB: i32 = 0x0026_200A;
const PIXEL_FORMAT_PARGB: i32 = 0x000E_200B;

pub const COLOR_RED: u32 = 0xFFFF_0000;
pub const TRANSPARENT: u32 = 0x0000_0000;
pub const WHITE: u32 = 0xFFFF_FFFF;

pub fn argb(alpha: u8, red: u8, green: u8, blue: u8) -> u32 {
    ((alpha as u32) << 24) | ((red as u32) << 16) | ((green as u32) << 8) | blue as u32
}

pub fn rgb(red: u8, green: u8, blue: u8) -> u32 {
    argb(255, red, green, blue)
}

pub fn alpha(color: u32) -> u8 {
    (color >> 24) as u8
}

pub fn with_alpha(color: u32, value: u8) -> u32 {
    (color & 0x00FF_FFFF) | ((value as u32) << 24)
}

pub fn opacity(color: u32, factor: f32) -> u32 {
    let value = (alpha(color) as f32 * factor).round().clamp(0.0, 255.0) as u8;
    with_alpha(color, value)
}

pub struct GdiPlus {
    token: usize,
}

impl GdiPlus {
    pub fn start() -> Option<Self> {
        unsafe {
            let input = GdiplusStartupInput {
                GdiplusVersion: 1,
                ..Default::default()
            };
            let mut output = GdiplusStartupOutput::default();
            let mut token = 0usize;
            let status = GdiplusStartup(&mut token, &input, &mut output);
            if status.0 == 0 {
                Some(Self { token })
            } else {
                None
            }
        }
    }
}

impl Drop for GdiPlus {
    fn drop(&mut self) {
        unsafe {
            GdiplusShutdown(self.token);
        }
    }
}

thread_local! {
    static FAMILY: RefCell<Option<usize>> = const { RefCell::new(None) };
    static FONTS: RefCell<HashMap<u64, usize>> = RefCell::new(HashMap::new());
    static FORMATS: RefCell<HashMap<u32, usize>> = RefCell::new(HashMap::new());
    static BRUSHES: RefCell<HashMap<u32, usize>> = RefCell::new(HashMap::new());
}

fn family() -> *mut GpFontFamily {
    FAMILY.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some(value) = *guard {
            return value as *mut GpFontFamily;
        }
        unsafe {
            let name = crate::win::wide("Segoe UI");
            let mut handle: *mut GpFontFamily = std::ptr::null_mut();
            let _ = GdipCreateFontFamilyFromName(PCWSTR(name.as_ptr()), std::ptr::null_mut(), &mut handle);
            *guard = Some(handle as usize);
            handle
        }
    })
}

fn font(size: f32, bold: bool) -> *mut GpFont {
    let key = ((size * 10.0).round() as u32 as u64) | ((bold as u64) << 32);
    FONTS.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some(value) = guard.get(&key) {
            return *value as *mut GpFont;
        }
        unsafe {
            let style = if bold {
                FontStyleBold.0
            } else {
                FontStyleRegular.0
            };
            let mut handle: *mut GpFont = std::ptr::null_mut();
            let pixels = size.round().max(1.0);
            let _ = GdipCreateFont(family(), pixels, style, UnitPixel, &mut handle);
            guard.insert(key, handle as usize);
            handle
        }
    })
}

fn string_format(align: i32, line_align: i32, nowrap: bool, ellipsis: bool) -> *mut GpStringFormat {
    let key = (align as u32 & 0xF)
        | ((line_align as u32 & 0xF) << 4)
        | ((nowrap as u32) << 8)
        | ((ellipsis as u32) << 9);
    FORMATS.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some(value) = guard.get(&key) {
            return *value as *mut GpStringFormat;
        }
        unsafe {
            let mut handle: *mut GpStringFormat = std::ptr::null_mut();
            let _ = GdipCreateStringFormat(0, 0, &mut handle);
            let _ = GdipSetStringFormatAlign(handle, StringAlignment(align));
            let _ = GdipSetStringFormatLineAlign(handle, StringAlignment(line_align));
            if nowrap {
                let _ = GdipSetStringFormatFlags(handle, 0x1000);
            }
            if ellipsis {
                let _ = GdipSetStringFormatTrimming(handle, StringTrimmingEllipsisCharacter);
            }
            guard.insert(key, handle as usize);
            handle
        }
    })
}

fn brush(color: u32) -> *mut GpSolidFill {
    BRUSHES.with(|cell| {
        let mut guard = cell.borrow_mut();
        if let Some(value) = guard.get(&color) {
            return *value as *mut GpSolidFill;
        }
        unsafe {
            let mut handle: *mut GpSolidFill = std::ptr::null_mut();
            let _ = GdipCreateSolidFill(color, &mut handle);
            guard.insert(color, handle as usize);
            handle
        }
    })
}

pub struct Gfx {
    handle: *mut GpGraphics,
}

impl Gfx {
    pub unsafe fn from_hdc(hdc: HDC) -> Option<Self> {
        let mut handle: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut handle).0 != 0 {
            return None;
        }
        let graphics = Self { handle };
        graphics.configure();
        Some(graphics)
    }

    pub unsafe fn from_image(image: *mut GpImage) -> Option<Self> {
        let mut handle: *mut GpGraphics = std::ptr::null_mut();
        if GdipGetImageGraphicsContext(image, &mut handle).0 != 0 {
            return None;
        }
        let graphics = Self { handle };
        graphics.configure();
        Some(graphics)
    }

    fn configure(&self) {
        unsafe {
            let _ = GdipSetSmoothingMode(self.handle, SmoothingModeAntiAlias);
            let _ = GdipSetTextRenderingHint(self.handle, TextRenderingHintClearTypeGridFit);
            let _ = GdipSetInterpolationMode(self.handle, InterpolationModeHighQualityBicubic);
            let _ = GdipSetPixelOffsetMode(self.handle, PixelOffsetModeHalf);
            let _ = GdipSetCompositingQuality(self.handle, CompositingQualityHighQuality);
        }
    }

    pub fn raw(&self) -> *mut GpGraphics {
        self.handle
    }

    pub fn clear(&self, color: u32) {
        unsafe {
            let _ = GdipGraphicsClear(self.handle, color);
        }
    }

    pub fn fill_rect(&self, color: u32, x: i32, y: i32, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        unsafe {
            let _ = GdipFillRectangleI(self.handle, brush(color) as *mut GpBrush, x, y, width, height);
        }
    }

    pub fn fill_rect_f(&self, color: u32, x: f32, y: f32, width: f32, height: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        unsafe {
            let _ = GdipFillRectangle(self.handle, brush(color) as *mut GpBrush, x, y, width, height);
        }
    }

    pub fn fill_round_rect(&self, color: u32, x: i32, y: i32, width: i32, height: i32, radius: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        unsafe {
            if let Some(path) = build_round_rect(x as f32, y as f32, width as f32, height as f32, radius as f32) {
                let _ = GdipFillPath(self.handle, brush(color) as *mut GpBrush, path);
                let _ = GdipDeletePath(path);
            }
        }
    }

    pub fn stroke_round_rect(
        &self,
        color: u32,
        width: f32,
        x: i32,
        y: i32,
        rect_width: i32,
        rect_height: i32,
        radius: i32,
    ) {
        if rect_width <= 0 || rect_height <= 0 {
            return;
        }
        unsafe {
            if let Some(path) = build_round_rect(
                x as f32,
                y as f32,
                rect_width as f32,
                rect_height as f32,
                radius as f32,
            ) {
                let pen = make_pen(color, width, true);
                let _ = GdipDrawPath(self.handle, pen, path);
                let _ = GdipDeletePen(pen);
                let _ = GdipDeletePath(path);
            }
        }
    }

    pub fn line(&self, color: u32, width: f32, x1: f32, y1: f32, x2: f32, y2: f32) {
        unsafe {
            let pen = make_pen(color, width, true);
            let _ = GdipDrawLine(self.handle, pen, x1, y1, x2, y2);
            let _ = GdipDeletePen(pen);
        }
    }

    pub fn line_caps(&self, color: u32, width: f32, x1: f32, y1: f32, x2: f32, y2: f32, square: bool) {
        unsafe {
            let pen = make_pen_caps(color, width, square);
            let _ = GdipDrawLine(self.handle, pen, x1, y1, x2, y2);
            let _ = GdipDeletePen(pen);
        }
    }

    pub fn dashed_rect(&self, color: u32, width: f32, rect: (f32, f32, f32, f32), dash: f32, gap: f32) {
        let (x, y, w, h) = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let mut offset = 0.0f32;
        while offset < w {
            let length = dash.min(w - offset);
            self.line(color, width, x + offset, y, x + offset + length, y);
            self.line(color, width, x + offset, y + h, x + offset + length, y + h);
            offset += dash + gap;
        }
        offset = 0.0;
        while offset < h {
            let length = dash.min(h - offset);
            self.line(color, width, x, y + offset, x, y + offset + length);
            self.line(color, width, x + w, y + offset, x + w, y + offset + length);
            offset += dash + gap;
        }
    }

    pub fn stroke_polyline(&self, color: u32, width: f32, points: &[(f32, f32)], square: bool) {
        if points.len() < 2 {
            return;
        }
        unsafe {
            let mut path: *mut GpPath = std::ptr::null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
                return;
            }
            let mut previous = points[0];
            for point in points.iter().skip(1) {
                let _ = GdipAddPathLine(path, previous.0, previous.1, point.0, point.1);
                previous = *point;
            }
            let pen = if square {
                make_pen_caps(color, width, true)
            } else {
                make_pen(color, width, true)
            };
            let _ = GdipDrawPath(self.handle, pen, path);
            let _ = GdipDeletePen(pen);
            let _ = GdipDeletePath(path);
        }
    }

    pub fn stroke_polygon(&self, color: u32, width: f32, points: &[(f32, f32)]) {
        if points.len() < 2 {
            return;
        }
        unsafe {
            let mut path: *mut GpPath = std::ptr::null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
                return;
            }
            let mut previous = points[0];
            for point in points.iter().skip(1) {
                let _ = GdipAddPathLine(path, previous.0, previous.1, point.0, point.1);
                previous = *point;
            }
            let _ = GdipClosePathFigure(path);
            let pen = make_pen(color, width, true);
            let _ = GdipDrawPath(self.handle, pen, path);
            let _ = GdipDeletePen(pen);
            let _ = GdipDeletePath(path);
        }
    }

    pub fn arc(
        &self,
        color: u32,
        width: f32,
        x: f32,
        y: f32,
        arc_width: f32,
        arc_height: f32,
        start_angle: f32,
        sweep_angle: f32,
    ) {
        unsafe {
            let mut path: *mut GpPath = std::ptr::null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
                return;
            }
            let _ = GdipAddPathArc(path, x, y, arc_width, arc_height, start_angle, sweep_angle);
            let pen = make_pen(color, width, true);
            let _ = GdipDrawPath(self.handle, pen, path);
            let _ = GdipDeletePen(pen);
            let _ = GdipDeletePath(path);
        }
    }

    pub fn fill_circle(&self, color: u32, center_x: f32, center_y: f32, radius: f32) {
        if radius <= 0.0 {
            return;
        }
        unsafe {
            let _ = GdipFillEllipse(
                self.handle,
                brush(color) as *mut GpBrush,
                center_x - radius,
                center_y - radius,
                radius * 2.0,
                radius * 2.0,
            );
        }
    }

    pub fn fill_polygon(&self, color: u32, points: &[(f32, f32)]) {
        if points.len() < 3 {
            return;
        }
        unsafe {
            let mut path: *mut GpPath = std::ptr::null_mut();
            if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
                return;
            }
            let mut previous = points[0];
            for point in points.iter().skip(1) {
                let _ = GdipAddPathLine(path, previous.0, previous.1, point.0, point.1);
                previous = *point;
            }
            let _ = GdipClosePathFigure(path);
            let _ = GdipFillPath(self.handle, brush(color) as *mut GpBrush, path);
            let _ = GdipDeletePath(path);
        }
    }

    pub fn draw_image(&self, image: *mut GpBitmap, x: i32, y: i32, width: i32, height: i32) {
        if width <= 0 || height <= 0 {
            return;
        }
        unsafe {
            let _ = GdipDrawImageRectI(self.handle, image as *mut GpImage, x, y, width, height);
        }
    }

    pub fn draw_image_f(&self, image: *mut GpBitmap, x: f32, y: f32, width: f32, height: f32) {
        if width <= 0.0 || height <= 0.0 {
            return;
        }
        unsafe {
            let _ = GdipDrawImageRect(self.handle, image as *mut GpImage, x, y, width, height);
        }
    }

    pub fn text(
        &self,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        size: f32,
        bold: bool,
        color: u32,
    ) {
        self.text_styled(text, x, y, width, height, size, bold, color, 0, 0, false, false);
    }

    pub fn text_center(
        &self,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        size: f32,
        bold: bool,
        color: u32,
    ) {
        self.text_styled(text, x, y, width, height, size, bold, color, 1, 1, false, false);
    }

    pub fn text_right(
        &self,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        size: f32,
        bold: bool,
        color: u32,
    ) {
        self.text_styled(text, x, y, width, height, size, bold, color, 2, 1, false, false);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn text_styled(
        &self,
        text: &str,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        size: f32,
        bold: bool,
        color: u32,
        align: i32,
        line_align: i32,
        wrap: bool,
        ellipsis: bool,
    ) {
        if text.is_empty() || width <= 0.0 || height <= 0.0 {
            return;
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF {
            X: x,
            Y: y,
            Width: width,
            Height: height,
        };
        unsafe {
            let format = string_format(align, line_align, !wrap, ellipsis);
            let _ = GdipDrawString(
                self.handle,
                PCWSTR(wide_text.as_ptr()),
                wide_text.len() as i32,
                font(size, bold),
                &layout,
                format,
                brush(color) as *mut GpBrush,
            );
        }
    }

    pub fn measure_text(
        &self,
        text: &str,
        size: f32,
        bold: bool,
        max_width: f32,
        wrap: bool,
    ) -> (f32, f32) {
        if text.is_empty() {
            return (0.0, size * 1.35);
        }
        let wide_text: Vec<u16> = text.encode_utf16().collect();
        let layout = RectF {
            X: 0.0,
            Y: 0.0,
            Width: if max_width > 0.0 { max_width } else { 100000.0 },
            Height: 100000.0,
        };
        let mut bounds = RectF::default();
        unsafe {
            let format = string_format(0, 0, !wrap, false);
            let _ = GdipMeasureString(
                self.handle,
                PCWSTR(wide_text.as_ptr()),
                wide_text.len() as i32,
                font(size, bold),
                &layout,
                format,
                &mut bounds,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
        }
        (bounds.Width, bounds.Height)
    }

    pub fn push_clip_rect(&self, x: i32, y: i32, width: i32, height: i32) {
        unsafe {
            let _ = GdipSetClipRectI(self.handle, x, y, width, height, CombineModeIntersect);
        }
    }

    pub fn push_clip_round_rect(&self, x: i32, y: i32, width: i32, height: i32, radius: i32) {
        unsafe {
            if let Some(path) = build_round_rect(
                x as f32,
                y as f32,
                width as f32,
                height as f32,
                radius as f32,
            ) {
                let _ = GdipSetClipPath(self.handle, path, CombineModeIntersect);
                let _ = GdipDeletePath(path);
            }
        }
    }

    pub fn reset_clip(&self) {
        unsafe {
            let _ = GdipResetClip(self.handle);
        }
    }

    pub fn set_transform(&self, m11: f32, m12: f32, m21: f32, m22: f32, dx: f32, dy: f32) {
        unsafe {
            let mut matrix: *mut Matrix = std::ptr::null_mut();
            if GdipCreateMatrix2(m11, m12, m21, m22, dx, dy, &mut matrix).0 == 0 {
                let _ = GdipSetWorldTransform(self.handle, matrix);
                let _ = GdipDeleteMatrix(matrix);
            }
        }
    }

    pub fn reset_transform(&self) {
        unsafe {
            let _ = GdipResetWorldTransform(self.handle);
        }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDeleteGraphics(self.handle);
        }
    }
}

fn make_pen(color: u32, width: f32, rounded: bool) -> *mut GpPen {
    unsafe {
        let mut pen: *mut GpPen = std::ptr::null_mut();
        let _ = GdipCreatePen1(color, width.max(0.5), UnitPixel, &mut pen);
        if rounded {
            let _ = GdipSetPenStartCap(pen, LineCapRound);
            let _ = GdipSetPenEndCap(pen, LineCapRound);
            let _ = GdipSetPenLineJoin(pen, LineJoinRound);
        }
        pen
    }
}

fn make_pen_caps(color: u32, width: f32, square: bool) -> *mut GpPen {
    unsafe {
        let mut pen: *mut GpPen = std::ptr::null_mut();
        let _ = GdipCreatePen1(color, width.max(0.5), UnitPixel, &mut pen);
        let cap = if square { LineCapSquare } else { LineCapRound };
        let _ = GdipSetPenStartCap(pen, cap);
        let _ = GdipSetPenEndCap(pen, cap);
        let _ = GdipSetPenLineJoin(pen, LineJoinRound);
        pen
    }
}

fn build_round_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<*mut GpPath> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    unsafe {
        let mut path: *mut GpPath = std::ptr::null_mut();
        if GdipCreatePath(FillModeAlternate, &mut path).0 != 0 {
            return None;
        }
        let radius = radius.min(width / 2.0).min(height / 2.0).max(0.0);
        if radius <= 0.5 {
            let _ = GdipAddPathRectangle(path, x, y, width, height);
            let _ = GdipClosePathFigure(path);
            return Some(path);
        }
        let diameter = radius * 2.0;
        let _ = GdipStartPathFigure(path);
        let _ = GdipAddPathLine(path, x + radius, y, x + width - radius, y);
        let _ = GdipAddPathArc(path, x + width - diameter, y, diameter, diameter, 270.0, 90.0);
        let _ = GdipAddPathLine(path, x + width, y + radius, x + width, y + height - radius);
        let _ = GdipAddPathArc(
            path,
            x + width - diameter,
            y + height - diameter,
            diameter,
            diameter,
            0.0,
            90.0,
        );
        let _ = GdipAddPathLine(path, x + width - radius, y + height, x + radius, y + height);
        let _ = GdipAddPathArc(path, x, y + height - diameter, diameter, diameter, 90.0, 90.0);
        let _ = GdipAddPathLine(path, x, y + height - radius, x, y + radius);
        let _ = GdipAddPathArc(path, x, y, diameter, diameter, 180.0, 90.0);
        let _ = GdipClosePathFigure(path);
        Some(path)
    }
}

pub struct ImageBitmap {
    handle: *mut GpBitmap,
    width: i32,
    height: i32,
}

impl ImageBitmap {
    pub unsafe fn from_bgra(data: &[u8], width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 || data.len() < (width * height * 4) as usize {
            return None;
        }
        let mut handle: *mut GpBitmap = std::ptr::null_mut();
        let stride = width * 4;
        let status = GdipCreateBitmapFromScan0(
            width,
            height,
            stride,
            PIXEL_FORMAT_ARGB,
            Some(data.as_ptr()),
            &mut handle,
        );
        if status.0 != 0 {
            return None;
        }
        Some(Self { handle, width, height })
    }

    pub fn raw(&self) -> *mut GpBitmap {
        self.handle
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }
}

impl Drop for ImageBitmap {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDisposeImage(self.handle as *mut GpImage);
        }
    }
}

pub struct BitmapSurface {
    pub hdc: HDC,
    hbitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    pub width: i32,
    pub height: i32,
}

impl BitmapSurface {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        unsafe {
            let screen = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND::default());
            let hdc = CreateCompatibleDC(screen);
            windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND::default(), screen);
            if hdc.is_invalid() {
                return None;
            }
            let info = bitmap_info(width, height);
            let mut bits: *mut c_void = std::ptr::null_mut();
            let hbitmap = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                std::result::Result::Ok(value) => value,
                std::result::Result::Err(_) => {
                    let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
                    return None;
                }
            };
            if hbitmap.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, hbitmap);
            Some(Self {
                hdc,
                hbitmap,
                old,
                bits: bits as *mut u8,
                width,
                height,
            })
        }
    }

    pub fn from_bgra(data: &[u8], width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 || data.len() < (width * height * 4) as usize {
            return None;
        }
        let mut surface = Self::new(width, height)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                surface.bits,
                (width * height * 4) as usize,
            );
        }
        Some(surface)
    }

    pub fn resize(&mut self, width: i32, height: i32) -> bool {
        if width == self.width && height == self.height {
            return true;
        }
        if let Some(replacement) = Self::new(width, height) {
            let old_hdc = self.hdc;
            let old_bitmap = self.hbitmap;
            let old_object = self.old;
            let bits = self.bits;
            *self = replacement;
            unsafe {
                let _ = SelectObject(old_hdc, old_object);
                let _ = DeleteObject(old_bitmap);
                let _ = DeleteDC(old_hdc);
            }
            let _ = bits;
            true
        } else {
            false
        }
    }

    pub fn bits(&self) -> *mut u8 {
        self.bits
    }

    pub fn graphics(&self) -> Option<Gfx> {
        unsafe { Gfx::from_hdc(self.hdc) }
    }
}

impl Drop for BitmapSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.hdc, self.old);
            let _ = DeleteObject(self.hbitmap);
            let _ = DeleteDC(self.hdc);
        }
    }
}

pub struct LayeredSurface {
    pub hdc: HDC,
    hbitmap: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    bitmap: *mut GpBitmap,
    pub width: i32,
    pub height: i32,
}

impl LayeredSurface {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        unsafe {
            let screen = windows::Win32::Graphics::Gdi::GetDC(windows::Win32::Foundation::HWND::default());
            let hdc = CreateCompatibleDC(screen);
            windows::Win32::Graphics::Gdi::ReleaseDC(windows::Win32::Foundation::HWND::default(), screen);
            if hdc.is_invalid() {
                return None;
            }
            let info = bitmap_info(width, height);
            let mut bits: *mut c_void = std::ptr::null_mut();
            let hbitmap = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                std::result::Result::Ok(value) => value,
                std::result::Result::Err(_) => {
                    let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
                    return None;
                }
            };
            if hbitmap.is_invalid() {
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
                return None;
            }
            let old = SelectObject(hdc, hbitmap);
            let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
            let status = GdipCreateBitmapFromScan0(
                width,
                height,
                width * 4,
                PIXEL_FORMAT_PARGB,
                Some(bits as *const u8),
                &mut bitmap,
            );
            if status.0 != 0 {
                let _ = SelectObject(hdc, old);
                let _ = DeleteObject(hbitmap);
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(hdc);
                return None;
            }
            Some(Self {
                hdc,
                hbitmap,
                old,
                bits: bits as *mut u8,
                bitmap,
                width,
                height,
            })
        }
    }

    pub fn ensure_size(&mut self, width: i32, height: i32) -> bool {
        if width == self.width && height == self.height {
            return true;
        }
        if let Some(replacement) = Self::new(width, height) {
            let old_hdc = self.hdc;
            let old_bitmap = self.hbitmap;
            let old_object = self.old;
            let old_image = self.bitmap;
            *self = replacement;
            unsafe {
                let _ = GdipDisposeImage(old_image as *mut GpImage);
                let _ = SelectObject(old_hdc, old_object);
                let _ = DeleteObject(old_bitmap);
                let _ = DeleteDC(old_hdc);
            }
            true
        } else {
            false
        }
    }

    pub fn graphics(&self) -> Option<Gfx> {
        unsafe { Gfx::from_image(self.bitmap as *mut GpImage) }
    }

    pub fn image(&self) -> *mut GpBitmap {
        self.bitmap
    }
}

impl Drop for LayeredSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = GdipDisposeImage(self.bitmap as *mut GpImage);
            let _ = SelectObject(self.hdc, self.old);
            let _ = DeleteObject(self.hbitmap);
            let _ = DeleteDC(self.hdc);
        }
    }
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
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
    info
}

pub fn colorref(color: u32) -> COLORREF {
    let red = ((color >> 16) & 0xFF) as u8;
    let green = ((color >> 8) & 0xFF) as u8;
    let blue = (color & 0xFF) as u8;
    COLORREF(red as u32 | ((green as u32) << 8) | ((blue as u32) << 16))
}

pub fn rect_from(x: i32, y: i32, width: i32, height: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + width,
        bottom: y + height,
    }
}
