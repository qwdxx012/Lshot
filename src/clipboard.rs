use std::path::{Path, PathBuf};
use std::time::SystemTime;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_DPICHANGED,
    WM_GETMINMAXINFO, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_SETCURSOR,
    WM_TIMER, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
    WS_THICKFRAME, HWND_TOPMOST,
};

use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use crate::gdiplus::{self, BitmapSurface, ImageBitmap};
use crate::icons::{self, Icon};
use crate::images;
use crate::win::{self, TIMER_STATUS, WM_APP};

pub const CLIPBOARD_CLOSED: u32 = WM_APP + 4;

const TIMER_THUMBS: usize = 11;
const THUMB_POLL_LIMIT: u8 = 60;

pub fn cache_directory() -> PathBuf {
    crate::settings::local_app_data()
        .join("Lshot")
        .join("ClipboardCache")
}

fn thumbs_directory() -> PathBuf {
    cache_directory().join("thumbs")
}

fn thumb_file(source: &Path) -> Option<PathBuf> {
    Some(thumbs_directory().join(source.file_name()?))
}

fn store_thumb(source: &Path, bgra: &[u8], width: i32, height: i32) {
    let Some(target) = thumb_file(source) else {
        return;
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let (thumb, thumb_width, thumb_height) = images::thumbnail_bgra(bgra, width, height);
    let Ok(png) = images::encode_png(&thumb, thumb_width, thumb_height) else {
        return;
    };
    let temporary = target.with_extension("tmp");
    if std::fs::write(&temporary, png).is_ok() {
        let _ = std::fs::rename(&temporary, &target);
    }
}

fn load_or_build_thumb(source: &Path, modified: Option<SystemTime>) -> Option<(Vec<u8>, i32, i32)> {
    if let Some(target) = thumb_file(source) {
        let fresh = match (std::fs::metadata(&target).and_then(|value| value.modified()), modified) {
            (Ok(cached), Some(original)) => cached >= original,
            _ => false,
        };
        if fresh {
            if let Ok(image) = image::open(&target) {
                let rgba = image.to_rgba8();
                let width = rgba.width() as i32;
                let height = rgba.height() as i32;
                return Some((images::rgba_to_bgra(&rgba), width, height));
            }
        }
    }

    let image = image::open(source).ok()?.to_rgba8();
    let width = image.width() as i32;
    let height = image.height() as i32;
    let bgra = images::rgba_to_bgra(&image);
    let (thumb, thumb_width, thumb_height) = images::thumbnail_bgra(&bgra, width, height);
    store_thumb(source, &bgra, width, height);
    Some((thumb, thumb_width, thumb_height))
}

fn load_cached_thumb(source: &Path, modified: Option<SystemTime>) -> Option<(Vec<u8>, i32, i32)> {
    let target = thumb_file(source)?;
    let fresh = match (std::fs::metadata(&target).and_then(|value| value.modified()), modified) {
        (Ok(cached), Some(original)) => cached >= original,
        _ => false,
    };
    if !fresh {
        return None;
    }
    let image = image::open(&target).ok()?.to_rgba8();
    let width = image.width() as i32;
    let height = image.height() as i32;
    Some((images::rgba_to_bgra(&image), width, height))
}

pub fn warm_thumbnails() {
    let directory = cache_directory();
    let Ok(read_dir) = std::fs::read_dir(&directory) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let is_png = path
            .extension()
            .map(|value| value.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if !is_png {
            continue;
        }
        let modified = entry.metadata().ok().and_then(|value| value.modified().ok());
        let _ = load_or_build_thumb(&path, modified);
    }
}

pub struct CacheEntry {
    pub bitmap: Option<ImageBitmap>,
    pub path: PathBuf,
    pub bytes: u64,
    pub modified: SystemTime,
    pub width: i32,
    pub height: i32,
    pub thumb_width: i32,
    pub thumb_height: i32,
    pub thumb_bgra: Vec<u8>,
}

pub fn load_entries() -> Vec<CacheEntry> {
    let directory = cache_directory();
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(&directory) else {
        return entries;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        let is_png = path
            .extension()
            .map(|value| value.eq_ignore_ascii_case("png"))
            .unwrap_or(false);
        if !is_png {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() || metadata.len() == 0 {
            continue;
        }
        let Ok((source_width, source_height)) = image::image_dimensions(&path) else {
            continue;
        };
        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let (bitmap, thumb_width, thumb_height, thumb_bgra) = match load_cached_thumb(&path, metadata.modified().ok()) {
            Some((thumb_bgra, thumb_width, thumb_height)) => {
                let bitmap = unsafe {
                    ImageBitmap::from_bgra(&thumb_bgra, thumb_width, thumb_height)
                };
                (bitmap, thumb_width, thumb_height, thumb_bgra)
            }
            None => (None, 0, 0, Vec::new()),
        };
        entries.push(CacheEntry {
            bitmap,
            path,
            bytes: metadata.len(),
            modified: modified_at,
            width: source_width as i32,
            height: source_height as i32,
            thumb_width,
            thumb_height,
            thumb_bgra,
        });
    }
    entries.sort_by(|left, right| {
        right
            .modified
            .cmp(&left.modified)
            .then_with(|| right.path.cmp(&left.path))
    });
    entries
}

pub fn total_bytes() -> u64 {
    let directory = cache_directory();
    let Ok(read_dir) = std::fs::read_dir(&directory) else {
        return 0;
    };
    let mut total = 0u64;
    for entry in read_dir.flatten() {
        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                total += metadata.len();
            }
        }
    }
    total
}

