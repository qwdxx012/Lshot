use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::GdiPlus::GpBitmap;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, HCURSOR, HMENU, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::gdiplus::{self, ImageBitmap, LayeredSurface};
use crate::images::PreviewImage;
use crate::win::{self, TIMER_PREVIEW};

const MAX_PREVIEW_WIDTH: f32 = 860.0;
const MAX_PREVIEW_HEIGHT: f32 = 680.0;

struct PreviewLayout {
    scale: f32,
    outer_width: i32,
    outer_height: i32,
    pad: i32,
    card_width: i32,
    card_height: i32,
    image_x: i32,
    image_y: i32,
    image_width: i32,
    image_height: i32,
    name_y: i32,
    meta_y: i32,
    name_size: f32,
    meta_size: f32,
    text_x: i32,
    text_width: i32,
    name_line: f32,
    meta_line: f32,
}

pub struct PreviewState {
    hwnd: HWND,
    loaded: Option<Arc<PreviewImage>>,
    bitmap: Option<ImageBitmap>,
    frame_index: usize,
    path: String,
    surface: Option<LayeredSurface>,
    layout: Option<PreviewLayout>,
    visible: bool,
}

impl PreviewState {
    pub fn new() -> Self {
        let class_name = "LshotPreviewWindow";
        let class = win::wide(class_name);
        unsafe {
            let class_struct = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(preview_proc),
                hInstance: win::module_instance(),
                hCursor: win::arrow_cursor(),
                lpszClassName: win::pcwstr(&class),
                ..Default::default()
            };
            let _ = windows::Win32::UI::WindowsAndMessaging::RegisterClassW(&class_struct);
            let hwnd = win::create_window(
                class_name,
                "",
                WS_POPUP,
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
                -20000,
                -20000,
                10,
                10,
                HWND::default(),
                None,
            );
            let mut state = Self {
                hwnd,
                loaded: None,
                bitmap: None,
                frame_index: 0,
                path: String::new(),
                surface: None,
                layout: None,
                visible: false,
            };
            win::set_user_data(hwnd, &mut state as *mut Self as isize);
            state
        }
    }

    pub fn show(&mut self, path: &str, image: Arc<PreviewImage>, cursor: POINT) -> bool {
        if self.path == path && self.loaded.is_some() {
            return true;
        }
        self.bitmap = None;
        self.loaded = Some(image);
        self.path = path.to_string();
        self.frame_index = 0;
        if !self.create_bitmap() {
            self.hide();
            return false;
        }
        if !self.compute_layout(cursor) {
            self.hide();
            return false;
        }
        if !self.render() {
            self.hide();
            return false;
        }
        self.position(cursor);
        win::show_without_activation(self.hwnd);
        self.visible = true;
        self.update_animation_timer();
        true
    }

    pub fn hide(&mut self) {
        win::kill_timer(self.hwnd, TIMER_PREVIEW);
        self.visible = false;
        win::hide(self.hwnd);
        self.bitmap = None;
        self.loaded = None;
        self.path.clear();
        self.frame_index = 0;
        self.layout = None;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn current_path(&self) -> &str {
        &self.path
    }

    fn create_bitmap(&mut self) -> bool {
        let Some(loaded) = &self.loaded else {
            return false;
        };
        if loaded.frames.is_empty() {
            return false;
        }
        let index = self.frame_index.min(loaded.frames.len() - 1);
        let frame = &loaded.frames[index];
        self.bitmap = unsafe {
            ImageBitmap::from_bgra(frame, loaded.render_width, loaded.render_height)
        };
        self.bitmap.is_some()
    }

    fn compute_layout(&mut self, cursor: POINT) -> bool {
        let Some(loaded) = &self.loaded else {
            return false;
        };
        let monitor = win::monitor_info(cursor);
        if monitor.work.right <= monitor.work.left || monitor.work.bottom <= monitor.work.top {
            return false;
        }
        let scale = monitor.scale.max(0.5);
        let scale_i = |value: f32| (value * scale).round() as i32;
        let available_width = ((monitor.work.right - monitor.work.left) as f32 / scale).max(260.0);
        let available_height = ((monitor.work.bottom - monitor.work.top) as f32 / scale).max(200.0);
        let max_width = MAX_PREVIEW_WIDTH.min(available_width * 0.88);
        let max_height = MAX_PREVIEW_HEIGHT.min(available_height * 0.88);
        let image_max_width = (max_width - 22.0).max(120.0);
        let image_max_height = (max_height - 84.0).max(100.0);

        let render_width = loaded.render_width.max(1) as f32;
        let render_height = loaded.render_height.max(1) as f32;
        let fit = 1.0f32
            .min(image_max_width / render_width)
            .min(image_max_height / render_height);
        let image_width = (render_width * fit).round().max(1.0) as i32;
        let image_height = (render_height * fit).round().max(1.0) as i32;

        let text_max = (max_width - 30.0).clamp(180.0, 700.0);
        let name_size = 13.0 * scale;
        let meta_size = 11.0 * scale;
        let name = Path::new(&self.path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone());
        let meta = self.meta_text();
        let measure_gfx = win::screen_dc();
        let _ = measure_gfx;
        let (name_width, name_line) = measure_with_scale(&name, name_size, true, text_max * scale);
        let (meta_width, meta_line) = measure_with_scale(&meta, meta_size, false, text_max * scale);
        let measured_text = name_width.max(meta_width).min(text_max * scale);
        let content_width = (image_width as f32 * scale).max(measured_text);
        let card_width = (content_width + 22.0 * scale)
            .max(180.0 * scale)
            .min(max_width * scale);
        let footer_height = 10.0 * scale + name_line + 3.0 * scale + meta_line + 2.0 * scale;
        let card_height = ((image_height as f32 * scale) + 20.0 * scale + footer_height)
            .max(120.0 * scale)
            .min(max_height * scale);

        let pad = scale_i(12.0);
        let outer_width = card_width.round() as i32 + pad * 2;
        let outer_height = card_height.round() as i32 + pad * 2;
        let image_x = pad + ((card_width.round() as i32 - (image_width as f32 * scale).round() as i32) / 2);
        let image_y = pad + scale_i(10.0);
        let name_y = image_y + (image_height as f32 * scale).round() as i32 + scale_i(10.0);
        let meta_y = name_y + name_line.round() as i32 + scale_i(3.0);
        let text_x = pad + scale_i(14.0);
        let text_width = (card_width.round() as i32 - scale_i(28.0)).max(1);

        self.layout = Some(PreviewLayout {
            scale,
            outer_width,
            outer_height,
            pad,
            card_width: card_width.round() as i32,
            card_height: card_height.round() as i32,
            image_x,
            image_y,
            image_width: (image_width as f32 * scale).round() as i32,
            image_height: (image_height as f32 * scale).round() as i32,
            name_y,
            meta_y,
            name_size,
            meta_size,
            text_x,
            text_width,
            name_line,
            meta_line,
        });
        true
    }

    fn meta_text(&self) -> String {
        let Some(loaded) = &self.loaded else {
            return String::new();
        };
        format!(
            "{} x {}  •  {}  •  {}",
            loaded.width,
            loaded.height,
            preview_bytes(loaded.file_bytes),
            loaded.format
        )
    }

    fn position(&mut self, cursor: POINT) {
        let Some(layout) = &self.layout else {
            return;
        };
        let monitor = win::monitor_info(cursor);
        let margin = (18.0 * layout.scale).round() as i32;
        let mut x = cursor.x + margin;
        let mut y = cursor.y + margin;
        if x + layout.outer_width > monitor.work.right {
            x = cursor.x - layout.outer_width - margin;
        }
        if y + layout.outer_height > monitor.work.bottom {
            y = cursor.y - layout.outer_height - margin;
        }
        let min_x = monitor.work.left + 4;
        let min_y = monitor.work.top + 4;
        let max_x = (monitor.work.right - layout.outer_width - 4).max(min_x);
        let max_y = (monitor.work.bottom - layout.outer_height - 4).max(min_y);
        x = x.clamp(min_x, max_x);
        y = y.clamp(min_y, max_y);
        win::set_topmost_bounds(
            self.hwnd,
            x,
            y,
            layout.outer_width,
            layout.outer_height,
            false,
        );
    }

    fn render(&mut self) -> bool {
        let Some(layout) = &self.layout else {
            return false;
        };
        let Some(loaded) = &self.loaded else {
            return false;
        };
        if self.bitmap.is_none() {
            return false;
        }
        let width = layout.outer_width;
        let height = layout.outer_height;
        if width <= 0 || height <= 0 {
            return false;
        }

        let name = Path::new(&self.path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.clone());
        let meta = self.meta_text();
        let bitmap_pointer = self.bitmap.as_ref().map(|bitmap| bitmap.raw());

        if self.surface.is_none() {
            self.surface = LayeredSurface::new(width, height);
        }
        let Some(surface) = self.surface.as_mut() else {
            return false;
        };
        if !surface.ensure_size(width, height) {
            return false;
        }

        let Some(g) = surface.graphics() else {
            return false;
        };
        g.clear(0x0000_0000);

        let radius = (10.0 * layout.scale).round() as i32;
        let pad = layout.pad;
        let shadow_steps = 10;
        for step in 0..shadow_steps {
            let expand = ((shadow_steps - step) as f32 * layout.scale).round() as i32;
            let alpha = 4u8;
            g.fill_round_rect(
                gdiplus::argb(alpha, 0, 0, 0),
                pad - expand,
                pad - expand + (3.0 * layout.scale).round() as i32,
                layout.card_width + expand * 2,
                layout.card_height + expand * 2,
                radius + expand,
            );
        }

        g.fill_round_rect(
            gdiplus::argb(0xF4, 0x15, 0x16, 0x19),
            pad,
            pad,
            layout.card_width,
            layout.card_height,
            radius,
        );
        g.stroke_round_rect(
            gdiplus::argb(0x55, 0xFF, 0xFF, 0xFF),
            (1.0 * layout.scale).max(1.0),
            pad,
            pad,
            layout.card_width,
            layout.card_height,
            radius,
        );

        if let Some(bitmap) = bitmap_pointer {
            g.push_clip_round_rect(pad, pad, layout.card_width, layout.card_height, radius);
            g.draw_image(
                bitmap,
                layout.image_x,
                layout.image_y,
                layout.image_width,
                layout.image_height,
            );
            g.reset_clip();
        }

        g.text_styled(
            &name,
            layout.text_x as f32,
            layout.name_y as f32,
            layout.text_width as f32,
            layout.name_line + 2.0,
            layout.name_size,
            true,
            gdiplus::rgb(0xF2, 0xF4, 0xF7),
            0,
            0,
            false,
            true,
        );
        g.text_styled(
            &meta,
            layout.text_x as f32,
            layout.meta_y as f32,
            layout.text_width as f32,
            layout.meta_line + 2.0,
            layout.meta_size,
            false,
            gdiplus::rgb(0xA8, 0xB0, 0xBC),
            0,
            0,
            false,
            true,
        );
        drop(g);

        let screen = win::screen_dc();
        let bounds = win::window_rect(self.hwnd);
        win::layered_update(
            self.hwnd,
            screen,
            bounds.left,
            bounds.top,
            width,
            height,
            surface.hdc,
        );
        win::release_screen_dc(screen);
        let _ = loaded;
        true
    }

    fn update_animation_timer(&mut self) {
        let Some(loaded) = &self.loaded else {
            return;
        };
        if loaded.frames.len() > 1 && self.visible {
            let delay = loaded
                .delays
                .get(self.frame_index)
                .copied()
                .unwrap_or(100)
                .clamp(30, 1000) as u32;
            win::set_timer(self.hwnd, TIMER_PREVIEW, delay);
        } else {
            win::kill_timer(self.hwnd, TIMER_PREVIEW);
        }
    }

    fn advance(&mut self) {
        let Some(loaded) = &self.loaded else {
            return;
        };
        if loaded.frames.len() < 2 || !self.visible {
            win::kill_timer(self.hwnd, TIMER_PREVIEW);
            return;
        }
        self.frame_index = (self.frame_index + 1) % loaded.frames.len();
        self.bitmap = None;
        if !self.create_bitmap() {
            return;
        }
        if self.render() {
            self.update_animation_timer();
        }
    }
}

