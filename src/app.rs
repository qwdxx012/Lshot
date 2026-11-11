use std::path::Path;
use std::time::{Duration, Instant};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, PostQuitMessage, SW_SHOWNORMAL, WM_DESTROY, WM_HOTKEY, WM_TIMER,
    WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::capture;
use crate::clipboard::{self, ClipboardWindow};
use crate::editor::EditorState;
use crate::hover::{HoverController, HoverEvent, HOVER_MESSAGE};
use crate::i18n;
use crate::icon;
use crate::preview::PreviewState;
use crate::settings::{AppSettings, SettingsStore, StartupManager};
use crate::settings_ui::{SettingsWindow, SETTINGS_CLOSED};
use crate::tray::{self, TrayIcon, TRAY_CALLBACK};
use crate::win::{self, TIMER_CAPTURE, WM_APP};

const HOTKEY_REGION: i32 = 1;
const HOTKEY_FULLSCREEN: i32 = 2;
const HOTKEY_CLIPBOARD: i32 = 3;

const MENU_SETTINGS: usize = 101;
const MENU_ABOUT: usize = 102;
const MENU_EXIT: usize = 103;
const CLIPBOARD_CLOSED: u32 = WM_APP + 4;
const EDITOR_CLOSED: u32 = WM_APP + 5;
const MUTEX_NAME: &str = "Local\\Lshot.SingleInstance";