pub fn save(bgra: &[u8], width: i32, height: i32) -> Option<PathBuf> {
    let directory = cache_directory();
    std::fs::create_dir_all(&directory).ok()?;
    let bytes = images::encode_png(bgra, width, height).ok()?;
    let name = next_file_name();
    let path = directory.join(name);
    std::fs::write(&path, bytes).ok()?;
    store_thumb(&path, bgra, width, height);
    Some(path)
}

fn next_file_name() -> String {
    unsafe {
        let time = windows::Win32::System::SystemInformation::GetLocalTime();
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|value| value.subsec_nanos())
            .unwrap_or(0);
        let random = ((nanos as u64) << 16)
            ^ ((std::process::id() as u64) << 32)
            ^ (time.wMilliseconds as u64);
        format!(
            "Lshot_{:04}{:02}{:02}_{:02}{:02}{:02}_{:03}_{:08x}.png",
            time.wYear,
            time.wMonth,
            time.wDay,
            time.wHour,
            time.wMinute,
            time.wSecond,
            time.wMilliseconds,
            (random & 0xFFFF_FFFF) as u32
        )
    }
}

pub fn delete(path: &Path) -> Result<(), String> {
    if let Some(thumb) = thumb_file(path) {
        let _ = std::fs::remove_file(thumb);
    }
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub fn clear() -> Result<(), String> {
    let directory = cache_directory();
    let Ok(read_dir) = std::fs::read_dir(&directory) else {
        return Ok(());
    };
    let mut last_error = None;
    for entry in read_dir.flatten() {
        if entry.metadata().map(|value| value.is_dir()).unwrap_or(false) {
            continue;
        }
        if let Err(error) = std::fs::remove_file(entry.path()) {
            if last_error.is_none() {
                last_error = Some(error.to_string());
            }
        }
    }
    let _ = std::fs::remove_dir_all(thumbs_directory());
    match last_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}


pub fn set_clipboard_text(owner: HWND, text: &str) -> bool {
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    wide.push(0);
    let bytes = wide.len() * 2;
    unsafe {
        if OpenClipboard(owner).is_err() {
            return false;
        }
        let mut success = false;
        if EmptyClipboard().is_ok() {
            if let Ok(handle) = GlobalAlloc(GMEM_MOVEABLE, bytes) {
                let pointer = GlobalLock(handle);
                if !pointer.is_null() {
                    std::ptr::copy_nonoverlapping(
                        wide.as_ptr() as *const u8,
                        pointer as *mut u8,
                        bytes,
                    );
                    let _ = GlobalUnlock(handle);
                    if SetClipboardData(
                        CF_UNICODETEXT.0 as u32,
                        windows::Win32::Foundation::HANDLE(handle.0),
                    )
                    .is_ok()
                    {
                        success = true;
                    }
                }
            }
        }
        let _ = CloseClipboard();
        success
    }
}

pub fn copy_entry(owner: HWND, entry: &CacheEntry) -> Result<(), String> {
    let (bgra, width, height) = images::load_png_bgra(&entry.path)?;
    images::set_clipboard_image(owner, &bgra, width, height)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum HitPart {
    None,
    Card(usize),
    Delete(usize),
    Copy(usize),
    Clear,
    Scrollbar,
}

#[derive(Clone)]
struct Layout {
    scale: f32,
    client_width: i32,
    client_height: i32,
    header_height: i32,
    footer_height: i32,
    content_top: i32,
    content_bottom: i32,
    inner_top: i32,
    inner_bottom: i32,
    clear_rect: RECT,
    cards_left: i32,
    columns: i32,
    card_width: i32,
    card_height: i32,
    pitch_x: i32,
    pitch_y: i32,
    scroll_track: RECT,
    scroll_thumb: RECT,
    max_scroll: i32,
    title_height: i32,
    meta_height: i32,
    footer_text_height: i32,
    empty: bool,
}

pub struct ClipboardWindow {
    pub hwnd: HWND,
    owner: HWND,
    entries: Vec<CacheEntry>,
    total: u64,
    scroll: i32,
    status: String,
    hover: HitPart,
    pressed: HitPart,
    dragging_scroll: bool,
    scroll_drag_offset: i32,
    backbuffer: Option<BitmapSurface>,
    layout: Option<Layout>,
    tracking_mouse: bool,
    thumb_poll: u8,
}

impl ClipboardWindow {
    pub fn create(owner: HWND) -> Option<Box<Self>> {
        let class_name = "LshotClipboardWindow";
        let class = win::wide(class_name);
        unsafe {
            let class_struct = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(clipboard_proc),
                hInstance: win::module_instance(),
                hCursor: win::arrow_cursor(),
                lpszClassName: win::pcwstr(&class),
                ..Default::default()
            };
            let _ = windows::Win32::UI::WindowsAndMessaging::RegisterClassW(&class_struct);
        }

        let cursor = win::cursor_position();
        let monitor = win::monitor_info(cursor);
        let scale = monitor.scale.max(0.5);
        let width = (960.0 * scale).round() as i32;
        let height = (700.0 * scale).round() as i32;
        let mut state = Box::new(Self {
            hwnd: HWND::default(),
            owner,
            entries: Vec::new(),
            total: 0,
            scroll: 0,
            status: String::new(),
            hover: HitPart::None,
            pressed: HitPart::None,
            dragging_scroll: false,
            scroll_drag_offset: 0,
            backbuffer: None,
            layout: None,
            tracking_mouse: false,
            thumb_poll: 0,
        });

        let style = WS_CAPTION
            | WS_SYSMENU
            | WS_THICKFRAME
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX;
        let hwnd = unsafe {
            win::create_window(
                class_name,
                crate::i18n::t("ClipboardTitle"),
                style,
                windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
                0,
                0,
                width,
                height,
                owner,
                None,
            )
        };
        if hwnd.is_invalid() {
            return None;
        }
        state.hwnd = hwnd;
        win::set_user_data(hwnd, &mut *state as *mut Self as isize);
        win::set_window_text(hwnd, crate::i18n::t("ClipboardTitle"));
        win::center_on_cursor_monitor(hwnd, width, height);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        let large_icon = crate::icon::load_window_icon();
        let small_icon = crate::icon::load_application_icon();
        if !large_icon.is_invalid() && !small_icon.is_invalid() {
            win::set_icon(hwnd, large_icon, small_icon);
        }
        state.refresh();
        Some(state)
    }

    pub fn show(&mut self) {
        win::show(self.hwnd);
        win::activate(self.hwnd);
    }

    pub fn relocalize(&mut self) {
        win::set_window_text(self.hwnd, crate::i18n::t("ClipboardTitle"));
        self.status.clear();
        win::kill_timer(self.hwnd, TIMER_STATUS);
        self.refresh();
    }

    pub fn refresh(&mut self) {
        self.entries = load_entries();
        self.total = total_bytes();
        self.scroll = self.scroll.max(0);
        win::invalidate(self.hwnd);
        self.schedule_thumb_poll();
    }

    fn schedule_thumb_poll(&mut self) {
        let missing = self.entries.iter().any(|entry| entry.bitmap.is_none());
        if missing && self.thumb_poll < THUMB_POLL_LIMIT {
            self.thumb_poll += 1;
            win::set_timer(self.hwnd, TIMER_THUMBS, 320);
        } else {
            self.thumb_poll = 0;
            win::kill_timer(self.hwnd, TIMER_THUMBS);
        }
    }

    fn status_visible(&self) -> bool {
        !self.status.is_empty()
    }

    fn show_status(&mut self, text: &str) {
        self.status = text.to_string();
        win::set_timer(self.hwnd, TIMER_STATUS, 1600);
        win::invalidate(self.hwnd);
    }

    fn clear_status(&mut self) {
        win::kill_timer(self.hwnd, TIMER_STATUS);
        if !self.status.is_empty() {
            self.status.clear();
            win::invalidate(self.hwnd);
        }
    }

    fn layout(&self, g: &gdiplus::Gfx) -> Layout {
        let scale = win::scale(self.hwnd);
        let client = win::client_rect(self.hwnd);
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);

        let title_size = 20.0 * scale;
        let meta_size = 12.0 * scale;
        let (_, title_height) = g.measure_text("Ag", title_size, true, 10000.0, false);
        let (_, meta_height) = g.measure_text("Ag", meta_size, false, 10000.0, false);
        let (_clear_width, clear_height) =
            g.measure_text(crate::i18n::t("ClipboardClear"), 12.5 * scale, false, 10000.0, false);
        let (_, footer_text_height) =
            g.measure_text("Ag", 11.5 * scale, false, 10000.0, false);

        let header_padding_top = (20.0 * scale).round() as i32;
        let header_padding_bottom = (16.0 * scale).round() as i32;
        let header_height = header_padding_top
            + title_height.round() as i32
            + (5.0 * scale).round() as i32
            + meta_height.round() as i32
            + header_padding_bottom
            + 1;

        let footer_height = (10.0 * scale).round() as i32
            + footer_text_height.round() as i32
            + (12.0 * scale).round() as i32
            + 1;
        let content_top = header_height;
        let content_bottom = (height - footer_height).max(header_height + 1);

        let clear_text_width =
            (g.measure_text(crate::i18n::t("ClipboardClear"), 12.5 * scale, false, 10000.0, false).0
                .round() as i32)
                + (28.0 * scale).round() as i32;
        let clear_w = clear_text_width;
        let clear_h = clear_height.round() as i32 + (14.0 * scale).round() as i32;
        let clear_rect = RECT {
            left: width - (24.0 * scale).round() as i32 - clear_w,
            top: header_padding_top + ((title_height + meta_height + 5.0 * scale).round() as i32 - clear_h) / 2,
            right: width - (24.0 * scale).round() as i32,
            bottom: 0,
        };
        let mut clear_rect = clear_rect;
        clear_rect.bottom = clear_rect.top + clear_h;

        let padding_left = (18.0 * scale).round() as i32;
        let padding_right = (18.0 * scale).round() as i32;
        let padding_top = (14.0 * scale).round() as i32;
        let padding_bottom = (18.0 * scale).round() as i32;
        let inner_top = content_top + padding_top;
        let inner_bottom = (content_bottom - padding_bottom).max(inner_top + 1);
        let cards_left = padding_left;
        let available = (width - padding_left - padding_right) + (12.0 * scale).round() as i32;
        let pitch_x = (220.0 * scale).round() as i32;
        let pitch_y = (196.0 * scale).round() as i32;
        let columns = (available / pitch_x).max(1);
        let card_width = (208.0 * scale).round() as i32;
        let card_height = (184.0 * scale).round() as i32;
        let rows = ((self.entries.len() as i32) + columns - 1) / columns;
        let virtual_height = rows * pitch_y;
        let visible_height = inner_bottom - inner_top;
        let max_scroll = (virtual_height - visible_height).max(0);

        let scroll_track = RECT {
            left: width - padding_right - (8.0 * scale).round() as i32,
            top: inner_top,
            right: width - padding_right,
            bottom: inner_bottom,
        };
        let track_height = (scroll_track.bottom - scroll_track.top).max(1);
        let thumb_height = ((visible_height as f64 / virtual_height.max(1) as f64) * track_height as f64)
            .round()
            .max((24.0 * scale).round() as f64) as i32;
        let thumb_height = thumb_height.min(track_height);
        let scroll_ratio = if max_scroll > 0 {
            self.scroll as f64 / max_scroll as f64
        } else {
            0.0
        };
        let thumb_top = scroll_track.top
            + ((track_height - thumb_height) as f64 * scroll_ratio).round() as i32;
        let scroll_thumb = RECT {
            left: scroll_track.left,
            top: thumb_top,
            right: scroll_track.right,
            bottom: thumb_top + thumb_height,
        };

        Layout {
            scale,
            client_width: width,
            client_height: height,
            header_height,
            footer_height,
            content_top,
            content_bottom,
            inner_top,
            inner_bottom,
            clear_rect,
            cards_left,
            columns,
            card_width,
            card_height,
            pitch_x,
            pitch_y,
            scroll_track,
            scroll_thumb,
            max_scroll,
            title_height: title_height.round() as i32,
            meta_height: meta_height.round() as i32,
            footer_text_height: footer_text_height.round() as i32,
            empty: self.entries.is_empty(),
        }
    }

    fn card_rect(&self, layout: &Layout, index: usize) -> RECT {
        let column = index as i32 % layout.columns;
        let row = index as i32 / layout.columns;
        let margin = (6.0 * layout.scale).round() as i32;
        let x = layout.cards_left + margin + column * layout.pitch_x;
        let y = layout.inner_top + margin + row * layout.pitch_y - self.scroll;
        RECT {
            left: x,
            top: y,
            right: x + layout.card_width,
            bottom: y + layout.card_height,
        }
    }

    fn clear_rect(&self, layout: &Layout) -> RECT {
        layout.clear_rect
    }

    fn hit_test(&self, layout: &Layout, x: i32, y: i32) -> HitPart {
        let point = POINT { x, y };
        if point_in(&layout.clear_rect, point) {
            return HitPart::Clear;
        }
        if !layout.empty && layout.max_scroll > 0 && point_in(&layout.scroll_track, point) {
            return HitPart::Scrollbar;
        }
        for (index, _) in self.entries.iter().enumerate() {
            let rect = self.card_rect(layout, index);
            if !point_in(&rect, point) {
                continue;
            }
            let scale = layout.scale;
            let delete_size = (20.0 * scale).round() as i32;
            let margin = (6.0 * scale).round() as i32;
            let delete_rect = RECT {
                left: rect.right - margin - delete_size,
                top: rect.top + margin,
                right: rect.right - margin,
                bottom: rect.top + margin + delete_size,
            };
            if point_in(&delete_rect, point) {
                return HitPart::Delete(index);
            }
            let copy_size = (24.0 * scale).round() as i32;
            let copy_margin = (8.0 * scale).round() as i32;
            let info_top = rect.top + (132.0 * scale).round() as i32;
            let copy_rect = RECT {
                left: rect.right - copy_margin - copy_size,
                top: info_top + ((rect.bottom - info_top) - copy_size) / 2,
                right: rect.right - copy_margin,
                bottom: 0,
            };
            let mut copy_rect = copy_rect;
            copy_rect.bottom = copy_rect.top + copy_size;
            if point_in(&copy_rect, point) {
                return HitPart::Copy(index);
            }
            return HitPart::Card(index);
        }
        HitPart::None
    }

    fn paint(&mut self) {
        let client = win::client_rect(self.hwnd);
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);
        let mut surface = self.backbuffer.take();
        if surface.is_none() {
            surface = BitmapSurface::new(width, height);
        }
        let Some(backbuffer) = surface.as_mut() else {
            return;
        };
        if !backbuffer.resize(width, height) {
            self.backbuffer = surface;
            return;
        }
        let Some(g) = backbuffer.graphics() else {
            self.backbuffer = surface;
            return;
        };

        g.clear(gdiplus::rgb(0x17, 0x18, 0x1B));
        let layout = self.layout(&g);
        let scale = layout.scale;
        self.layout = Some(layout.clone());

        let title_x = (24.0 * scale).round() as i32;
        let title_y = (20.0 * scale).round() as i32;
        g.text(
            crate::i18n::t("ClipboardTitle"),
            title_x as f32,
            title_y as f32,
            (layout.client_width - title_x * 2) as f32,
            layout.title_height as f32 + 2.0,
            20.0 * scale,
            true,
            gdiplus::rgb(0xF2, 0xF3, 0xF5),
        );
        let meta_y = title_y + layout.title_height + (5.0 * scale).round() as i32;
        let size_text = crate::i18n::t1(
            "ClipboardBufferSize",
            &images::format_buffer_size(self.total),
        );
        let (size_width, _) = g.measure_text(&size_text, 12.0 * scale, false, 10000.0, false);
        g.text(
            &size_text,
            title_x as f32,
            meta_y as f32,
            size_width + 4.0,
            layout.meta_height as f32 + 2.0,
            12.0 * scale,
            false,
            gdiplus::rgb(0x9C, 0xA3, 0xAF),
        );
        if self.status_visible() {
            g.text(
                &self.status,
                title_x as f32 + size_width + 16.0 * scale,
                meta_y as f32,
                300.0 * scale,
                layout.meta_height as f32 + 2.0,
                12.0 * scale,
                false,
                gdiplus::rgb(0x7B, 0xD8, 0x8F),
            );
        }

        let clear_rect = self.clear_rect(&layout);
        let clear_hover = self.hover == HitPart::Clear && !self.entries.is_empty();
        let clear_bg = if clear_hover {
            gdiplus::rgb(0x38, 0x22, 0x25)
        } else {
            gdiplus::rgb(0x2A, 0x1D, 0x1F)
        };
        let clear_alpha = if self.entries.is_empty() { 0.45 } else { 1.0 };
        g.fill_round_rect(
            gdiplus::opacity(clear_bg, clear_alpha),
            clear_rect.left,
            clear_rect.top,
            clear_rect.right - clear_rect.left,
            clear_rect.bottom - clear_rect.top,
            (6.0 * scale).round() as i32,
        );
        g.stroke_round_rect(
            gdiplus::opacity(gdiplus::rgb(0x55, 0x30, 0x34), clear_alpha),
            (1.0 * scale).max(1.0),
            clear_rect.left,
            clear_rect.top,
            clear_rect.right - clear_rect.left,
            clear_rect.bottom - clear_rect.top,
            (6.0 * scale).round() as i32,
        );
        g.text_center(
            crate::i18n::t("ClipboardClear"),
            clear_rect.left as f32,
            clear_rect.top as f32,
            (clear_rect.right - clear_rect.left) as f32,
            (clear_rect.bottom - clear_rect.top) as f32,
            12.5 * scale,
            false,
            gdiplus::opacity(gdiplus::rgb(0xFF, 0xB4, 0xB4), clear_alpha),
        );

        g.fill_rect(
            gdiplus::rgb(0x2C, 0x2F, 0x36),
            0,
            layout.content_top - 1,
            layout.client_width,
            1,
        );
        g.fill_rect(
            gdiplus::rgb(0x2C, 0x2F, 0x36),
            0,
            layout.content_bottom,
            layout.client_width,
            1,
        );
        g.text(
            crate::i18n::t("ClipboardHint"),
            (24.0 * scale).round() as f32,
            (layout.content_bottom + (10.0 * scale).round() as i32) as f32,
            (layout.client_width - (48.0 * scale).round() as i32) as f32,
            layout.footer_text_height as f32 + 2.0,
            11.5 * scale,
            false,
            gdiplus::rgb(0x9C, 0xA3, 0xAF),
        );

        if self.entries.is_empty() {
            self.paint_empty(&g, &layout);
        } else {
            self.paint_cards(&g, &layout);
            if layout.max_scroll > 0 {
                g.fill_round_rect(
                    gdiplus::rgb(0x3B, 0x3F, 0x48),
                    layout.scroll_thumb.left,
                    layout.scroll_thumb.top,
                    layout.scroll_thumb.right - layout.scroll_thumb.left,
                    layout.scroll_thumb.bottom - layout.scroll_thumb.top,
                    ((layout.scroll_thumb.right - layout.scroll_thumb.left) / 2).max(2),
                );
            }
        }
        drop(g);

        let (hdc, paint) = win::begin_paint(self.hwnd);
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::BitBlt(
                hdc,
                0,
                0,
                width,
                height,
                backbuffer.hdc,
                0,
                0,
                windows::Win32::Graphics::Gdi::SRCCOPY,
            );
        }
        win::end_paint(self.hwnd, &paint);
        self.backbuffer = surface;
    }

    fn paint_empty(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let center_y = (layout.inner_top + layout.inner_bottom) / 2;
        let icon_w = (58.0 * scale).round() as i32;
        let icon_h = (46.0 * scale).round() as i32;
        let icon_x = (layout.client_width - icon_w) / 2;
        let icon_y = center_y - (70.0 * scale).round() as i32;
        g.fill_round_rect(
            gdiplus::rgb(0x1D, 0x1F, 0x24),
            icon_x,
            icon_y,
            icon_w,
            icon_h,
            (10.0 * scale).round() as i32,
        );
        g.stroke_round_rect(
            gdiplus::rgb(0x2C, 0x30, 0x37),
            (1.0 * scale).max(1.0),
            icon_x,
            icon_y,
            icon_w,
            icon_h,
            (10.0 * scale).round() as i32,
        );
        icons::draw(
            g,
            Icon::Copy,
            icon_x as f32 + icon_w as f32 / 2.0,
            icon_y as f32 + icon_h as f32 / 2.0,
            14.0 * scale,
            gdiplus::rgb(0xF2, 0xF3, 0xF5),
        );

        let title = crate::i18n::t("ClipboardEmptyTitle");
        let hint = crate::i18n::t("ClipboardEmptyHint");
        let title_size = 15.5 * scale;
        let hint_size = 12.5 * scale;
        let hint_width = (420.0 * scale).min(layout.client_width as f32 - 80.0 * scale);
        let (_, title_line) = g.measure_text(title, title_size, true, 10000.0, false);
        let (_, hint_line) = g.measure_text(hint, hint_size, false, hint_width, true);
        let title_y = icon_y + icon_h + (16.0 * scale).round() as i32;
        g.text_center(
            title,
            0.0,
            title_y as f32,
            layout.client_width as f32,
            title_line + 2.0,
            title_size,
            true,
            gdiplus::rgb(0xF2, 0xF3, 0xF5),
        );
        g.text_styled(
            hint,
            (layout.client_width as f32 - hint_width) / 2.0,
            title_y as f32 + title_line + 6.0 * scale,
            hint_width,
            hint_line + 4.0,
            hint_size,
            false,
            gdiplus::rgb(0x9C, 0xA3, 0xAF),
            1,
            0,
            true,
            false,
        );
    }

    fn paint_cards(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let margin = (6.0 * scale).round() as i32;
        let radius = (8.0 * scale).round() as i32;
        let host_height = (132.0 * scale).round() as i32;

        for (index, entry) in self.entries.iter().enumerate() {
            let rect = self.card_rect(layout, index);
            if rect.bottom < layout.content_top || rect.top > layout.content_bottom {
                continue;
            }
            let hovered_card = matches!(self.hover, HitPart::Card(i) | HitPart::Delete(i) | HitPart::Copy(i) if i == index);
            let border = if hovered_card {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else {
                gdiplus::rgb(0x2C, 0x30, 0x37)
            };
            g.fill_round_rect(gdiplus::rgb(0x1D, 0x1F, 0x24), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, radius);
            g.stroke_round_rect(
                border,
                (1.0 * scale).max(1.0),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                radius,
            );

            g.push_clip_round_rect(rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, radius);
            g.fill_rect(
                gdiplus::rgb(0x10, 0x11, 0x14),
                rect.left + 1,
                rect.top + 1,
                rect.right - rect.left - 2,
                host_height,
            );
            let host_left = rect.left + 1 + margin;
            let host_top = rect.top + 1 + margin;
            let host_width = rect.right - rect.left - 2 - margin * 2;
            let host_cell_height = host_height - margin * 2;
            let fit = 1.0f32
                .min(host_width as f32 / entry.thumb_width.max(1) as f32)
                .min(host_cell_height as f32 / entry.thumb_height.max(1) as f32);
            let draw_w = (entry.thumb_width as f32 * fit).round().max(1.0) as i32;
            let draw_h = (entry.thumb_height as f32 * fit).round().max(1.0) as i32;
            let draw_x = host_left + (host_width - draw_w) / 2;
            let draw_y = host_top + (host_cell_height - draw_h) / 2;
            if let Some(bitmap) = &entry.bitmap {
                g.draw_image(bitmap.raw(), draw_x, draw_y, draw_w, draw_h);
            } else {
                g.stroke_round_rect(
                    gdiplus::argb(0x44, 0xFF, 0xFF, 0xFF),
                    (1.0 * scale).max(1.0),
                    draw_x,
                    draw_y,
                    draw_w,
                    draw_h,
                    (4.0 * scale).round() as i32,
                );
            }
            g.reset_clip();

            let info_top = rect.top + host_height;
            let info_center = info_top + (rect.bottom - info_top) / 2;
            g.text(
                &format!("{} x {}", entry.width, entry.height),
                (rect.left + (10.0 * scale).round() as i32) as f32,
                (info_center - (22.0 * scale).round() as i32) as f32,
                (rect.right - rect.left - (44.0 * scale).round() as i32) as f32,
                (18.0 * scale).round() as f32,
                12.5 * scale,
                false,
                gdiplus::rgb(0xF2, 0xF3, 0xF5),
            );
            g.text(
                &images::format_size(entry.bytes),
                (rect.left + (10.0 * scale).round() as i32) as f32,
                (info_center + (1.0 * scale).round() as i32) as f32,
                (rect.right - rect.left - (44.0 * scale).round() as i32) as f32,
                (16.0 * scale).round() as f32,
                11.5 * scale,
                false,
                gdiplus::rgb(0x9C, 0xA3, 0xAF),
            );

            let delete_size = (20.0 * scale).round() as i32;
            let delete_left = rect.right - margin - delete_size;
            let delete_top = rect.top + margin;
            let delete_hover = self.hover == HitPart::Delete(index);
            g.fill_round_rect(
                if delete_hover {
                    gdiplus::argb(0xE6, 0xC4, 0x2B, 0x35)
                } else {
                    gdiplus::argb(0xCC, 0x15, 0x16, 0x19)
                },
                delete_left,
                delete_top,
                delete_size,
                delete_size,
                (4.0 * scale).round() as i32,
            );
            icons::draw(
                g,
                Icon::Close,
                delete_left as f32 + delete_size as f32 / 2.0,
                delete_top as f32 + delete_size as f32 / 2.0,
                14.0 * scale,
                gdiplus::rgb(0xFF, 0xFF, 0xFF),
            );

            let copy_size = (24.0 * scale).round() as i32;
            let copy_left = rect.right - (8.0 * scale).round() as i32 - copy_size;
            let copy_top = info_top + ((rect.bottom - info_top) - copy_size) / 2;
            if self.hover == HitPart::Copy(index) {
                g.fill_round_rect(
                    gdiplus::argb(0x24, 0xFF, 0xFF, 0xFF),
                    copy_left,
                    copy_top,
                    copy_size,
                    copy_size,
                    (4.0 * scale).round() as i32,
                );
            }
            icons::draw(
                g,
                Icon::Copy,
                copy_left as f32 + copy_size as f32 / 2.0,
                copy_top as f32 + copy_size as f32 / 2.0,
                14.0 * scale,
                gdiplus::rgb(0xF2, 0xF3, 0xF5),
            );
        }
    }

    fn mouse_move(&mut self, x: i32, y: i32) {
        let Some(layout) = self.current_layout() else {
            return;
        };
        if self.dragging_scroll {
            self.update_scroll_drag(y, &layout);
            return;
        }
        let hit = self.hit_test(&layout, x, y);
        if hit != self.hover {
            self.hover = hit;
            let cursor = match hit {
                HitPart::None => win::arrow_cursor(),
                HitPart::Scrollbar => win::arrow_cursor(),
                _ => win::hand_cursor(),
            };
            win::set_cursor(cursor);
            win::invalidate(self.hwnd);
        } else if !matches!(hit, HitPart::None) {
            win::set_cursor(win::hand_cursor());
        }
    }

    fn current_layout(&self) -> Option<Layout> {
        self.layout.clone()
    }

    fn update_scroll_drag(&mut self, y: i32, layout: &Layout) {
        let track_height = (layout.scroll_track.bottom - layout.scroll_track.top).max(1);
        let thumb_height = layout.scroll_thumb.bottom - layout.scroll_thumb.top;
        let movable = (track_height - thumb_height).max(1);
        let relative = (y - layout.scroll_track.top - self.scroll_drag_offset).clamp(0, movable);
        self.scroll = ((relative as f64 / movable as f64) * layout.max_scroll as f64).round() as i32;
        win::invalidate(self.hwnd);
    }

    fn mouse_down(&mut self, x: i32, y: i32) {
        let Some(layout) = self.current_layout() else {
            return;
        };
        let hit = self.hit_test(&layout, x, y);
        self.pressed = hit;
        if hit == HitPart::Scrollbar {
            if point_in(&layout.scroll_thumb, POINT { x, y }) {
                self.dragging_scroll = true;
                self.scroll_drag_offset = y - layout.scroll_thumb.top;
                win::capture_mouse(self.hwnd);
            } else {
                let page = layout.inner_bottom - layout.inner_top;
                if y < layout.scroll_thumb.top {
                    self.scroll = (self.scroll - page).max(0);
                } else {
                    self.scroll = (self.scroll + page).min(layout.max_scroll);
                }
                win::invalidate(self.hwnd);
            }
        }
    }

    fn mouse_up(&mut self, x: i32, y: i32) {
        if self.dragging_scroll {
            self.dragging_scroll = false;
            win::release_mouse();
            return;
        }
        let Some(layout) = self.current_layout() else {
            self.pressed = HitPart::None;
            return;
        };
        let hit = self.hit_test(&layout, x, y);
        let pressed = self.pressed;
        self.pressed = HitPart::None;
        if hit != pressed {
            return;
        }
        match hit {
            HitPart::Clear => self.clear_buffer(),
            HitPart::Delete(index) => self.delete_entry(index),
            HitPart::Copy(index) => self.copy_entry(index),
            HitPart::Card(index) => self.copy_entry(index),
            _ => {}
        }
    }

    fn copy_entry(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        let path = entry.path.clone();
        let result = images::load_png_bgra(&path).and_then(|(bgra, width, height)| {
            images::set_clipboard_image(self.hwnd, &bgra, width, height)
        });
        match result {
            Ok(()) => {
                let text = crate::i18n::t("ClipboardCopied").to_string();
                self.show_status(&text);
            }
            Err(error) => {
                crate::dialogs::show_error(
                    self.hwnd,
                    &crate::i18n::t1("CopyFailed", &error),
                );
            }
        }
    }

    fn delete_entry(&mut self, index: usize) {
        let Some(entry) = self.entries.get(index) else {
            return;
        };
        let path = entry.path.clone();
        if let Err(error) = delete(&path) {
            crate::dialogs::show_error(
                self.hwnd,
                &crate::i18n::t1("ClipboardDeleteFailed", &error),
            );
        }
        self.hover = HitPart::None;
        self.refresh();
    }

    fn clear_buffer(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        let message = crate::i18n::t("ClipboardClearConfirm");
        let caption = crate::i18n::t("ClipboardClearCaption");
        if !crate::dialogs::confirm_yes_no(self.hwnd, message, caption) {
            return;
        }
        if let Err(error) = clear() {
            crate::dialogs::show_error(
                self.hwnd,
                &crate::i18n::t1("ClipboardClearFailed", &error),
            );
        }
        self.scroll = 0;
        self.refresh();
    }

    fn wheel(&mut self, delta: i16) {
        let Some(layout) = self.current_layout() else {
            return;
        };
        let step = (60.0 * layout.scale).round() as i32;
        let direction = if delta > 0 { -1 } else { 1 };
        self.scroll = (self.scroll + direction * step * 2).clamp(0, layout.max_scroll);
        win::invalidate(self.hwnd);
    }
}

