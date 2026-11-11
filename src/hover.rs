use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{BSTR, Interface, VARIANT};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationValuePattern, TreeScope_Descendants,
    UIA_ControlTypePropertyId, UIA_EditControlTypeId, UIA_ValuePatternId, UIA_WindowControlTypeId,
};
use windows::Win32::UI::Shell::{
    IWebBrowserApp, IShellWindows, SWC_BROWSER, SWFO_NEEDDISPATCH, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, SW_HIDE};

use crate::images::{self, PreviewImage};
use crate::win::{self, WM_APP};

pub const HOVER_MESSAGE: u32 = WM_APP + 2;

const IDLE_INTERVAL: Duration = Duration::from_millis(130);
const ACTIVE_INTERVAL: Duration = Duration::from_millis(70);
const IMAGE_EXTENSIONS: [&str; 6] = [".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp"];

pub enum HoverEvent {
    Show {
        path: String,
        image: Arc<PreviewImage>,
        cursor: POINT,
    },
    Hide,
}

pub struct HoverController {
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    events: Arc<Mutex<VecDeque<HoverEvent>>>,
    thread: Option<JoinHandle<()>>,
}

impl HoverController {
    pub fn new(main: HWND) -> Self {
        let paused = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let events = Arc::new(Mutex::new(VecDeque::new()));
        let thread_paused = paused.clone();
        let thread_stop = stop.clone();
        let thread_events = events.clone();
        let main_value = main.0 as isize;
        let thread = std::thread::spawn(move || {
            run_worker(main_value, thread_paused, thread_stop, thread_events);
        });
        Self {
            paused,
            stop,
            events,
            thread: Some(thread),
        }
    }

    pub fn set_paused(&self, value: bool) {
        self.paused.store(value, Ordering::Release);
    }