pub struct App {
    pub main: HWND,
    pub settings: AppSettings,
    icon: windows::Win32::UI::WindowsAndMessaging::HICON,
    tray: Option<TrayIcon>,
    settings_win: Option<Box<SettingsWindow>>,
    clipboard_win: Option<Box<ClipboardWindow>>,
    editor: Option<Box<EditorState>>,
    preview: Option<PreviewState>,
    hover: Option<HoverController>,
    capture_in_progress: bool,
    pending_fullscreen: bool,
    settings_was_visible: bool,
    clipboard_was_visible: bool,
    is_exiting: bool,
    last_saved_path: Option<String>,
    last_clipboard_toggle: Option<Instant>,
    mutex: HANDLE,
    owns_mutex: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            main: HWND::default(),
            settings: AppSettings::default(),
            icon: Default::default(),
            tray: None,
            settings_win: None,
            clipboard_win: None,
            editor: None,
            preview: None,
            hover: None,
            capture_in_progress: false,
            pending_fullscreen: false,
            settings_was_visible: false,
            clipboard_was_visible: false,
            is_exiting: false,
            last_saved_path: None,
            last_clipboard_toggle: None,
            mutex: HANDLE::default(),
            owns_mutex: false,
        }
    }

    pub fn initialize(&mut self) -> bool {
        unsafe {
            let name = win::wide(MUTEX_NAME);
            let mutex = match CreateMutexW(None, true, PCWSTR(name.as_ptr())) {
                Ok(handle) => handle,
                Err(_) => return false,
            };
            let already_running =
                windows::Win32::Foundation::GetLastError().0 == windows::Win32::Foundation::ERROR_ALREADY_EXISTS.0;
            if already_running {
                let _ = CloseHandle(mutex);
                return false;
            }
            self.mutex = mutex;
            self.owns_mutex = true;
        }

        let class_name = "LshotMainWindow";
        let class = win::wide(class_name);
        unsafe {
            let class_struct = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(main_proc),
                hInstance: win::module_instance(),
                hCursor: win::arrow_cursor(),
                lpszClassName: win::pcwstr(&class),
                ..Default::default()
            };
            let _ = windows::Win32::UI::WindowsAndMessaging::RegisterClassW(&class_struct);
            let hwnd = win::create_window(
                class_name,
                "Lshot",
                WS_POPUP,
                WS_EX_TOOLWINDOW,
                0,
                0,
                0,
                0,
                HWND::default(),
                None,
            );
            if hwnd.is_invalid() {
                return false;
            }
            self.main = hwnd;
            win::set_user_data(hwnd, self as *mut Self as isize);
        }

        self.settings = SettingsStore::load();
        i18n::set_language(&self.settings.language);
        StartupManager::cleanup_legacy_entry();
        self.settings.run_at_startup = StartupManager::is_enabled();
        if self.settings.run_at_startup {
            let _ = StartupManager::set_enabled(true);
        }
        if !SettingsStore::current_path().exists() {
            let _ = SettingsStore::save(&mut self.settings);
        }

        self.icon = icon::load_application_icon();
        match TrayIcon::create(self.main, self.icon, "Lshot") {
            Some(tray) => self.tray = Some(tray),
            None => {
                self.balloon(
                    i18n::t("ErrorCaption"),
                    "The Lshot tray icon could not be created.",
                    0,
                );
            }
        }

        std::thread::spawn(crate::clipboard::warm_thumbnails);
        self.preview = Some(PreviewState::new());
        self.hover = Some(HoverController::new(self.main));
        self.register_hotkeys();
        true
    }

    pub fn cleanup(&mut self) {
        if let Some(mut tray) = self.tray.take() {
            tray.remove();
        }
        if let Some(hover) = &mut self.hover {
            hover.shutdown();
        }
        self.unregister_hotkeys();
        self.settings_win = None;
        self.clipboard_win = None;
        self.editor = None;
        self.preview = None;
        if self.owns_mutex && !self.mutex.is_invalid() {
            unsafe {
                let _ = ReleaseMutex(self.mutex);
                let _ = CloseHandle(self.mutex);
            }
            self.mutex = HANDLE::default();
            self.owns_mutex = false;
        }
        icon::destroy(self.icon);
        self.icon = Default::default();
    }

    fn rebuild_tray(&self) {
        if let Some(tray) = &self.tray {
            tray.set_tooltip("Lshot");
        }
    }

    fn register_hotkeys(&mut self) {
        self.unregister_hotkeys();
        let region = match crate::hotkeys::parse(&self.settings.region_hotkey) {
            Some(gesture) => gesture,
            None => {
                self.balloon(
                    i18n::t("ErrorCaption"),
                    i18n::t("HotkeyInvalid"),
                    1,
                );
                return;
            }
        };
        let fullscreen = match crate::hotkeys::parse(&self.settings.fullscreen_hotkey) {
            Some(gesture) => gesture,
            None => {
                self.balloon(
                    i18n::t("ErrorCaption"),
                    i18n::t("HotkeyInvalid"),
                    1,
                );
                return;
            }
        };
        let clipboard = match crate::hotkeys::parse(&self.settings.clipboard_hotkey) {
            Some(gesture) => gesture,
            None => {
                self.balloon(
                    i18n::t("ErrorCaption"),
                    i18n::t("HotkeyInvalid"),
                    1,
                );
                return;
            }
        };

        let mut failures: Vec<String> = Vec::new();
        let registrations = [
            (HOTKEY_REGION, &region),
            (HOTKEY_FULLSCREEN, &fullscreen),
            (HOTKEY_CLIPBOARD, &clipboard),
        ];
        for (id, gesture) in registrations {
            let modifiers =
                HOT_KEY_MODIFIERS(gesture.modifiers | crate::hotkeys::MOD_NOREPEAT_VALUE);
            let result = unsafe { RegisterHotKey(self.main, id, modifiers, gesture.vk) };
            if result.is_err() {
                failures.push(gesture.display.clone());
            }
        }
        if !failures.is_empty() {
            self.balloon(
                i18n::t("ErrorCaption"),
                &i18n::t1("HotkeyRegisterError", &failures.join(", ")),
                1,
            );
        }
    }

    fn unregister_hotkeys(&self) {
        if self.main.is_invalid() {
            return;
        }
        unsafe {
            let _ = UnregisterHotKey(self.main, HOTKEY_REGION);
            let _ = UnregisterHotKey(self.main, HOTKEY_FULLSCREEN);
            let _ = UnregisterHotKey(self.main, HOTKEY_CLIPBOARD);
        }
    }

    fn balloon(&mut self, title: &str, text: &str, kind: u32) {
        self.last_saved_path = None;
        if let Some(tray) = &self.tray {
            tray.notify(title, text, kind);
        }
    }

    pub fn notify_saved(&mut self, path: &str) {
        if !self.settings.show_save_notification {
            return;
        }
        if !Path::new(path).exists() {
            return;
        }
        self.last_saved_path = Some(path.to_string());
        let file_name = Path::new(path)
            .file_name()
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|| path.to_string());
        let mut text = i18n::t1("NotificationSavedText", &file_name);
        if text.chars().count() > 240 {
            text = text.chars().take(237).collect::<String>() + "...";
        }
        if let Some(tray) = &self.tray {
            tray.notify(i18n::t("NotificationSavedTitle"), &text, 2);
        }
    }

    fn open_last_saved(&mut self) {
        let Some(path) = self.last_saved_path.take() else {
            return;
        };
        if !Path::new(&path).exists() {
            return;
        }
        let arguments = format!("/select,\"{}\"", path);
        let operation = win::wide("open");
        let file = win::wide("explorer.exe");
        let args = win::wide(&arguments);
        unsafe {
            ShellExecuteW(
                HWND::default(),
                PCWSTR(operation.as_ptr()),
                PCWSTR(file.as_ptr()),
                PCWSTR(args.as_ptr()),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            );
        }
    }

    fn show_about(&mut self) {
        let version = env!("CARGO_PKG_VERSION");
        let text = i18n::t1("AboutText", version);
        let caption = i18n::t("AboutCaption");
        let owner = self
            .settings_win
            .as_ref()
            .map(|window| window.hwnd)
            .filter(|hwnd| win::is_visible(*hwnd))
            .unwrap_or_default();
        win::message_box(
            owner,
            &text,
            caption,
            win::MB_OK | win::MB_ICONINFORMATION,
        );
    }

    fn show_tray_menu(&mut self) {
        let menu = win::create_popup_menu();
        if menu.is_invalid() {
            return;
        }
        win::append_menu(menu, MENU_SETTINGS, i18n::t("TraySettings"));
        win::append_menu(menu, MENU_ABOUT, i18n::t("TrayAbout"));
        win::append_menu(menu, MENU_EXIT, i18n::t("TrayExit"));
        let cursor = win::cursor_position();
        let command = win::show_popup_menu(menu, self.main, cursor.x, cursor.y);
        win::destroy_menu(menu);
        match command {
            MENU_SETTINGS => self.open_settings(),
            MENU_ABOUT => self.show_about(),
            MENU_EXIT => self.exit_application(),
            _ => {}
        }
    }

    fn open_settings(&mut self) {
        if self.is_exiting || self.capture_in_progress {
            return;
        }
        if let Some(window) = &self.clipboard_win {
            win::destroy_window(window.hwnd);
        }
        if let Some(window) = &self.settings_win {
            if !win::is_visible(window.hwnd) {
                win::show(window.hwnd);
            }
            win::activate(window.hwnd);
            return;
        }

        self.unregister_hotkeys();
        let app_pointer = self as *mut App;
        match SettingsWindow::create(self.main, app_pointer, self.settings.clone()) {
            Some(mut window) => {
                win::show(window.hwnd);
                win::activate(window.hwnd);
                window.focus_window();
                self.settings_win = Some(window);
            }
            None => {
                self.register_hotkeys();
                self.balloon(
                    i18n::t("ErrorCaption"),
                    "The settings window could not be created.",
                    0,
                );
            }
        }
    }

    pub fn preview_language(&mut self, language: &str) {
        i18n::set_language(language);
        self.rebuild_tray();
        if let Some(window) = &mut self.clipboard_win {
            window.relocalize();
        }
    }

    pub fn apply_settings(&mut self, mut candidate: AppSettings) -> Result<(), String> {
        candidate.normalize();
        StartupManager::set_enabled(candidate.run_at_startup)
            .map_err(|error| i18n::t1("StartupError", &error))?;
        SettingsStore::save(&mut candidate)
            .map_err(|error| i18n::t1("SettingsSaveError", &error))?;
        self.settings = candidate;
        i18n::set_language(&self.settings.language);
        self.rebuild_tray();
        if let Some(window) = &mut self.clipboard_win {
            window.relocalize();
        }
        Ok(())
    }

    fn hide_own_windows(&mut self) {
        self.hide_preview();
        self.settings_was_visible = self
            .settings_win
            .as_ref()
            .map(|window| win::is_visible(window.hwnd))
            .unwrap_or(false);
        if self.settings_was_visible {
            if let Some(window) = &self.settings_win {
                win::hide(window.hwnd);
            }
        }
        self.clipboard_was_visible = self
            .clipboard_win
            .as_ref()
            .map(|window| win::is_visible(window.hwnd))
            .unwrap_or(false);
        if self.clipboard_was_visible {
            if let Some(window) = &self.clipboard_win {
                win::hide(window.hwnd);
            }
        }
        if let Some(hover) = &self.hover {
            hover.set_paused(true);
        }
    }

    fn restore_own_windows(&mut self) {
        if self.settings_was_visible {
            if let Some(window) = &self.settings_win {
                win::show(window.hwnd);
                win::activate(window.hwnd);
            }
        }
        self.settings_was_visible = false;
        if self.clipboard_was_visible {
            if let Some(window) = &mut self.clipboard_win {
                win::show(window.hwnd);
                win::activate(window.hwnd);
                window.refresh();
            }
        }
        self.clipboard_was_visible = false;
    }

    fn start_capture(&mut self, fullscreen: bool) {
        if self.capture_in_progress || self.is_exiting {
            return;
        }
        self.capture_in_progress = true;
        self.pending_fullscreen = fullscreen;
        self.hide_own_windows();
        win::set_timer(self.main, TIMER_CAPTURE, 160);
    }

    fn finish_capture(&mut self) {
        win::kill_timer(self.main, TIMER_CAPTURE);
        if self.is_exiting {
            self.capture_in_progress = false;
            return;
        }
        let cursor = win::cursor_position();
        let monitor = win::monitor_info(cursor);
        let fullscreen = self.pending_fullscreen;
        let bounds = if fullscreen {
            win::virtual_screen_rect()
        } else {
            monitor.monitor
        };
        match capture::capture(bounds) {
            Ok(screen) => {
                let app_pointer = self as *mut App;
                match EditorState::create(
                    self.main,
                    app_pointer,
                    screen,
                    bounds,
                    monitor.scale,
                    self.settings.clone(),
                    fullscreen,
                ) {
                    Some(mut editor) => {
                        editor.show();
                        self.editor = Some(editor);
                    }
                    None => {
                        self.reset_capture();
                        self.balloon(
                            i18n::t("ErrorCaption"),
                            "The editor could not be created.",
                            0,
                        );
                    }
                }
            }
            Err(error) => {
                self.reset_capture();
                self.balloon(i18n::t("ErrorCaption"), &error, 0);
            }
        }
    }

    fn reset_capture(&mut self) {
        self.capture_in_progress = false;
        if let Some(hover) = &self.hover {
            hover.set_paused(false);
        }
        self.restore_own_windows();
    }

    fn toggle_clipboard(&mut self) {
        if self.is_exiting || self.capture_in_progress {
            return;
        }
        if let Some(last) = self.last_clipboard_toggle {
            if last.elapsed() < Duration::from_millis(320) {
                return;
            }
        }
        self.last_clipboard_toggle = Some(Instant::now());
        if let Some(window) = &self.clipboard_win {
            win::destroy_window(window.hwnd);
            return;
        }
        match ClipboardWindow::create(self.main) {
            Some(mut window) => {
                window.show();
                self.clipboard_win = Some(window);
            }
            None => {
                self.balloon(
                    i18n::t("ErrorCaption"),
                    "The clipboard history window could not be created.",
                    0,
                );
            }
        }
    }

    fn hide_preview(&mut self) {
        if let Some(preview) = &mut self.preview {
            preview.hide();
        }
    }

    fn process_hover_events(&mut self) {
        let events = self
            .hover
            .as_ref()
            .map(|hover| hover.drain())
            .unwrap_or_default();
        for event in events {
            match event {
                HoverEvent::Show { path, image, cursor } => {
                    if let Some(preview) = &mut self.preview {
                        if !preview.show(&path, image, cursor) {
                            preview.hide();
                        }
                    }
                }
                HoverEvent::Hide => self.hide_preview(),
            }
        }
    }

    fn handle_settings_closed(&mut self, pointer: usize) {
        let is_current = self
            .settings_win
            .as_ref()
            .map(|window| window.as_ref() as *const SettingsWindow as usize == pointer)
            .unwrap_or(false);
        if !is_current {
            return;
        }
        self.settings_win = None;
        if self.is_exiting {
            return;
        }
        i18n::set_language(&self.settings.language);
        self.rebuild_tray();
        self.register_hotkeys();
    }

    fn handle_clipboard_closed(&mut self, pointer: usize) {
        let is_current = self
            .clipboard_win
            .as_ref()
            .map(|window| window.as_ref() as *const ClipboardWindow as usize == pointer)
            .unwrap_or(false);
        if is_current {
            self.clipboard_win = None;
        }
    }

    fn handle_editor_closed(&mut self, pointer: usize) {
        let is_current = self
            .editor
            .as_ref()
            .map(|window| window.as_ref() as *const EditorState as usize == pointer)
            .unwrap_or(false);
        if !is_current {
            return;
        }
        self.editor = None;
        self.capture_in_progress = false;
        if let Some(hover) = &self.hover {
            hover.set_paused(false);
        }
        if !self.is_exiting {
            self.restore_own_windows();
        }
    }

    fn exit_application(&mut self) {
        if self.is_exiting {
            return;
        }
        self.is_exiting = true;
        if let Some(mut tray) = self.tray.take() {
            tray.remove();
        }
        if let Some(hover) = &mut self.hover {
            hover.shutdown();
        }
        self.unregister_hotkeys();
        if let Some(window) = &self.settings_win {
            win::destroy_window(window.hwnd);
        }
        if let Some(window) = &self.clipboard_win {
            win::destroy_window(window.hwnd);
        }
        if let Some(window) = &self.editor {
            win::destroy_window(window.hwnd);
        }
        win::destroy_window(self.main);
    }

    fn handle_hotkey(&mut self, id: i32) {
        match id {
            HOTKEY_REGION => self.start_capture(false),
            HOTKEY_FULLSCREEN => self.start_capture(true),
            HOTKEY_CLIPBOARD => self.toggle_clipboard(),
            _ => {}
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.cleanup();
    }
}

