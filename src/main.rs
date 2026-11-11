#![windows_subsystem = "windows"]

mod app;
mod capture;
mod clipboard;
mod dialogs;
mod editor;
mod gdiplus;
mod hover;
mod hotkeys;
mod i18n;
mod icon;
mod icons;
mod images;
mod preview;
mod settings;
mod settings_ui;
mod textedit;
mod tray;
mod win;

use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    let Some(_gdiplus) = gdiplus::GdiPlus::start() else {
        unsafe { CoUninitialize() };
        return;
    };

    let app = Box::into_raw(Box::new(app::App::new()));
    let initialized = unsafe { (*app).initialize() };
    if initialized {
        win::run_message_loop();
        unsafe {
            (*app).cleanup();
        }
    }

    unsafe {
        drop(Box::from_raw(app));
        CoUninitialize();
    }
}
