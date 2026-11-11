use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO,
    NIIF_WARNING, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{HICON, WM_APP};

use crate::win::{self, WM_APP as APP_MESSAGE};

pub const TRAY_CALLBACK: u32 = WM_APP + 1;

pub struct TrayIcon {
    hwnd: HWND,
    icon: HICON,
    added: bool,
}

impl TrayIcon {
    pub fn create(hwnd: HWND, icon: HICON, tooltip: &str) -> Option<Self> {
        let mut tray = Self {
            hwnd,
            icon,
            added: false,
        };
        let mut data = tray.data();
        data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        data.uCallbackMessage = TRAY_CALLBACK;
        data.hIcon = icon;
        copy_wide(&mut data.szTip, tooltip);
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return None;
            }
        }
        tray.added = true;
        Some(tray)
    }

    fn data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            ..Default::default()
        };
        data.dwState = windows::Win32::UI::Shell::NOTIFY_ICON_STATE(0);
        data.dwStateMask = windows::Win32::UI::Shell::NOTIFY_ICON_STATE(0);
        data
    }

    pub fn set_tooltip(&self, tooltip: &str) {
        if !self.added {
            return;
        }
        let mut data = self.data();
        data.uFlags = NIF_TIP;
        data.hIcon = self.icon;
        copy_wide(&mut data.szTip, tooltip);
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    pub fn notify(&self, title: &str, text: &str, kind: u32) {
        if !self.added {
            return;
        }
        let mut data = self.data();
        data.uFlags = NIF_INFO;
        data.dwInfoFlags = match kind {
            0 => NIIF_ERROR,
            1 => NIIF_WARNING,
            _ => NIIF_INFO,
        };
        copy_wide(&mut data.szInfoTitle, title);
        copy_wide(&mut data.szInfo, &truncate(text, 250));
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
        }
    }

    pub fn remove(&mut self) {
        if !self.added {
            return;
        }
        let data = self.data();
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &data);
        }
        self.added = false;
    }
}

impl Drop for TrayIcon {
    fn drop(&mut self) {
        self.remove();
    }
}

fn copy_wide(destination: &mut [u16], text: &str) {
    if destination.is_empty() {
        return;
    }
    let encoded: Vec<u16> = text.encode_utf16().take(destination.len() - 1).collect();
    destination.fill(0);
    destination[..encoded.len()].copy_from_slice(&encoded);
}

fn truncate(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut result: String = text.chars().take(limit.saturating_sub(3)).collect();
    result.push_str("...");
    result
}

pub fn balloon_clicked(lparam: LPARAM) -> bool {
    (lparam.0 & 0xFFFF) as usize == crate::win::NIN_BALLOONUSERCLICK
}

pub fn tray_left_click(lparam: LPARAM) -> bool {
    let event = (lparam.0 & 0xFFFF) as usize;
    event == crate::win::NIN_SELECT || event == 0x0202
}

pub fn tray_right_click(lparam: LPARAM) -> bool {
    let event = (lparam.0 & 0xFFFF) as usize;
    event == 0x0205 || event == 0x0403
}

pub fn push_app_message(hwnd: HWND) {
    let _ = PCWSTR::null();
    win::post(hwnd, APP_MESSAGE, WPARAM(0), LPARAM(0));
}