fn measure_with_scale(text: &str, size: f32, bold: bool, max_width: f32) -> (f32, f32) {
    let surface = match crate::gdiplus::BitmapSurface::new(4, 4) {
        Some(surface) => surface,
        None => return (0.0, size * 1.35),
    };
    let Some(g) = surface.graphics() else {
        return (0.0, size * 1.35);
    };
    g.measure_text(text, size, bold, max_width, false)
}

extern "system" fn preview_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == win::WM_TIMER_MSG && wparam.0 == TIMER_PREVIEW {
            let pointer = win::user_data(hwnd) as *mut PreviewState;
            if !pointer.is_null() {
                (*pointer).advance();
            }
            return LRESULT(0);
        }
        if message == win::WM_NCHITTEST_MSG {
            return LRESULT(win::HTTRANSPARENT_VALUE);
        }
        if message == windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND {
            return LRESULT(1);
        }
        if message == windows::Win32::UI::WindowsAndMessaging::WM_PAINT {
            let (hdc, paint) = win::begin_paint(hwnd);
            let _ = hdc;
            win::end_paint(hwnd, &paint);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

fn preview_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    if bytes < 1024 * 1024 {
        return format!("{:.1} KB", bytes as f64 / 1024.0);
    }
    format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
}

impl Drop for PreviewState {
    fn drop(&mut self) {
        self.bitmap = None;
        self.loaded = None;
    }
}