impl Drop for ClipboardWindow {
    fn drop(&mut self) {
        win::kill_timer(self.hwnd, TIMER_STATUS);
        win::kill_timer(self.hwnd, TIMER_THUMBS);
    }
}

fn point_in(rect: &RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

extern "system" fn clipboard_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let pointer = win::user_data(hwnd) as *mut ClipboardWindow;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let state = &mut *pointer;

        match message {
            windows::Win32::UI::WindowsAndMessaging::WM_PAINT => {
                state.paint();
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_ERASEBKGND => LRESULT(1),
            windows::Win32::UI::WindowsAndMessaging::WM_SIZE => {
                state.layout = None;
                win::invalidate(hwnd);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let scale = win::scale(hwnd);
                info.ptMinTrackSize.x = (620.0 * scale).round() as i32;
                info.ptMinTrackSize.y = (440.0 * scale).round() as i32;
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_STATUS {
                    state.clear_status();
                } else if wparam.0 == TIMER_THUMBS {
                    state.refresh();
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                state.mouse_move(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                if !state.tracking_mouse {
                    state.tracking_mouse = true;
                    let mut track = windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT>() as u32,
                        dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,
                        hwndTrack: hwnd,
                        dwHoverTime: 0,
                    };
                    let _ = windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut track);
                }
                LRESULT(0)
            }
            win::WM_MOUSELEAVE_MSG => {
                state.tracking_mouse = false;
                if state.hover != HitPart::None {
                    state.hover = HitPart::None;
                    win::invalidate(hwnd);
                }
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN => {
                state.mouse_down(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                state.mouse_up(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                state.wheel(win::wheel_delta(wparam));
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if win::loword(lparam.0 as usize) as u32
                    == windows::Win32::UI::WindowsAndMessaging::HTCLIENT
                {
                    let cursor = match state.hover {
                        HitPart::None | HitPart::Scrollbar => win::arrow_cursor(),
                        _ => win::hand_cursor(),
                    };
                    win::set_cursor(cursor);
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN => {
                if win::key_from_wparam(wparam)
                    == windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE.0 as u32
                {
                    win::destroy_window(hwnd);
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
                state.layout = None;
                let suggested = &*(lparam.0 as *const RECT);
                win::move_window(
                    hwnd,
                    suggested.left,
                    suggested.top,
                    suggested.right - suggested.left,
                    suggested.bottom - suggested.top,
                );
                win::invalidate(hwnd);
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_CLOSE => {
                win::destroy_window(hwnd);
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_NCDESTROY => {
                let result = DefWindowProcW(hwnd, message, wparam, lparam);
                win::post(
                    state.owner,
                    CLIPBOARD_CLOSED,
                    WPARAM(pointer as usize),
                    LPARAM(0),
                );
                result
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