    pub fn drain(&self) -> Vec<HoverEvent> {
        let mut guard = match self.events.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.drain(..).collect()
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for HoverController {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct FolderItem {
    display_name: String,
    full_path: PathBuf,
}

struct Cache {
    folder_window: isize,
    folder: Option<PathBuf>,
    folder_expiry: Instant,
    items_folder: Option<PathBuf>,
    items: Vec<FolderItem>,
    items_expiry: Instant,
}

impl Cache {
    fn new() -> Self {
        Self {
            folder_window: 0,
            folder: None,
            folder_expiry: Instant::now(),
            items_folder: None,
            items: Vec::new(),
            items_expiry: Instant::now(),
        }
    }
}

struct Worker {
    uia: Option<IUIAutomation>,
    cache: Cache,
}

fn run_worker(
    main_value: isize,
    paused: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    events: Arc<Mutex<VecDeque<HoverEvent>>>,
) {
    let main = HWND(main_value as *mut core::ffi::c_void);
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }

    let mut worker = Worker {
        uia: unsafe {
            CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()
        },
        cache: Cache::new(),
    };

    let mut shift_was_down = false;
    let mut suppress_until_release = false;
    let mut paused_sent = false;
    let mut last_x = i32::MIN;
    let mut last_y = i32::MIN;
    let mut current_path: Option<String> = None;

    loop {
        if stop.load(Ordering::Acquire) {
            break;
        }

        let is_paused = paused.load(Ordering::Acquire);
        if is_paused {
            if !paused_sent {
                push_event(&events, main, HoverEvent::Hide);
                paused_sent = true;
                current_path = None;
                last_x = i32::MIN;
                last_y = i32::MIN;
            }
            std::thread::sleep(IDLE_INTERVAL);
            continue;
        }
        if paused_sent {
            paused_sent = false;
            if win::key_is_down(crate::hotkeys::VK_SHIFT) {
                suppress_until_release = true;
                shift_was_down = true;
                last_x = i32::MIN;
                last_y = i32::MIN;
            }
        }

        let shift_down = win::key_is_down(crate::hotkeys::VK_SHIFT);
        if suppress_until_release {
            if shift_down {
                std::thread::sleep(IDLE_INTERVAL);
                continue;
            }
            suppress_until_release = false;
        }

        if !shift_down {
            if shift_was_down || current_path.is_some() {
                push_event(&events, main, HoverEvent::Hide);
                current_path = None;
            }
            shift_was_down = false;
            last_x = i32::MIN;
            last_y = i32::MIN;
            std::thread::sleep(IDLE_INTERVAL);
            continue;
        }

        if !shift_was_down {
            shift_was_down = true;
            last_x = i32::MIN;
            last_y = i32::MIN;
        }

        let cursor = win::cursor_position();
        let moved = last_x == i32::MIN
            || (cursor.x - last_x).abs() >= 3
            || (cursor.y - last_y).abs() >= 3;
        if !moved {
            std::thread::sleep(ACTIVE_INTERVAL);
            continue;
        }
        last_x = cursor.x;
        last_y = cursor.y;

        let Some((root, is_desktop)) = root_window_at(cursor) else {
            if current_path.is_some() {
                push_event(&events, main, HoverEvent::Hide);
                current_path = None;
            }
            std::thread::sleep(ACTIVE_INTERVAL);
            continue;
        };

        let resolved = worker.resolve(cursor, root, is_desktop);
        if stop.load(Ordering::Acquire) {
            break;
        }
        if paused.load(Ordering::Acquire) {
            continue;
        }
        if !win::key_is_down(crate::hotkeys::VK_SHIFT) {
            push_event(&events, main, HoverEvent::Hide);
            current_path = None;
            shift_was_down = false;
            continue;
        }

        let current_cursor = win::cursor_position();
        if (current_cursor.x - cursor.x).abs() > 10 || (current_cursor.y - cursor.y).abs() > 10 {
            std::thread::sleep(ACTIVE_INTERVAL);
            continue;
        }

        match resolved {
            Some(path) => {
                if current_path.as_deref() != Some(path.as_str()) {
                    let loaded = images::load_preview(Path::new(&path));
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    if paused.load(Ordering::Acquire) {
                        continue;
                    }
                    match loaded {
                        Some(image) => {
                            push_event(
                                &events,
                                main,
                                HoverEvent::Show {
                                    path: path.clone(),
                                    image: Arc::new(image),
                                    cursor: current_cursor,
                                },
                            );
                            current_path = Some(path);
                        }
                        None => {
                            push_event(&events, main, HoverEvent::Hide);
                            current_path = None;
                        }
                    }
                }
            }
            None => {
                if current_path.is_some() {
                    push_event(&events, main, HoverEvent::Hide);
                    current_path = None;
                }
            }
        }

        std::thread::sleep(ACTIVE_INTERVAL);
    }

    if current_path.is_some() {
        push_event(&events, main, HoverEvent::Hide);
    }
    unsafe {
        CoUninitialize();
    }
}

fn push_event(events: &Arc<Mutex<VecDeque<HoverEvent>>>, main: HWND, event: HoverEvent) {
    if let Ok(mut guard) = events.lock() {
        if guard.len() > 16 {
            guard.clear();
        }
        guard.push_back(event);
    }
    win::post(main, HOVER_MESSAGE, windows::Win32::Foundation::WPARAM(0), windows::Win32::Foundation::LPARAM(0));
}

fn root_window_at(point: POINT) -> Option<(HWND, bool)> {
    unsafe {
        let hwnd = windows::Win32::UI::WindowsAndMessaging::WindowFromPoint(point);
        if hwnd.is_invalid() {
            return None;
        }
        let mut root = windows::Win32::UI::WindowsAndMessaging::GetAncestor(hwnd, GA_ROOT);
        if root.is_invalid() {
            root = hwnd;
        }
        let mut buffer = [0u16; 256];
        let length = windows::Win32::UI::WindowsAndMessaging::GetClassNameW(root, &mut buffer);
        if length <= 0 {
            return None;
        }
        let class_name = String::from_utf16_lossy(&buffer[..length as usize]);
        if class_name.eq_ignore_ascii_case("Progman")
            || class_name.eq_ignore_ascii_case("WorkerW")
        {
            return Some((root, true));
        }
        if class_name.eq_ignore_ascii_case("CabinetWClass")
            || class_name.eq_ignore_ascii_case("ExploreWClass")
        {
            return Some((root, false));
        }
        None
    }
}

impl Worker {
    fn resolve(&mut self, point: POINT, root: HWND, is_desktop: bool) -> Option<String> {
        let mut candidates = self.collect_candidates(point);
        if candidates.is_empty() {
            return None;
        }

        if is_desktop {
            if let Some(found) = resolve_on_desktop(&candidates) {
                return Some(found);
            }
            return None;
        }

        let folder = self.explorer_folder(root).or_else(|| self.address_bar_folder(root));
        let folder = folder?;
        self.match_in_folder(&folder, &candidates)
    }

    fn collect_candidates(&mut self, point: POINT) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        let Some(uia) = &self.uia else {
            return names;
        };
        unsafe {
            let Ok(mut element) = uia.ElementFromPoint(point) else {
                return names;
            };
            let Ok(walker) = uia.RawViewWalker() else {
                return names;
            };
            for _ in 0..4 {
                let control_type = element.CurrentControlType().unwrap_or_default();
                let name = element.CurrentName().unwrap_or_default().to_string();
                if control_type != UIA_WindowControlTypeId {
                    add_candidate(&mut names, &name);
                }
                let Ok(parent) = walker.GetParentElement(&element) else {
                    break;
                };
                let parent_type = parent.CurrentControlType().unwrap_or_default();
                if parent_type == UIA_WindowControlTypeId {
                    break;
                }
                element = parent;
            }
        }
        names
    }

    fn explorer_folder(&mut self, root: HWND) -> Option<PathBuf> {
        let now = Instant::now();
        if self.cache.folder_window == root.0 as isize && now < self.cache.folder_expiry {
            return self.cache.folder.clone();
        }
        let folder = unsafe { query_explorer_folder(root) };
        self.cache.folder_window = root.0 as isize;
        self.cache.folder = folder.clone();
        self.cache.folder_expiry = now + Duration::from_secs(2);
        folder
    }

    fn address_bar_folder(&mut self, root: HWND) -> Option<PathBuf> {
        let uia = self.uia.as_ref()?;
        unsafe {
            let element = uia.ElementFromHandle(root).ok()?;
            let edit_value = VARIANT::from(UIA_EditControlTypeId.0);
            let condition = uia
                .CreatePropertyCondition(UIA_ControlTypePropertyId, &edit_value)
                .ok()?;
            let matches = element.FindAll(TreeScope_Descendants, &condition).ok()?;
            let count = matches.Length().unwrap_or(0);
            for index in 0..count {
                let Ok(edit) = matches.GetElement(index) else {
                    continue;
                };
                let Ok(pattern) = edit
                    .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                else {
                    continue;
                };
                let value = pattern.CurrentValue().unwrap_or_default().to_string();
                if let Some(path) = normalize_path(&value) {
                    if path.is_dir() {
                        return Some(path);
                    }
                }
            }
        }
        None
    }

    fn match_in_folder(&mut self, folder: &Path, candidates: &[String]) -> Option<String> {
        for candidate in candidates {
            let direct = folder.join(candidate);
            if is_image_file(&direct) && direct.is_file() {
                return Some(direct.to_string_lossy().to_string());
            }
        }

        let items = self.folder_items(folder);
        for item in &items {
            for candidate in candidates {
                let trimmed = candidate.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.eq_ignore_ascii_case(&item.display_name) {
                    return Some(item.full_path.to_string_lossy().to_string());
                }
                let file_name = item
                    .full_path
                    .file_name()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                if trimmed.eq_ignore_ascii_case(&file_name) {
                    return Some(item.full_path.to_string_lossy().to_string());
                }
                let stem = item
                    .full_path
                    .file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                if trimmed.eq_ignore_ascii_case(&stem) {
                    return Some(item.full_path.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    fn folder_items(&mut self, folder: &Path) -> Vec<FolderItem> {
        let now = Instant::now();
        if self.cache.items_folder.as_deref() == Some(folder) && now < self.cache.items_expiry {
            return self.cache.items.clone();
        }
        let mut items = Vec::new();
        if let Ok(entries) = std::fs::read_dir(folder) {
            for entry in entries.flatten().take(20000) {
                let path = entry.path();
                if !is_image_file(&path) || !path.is_file() {
                    continue;
                }
                let display_name = entry.file_name().to_string_lossy().to_string();
                items.push(FolderItem {
                    display_name,
                    full_path: path,
                });
            }
        }
        self.cache.items_folder = Some(folder.to_path_buf());
        self.cache.items = items.clone();
        self.cache.items_expiry = now + Duration::from_millis(1500);
        items
    }
}

impl Clone for FolderItem {
    fn clone(&self) -> Self {
        Self {
            display_name: self.display_name.clone(),
            full_path: self.full_path.clone(),
        }
    }
}

fn resolve_on_desktop(candidates: &[String]) -> Option<String> {
    if let Some(desktop) = crate::settings::desktop_folder() {
        if let Some(found) = match_folder_simple(&desktop, candidates) {
            return Some(found);
        }
    }
    if let Some(desktop) = crate::settings::public_desktop_folder() {
        if let Some(found) = match_folder_simple(&desktop, candidates) {
            return Some(found);
        }
    }
    None
}

fn match_folder_simple(folder: &Path, candidates: &[String]) -> Option<String> {
    for candidate in candidates {
        let direct = folder.join(candidate);
        if is_image_file(&direct) && direct.is_file() {
            return Some(direct.to_string_lossy().to_string());
        }
    }
    if let Ok(entries) = std::fs::read_dir(folder) {
        let entries: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_image_file(path))
            .take(20000)
            .collect();
        for path in entries {
            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            let stem = path
                .file_stem()
                .map(|value| value.to_string_lossy().to_string())
                .unwrap_or_default();
            for candidate in candidates {
                let trimmed = candidate.trim();
                if trimmed.eq_ignore_ascii_case(&file_name)
                    || trimmed.eq_ignore_ascii_case(&stem)
                {
                    return Some(path.to_string_lossy().to_string());
                }
            }
        }
    }
    None
}

fn add_candidate(names: &mut Vec<String>, raw: &str) {
    let mut name = raw.trim().to_string();
    if name.is_empty() || name.len() > 260 {
        return;
    }
    if name.to_ascii_lowercase().starts_with("name: ") {
        name = name[6..].trim().to_string();
    }
    if name.to_ascii_lowercase().ends_with(" - shortcut") {
        name = name[..name.len() - " - Shortcut".len()].trim().to_string();
    }
    if name.is_empty() {
        return;
    }
    if names.iter().any(|existing| existing.eq_ignore_ascii_case(&name)) {
        return;
    }
    names.push(name);
}

fn is_image_file(path: &Path) -> bool {
    path.extension()
        .map(|extension| {
            let extension = format!(".{}", extension.to_string_lossy().to_ascii_lowercase());
            IMAGE_EXTENSIONS.contains(&extension.as_str())
        })
        .unwrap_or(false)
}

unsafe fn query_explorer_folder(root: HWND) -> Option<PathBuf> {
    let windows: IShellWindows = CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
    let mut handle = root.0 as i32;
    let dispatch = windows
        .FindWindowSW(
            std::ptr::null(),
            std::ptr::null(),
            SWC_BROWSER,
            &mut handle,
            SWFO_NEEDDISPATCH,
        )
        .ok()?;
    let browser: IWebBrowserApp = dispatch.cast().ok()?;
    let url: BSTR = browser.LocationURL().ok()?;
    url_to_path(&url.to_string())
}

fn url_to_path(url: &str) -> Option<PathBuf> {
    if url.is_empty() {
        return None;
    }
    let lowercase = url.to_ascii_lowercase();
    if !lowercase.starts_with("file://") {
        return None;
    }
    let rest = &url[7..];
    let rest = rest.trim_start_matches('/');
    let decoded = percent_decode(rest);
    let path = decoded.replace('/', "\\");
    if path.is_empty() {
        None
    } else {
        Some(PathBuf::from(path))
    }
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut result: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex_value(bytes[index + 1]);
            let low = hex_value(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                result.push((high << 4) | low);
                index += 3;
                continue;
            }
        }
        result.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&result).to_string()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn normalize_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.to_ascii_lowercase().starts_with("file://") {
        return url_to_path(trimmed);
    }
    Some(PathBuf::from(trimmed))
}

pub fn hide_preview_window_if_needed(hwnd: HWND) {
    if !hwnd.is_invalid() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(hwnd, SW_HIDE);
        }
    }
}