extern "system" fn main_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if message == TRAY_CALLBACK {
            let pointer = win::user_data(hwnd) as *mut App;
            if pointer.is_null() {
                return LRESULT(0);
            }
            let app = &mut *pointer;
            if tray::balloon_clicked(lparam) {
                app.open_last_saved();
            } else if tray::tray_right_click(lparam) {
                app.show_tray_menu();
            } else if tray::tray_left_click(lparam) {
                app.open_settings();
            }
            return LRESULT(0);
        }
        if message == HOVER_MESSAGE {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() {
                (*pointer).process_hover_events();
            }
            return LRESULT(0);
        }
        if message == SETTINGS_CLOSED {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() {
                (*pointer).handle_settings_closed(wparam.0);
            }
            return LRESULT(0);
        }
        if message == CLIPBOARD_CLOSED {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() {
                (*pointer).handle_clipboard_closed(wparam.0);
            }
            return LRESULT(0);
        }
        if message == EDITOR_CLOSED {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() {
                (*pointer).handle_editor_closed(wparam.0);
            }
            return LRESULT(0);
        }
        if message == WM_HOTKEY {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() {
                (*pointer).handle_hotkey(wparam.0 as i32);
            }
            return LRESULT(0);
        }
        if message == WM_TIMER {
            let pointer = win::user_data(hwnd) as *mut App;
            if !pointer.is_null() && wparam.0 == TIMER_CAPTURE {
                (*pointer).finish_capture();
            }
            return LRESULT(0);
        }
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return LRESULT(0);
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}
