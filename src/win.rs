use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    GetDC, GetMonitorInfoW, InvalidateRect, MonitorFromPoint, ReleaseDC, BLENDFUNCTION, HBRUSH,
    HDC, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, AC_SRC_ALPHA, AC_SRC_OVER,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForSystem, GetDpiForWindow};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, ReleaseCapture, SetCapture, SetFocus, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DispatchMessageW, GetClientRect, GetCursorPos, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, KillTimer, LoadCursorW, MessageBoxW,
    MoveWindow, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW, SetCursor,
    SetForegroundWindow, SetMenuDefaultItem, SetTimer, SetWindowLongPtrW, SetWindowPos,
    SetWindowTextW, ShowWindow, TrackPopupMenu, TranslateMessage, UpdateLayeredWindow,
    HCURSOR, HICON, HWND_TOPMOST, MSG, ULW_ALPHA, WINDOW_EX_STYLE, WINDOW_STYLE,
    WNDCLASSW, WNDPROC, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};

pub use windows::Win32::UI::WindowsAndMessaging::{
    MB_DEFBUTTON2, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNO,
    MESSAGEBOX_RESULT,
};

pub const WM_APP: u32 = 0x8000;
pub const WM_DPICHANGED_MSG: u32 = 0x02E0;
pub const WM_MOUSEWHEEL_MSG: u32 = 0x020A;
pub const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
pub const WM_NCHITTEST_MSG: u32 = 0x0084;
pub const WM_SETCURSOR_MSG: u32 = 0x0020;
pub const WM_TIMER_MSG: u32 = 0x0113;
pub const WM_HOTKEY_MSG: u32 = 0x0312;
pub const WM_LBUTTONDOWN_MSG: u32 = 0x0201;
pub const WM_LBUTTONUP_MSG: u32 = 0x0202;
pub const WM_MOUSEMOVE_MSG: u32 = 0x0200;
pub const WM_RBUTTONUP_MSG: u32 = 0x0205;
pub const WM_CHAR_MSG: u32 = 0x0102;
pub const WM_CLOSE_MSG: u32 = 0x0010;
pub const WM_DESTROY_MSG: u32 = 0x0002;
pub const HTTRANSPARENT_VALUE: isize = -1;
pub const IDYES_VALUE: i32 = 6;
pub const IDNO_VALUE: i32 = 7;

pub const TIMER_CAPTURE: usize = 1;
pub const TIMER_PREVIEW: usize = 2;
pub const TIMER_STATUS: usize = 3;
pub const TIMER_CARET: usize = 4;

pub const NIN_SELECT: usize = 0x0400;
pub const NIN_BALLOONUSERCLICK: usize = 0x0405;

pub fn wide(text: &str) -> Vec<u16> {
    let mut buffer: Vec<u16> = text.encode_utf16().collect();
    buffer.push(0);
    buffer
}

pub fn pcwstr(buffer: &[u16]) -> PCWSTR {
    PCWSTR(buffer.as_ptr())
}

pub fn loword(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

pub fn get_x_lparam(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xFFFF) as u16 as i16 as i32
}

pub fn get_y_lparam(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32
}

pub fn wheel_delta(wparam: WPARAM) -> i16 {
    ((wparam.0 >> 16) & 0xFFFF) as u16 as i16
}

pub fn key_from_wparam(wparam: WPARAM) -> u32 {
    (wparam.0 & 0xFFFF) as u32
}

pub fn module_instance() -> windows::Win32::Foundation::HINSTANCE {
    unsafe {
        let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .unwrap_or_default();
        windows::Win32::Foundation::HINSTANCE(module.0)
    }
}

pub fn register_class(name: &str, proc: WNDPROC, cursor: Option<HCURSOR>) -> bool {
    unsafe {
        let class_name = wide(name);
        let class = WNDCLASSW {
            style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
            lpfnWndProc: proc,
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: module_instance(),
            hIcon: HICON::default(),
            hCursor: cursor.unwrap_or_default(),
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: pcwstr(&class_name),
        };
        RegisterClassW(&class) != 0
    }
}

#[allow(clippy::too_many_arguments)]
pub unsafe fn create_window(
    class_name: &str,
    title: &str,
    style: WINDOW_STYLE,
    ex_style: WINDOW_EX_STYLE,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    parent: HWND,
    param: Option<*const core::ffi::c_void>,
) -> HWND {
    let class = wide(class_name);
    let title_wide = wide(title);
    CreateWindowExW(
        ex_style,
        pcwstr(&class),
        pcwstr(&title_wide),
        style,
        x,
        y,
        width,
        height,
        parent,
        None,
        module_instance(),
        param.map(|value| value as *const _),
    )
    .unwrap_or_default()
}

pub fn run_message_loop() {
    unsafe {
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, HWND::default(), 0, 0).0;
            if result <= 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

pub fn quit(code: i32) {
    unsafe { PostQuitMessage(code) };
}

pub fn client_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetClientRect(hwnd, &mut rect);
    }
    rect
}

pub fn window_rect(hwnd: HWND) -> RECT {
    let mut rect = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rect);
    }
    rect
}

pub fn set_window_text(hwnd: HWND, text: &str) {
    let buffer = wide(text);
    unsafe {
        let _ = SetWindowTextW(hwnd, pcwstr(&buffer));
    }
}

pub fn invalidate(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

pub fn invalidate_rect(hwnd: HWND, rect: &RECT) {
    unsafe {
        let _ = InvalidateRect(hwnd, Some(rect), false);
    }
}

pub fn show(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_SHOW);
    }
}

pub fn show_without_activation(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
        );
    }
}

pub fn hide(hwnd: HWND) {
    unsafe {
        let _ = ShowWindow(hwnd, windows::Win32::UI::WindowsAndMessaging::SW_HIDE);
    }
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(hwnd).as_bool() }
}

pub fn activate(hwnd: HWND) {
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(hwnd);
    }
}

pub fn focus(hwnd: HWND) {
    unsafe {
        let _ = SetFocus(hwnd);
    }
}

pub fn set_topmost_bounds(hwnd: HWND, x: i32, y: i32, width: i32, height: i32, show_window: bool) {
    let mut flags = windows::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER
        | windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE;
    if show_window {
        flags |= windows::Win32::UI::WindowsAndMessaging::SWP_SHOWWINDOW;
    }
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, flags);
    }
}

pub fn move_window(hwnd: HWND, x: i32, y: i32, width: i32, height: i32) {
    unsafe {
        let _ = MoveWindow(hwnd, x, y, width, height, true);
    }
}

pub fn begin_paint(hwnd: HWND) -> (HDC, PAINTSTRUCT) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { windows::Win32::Graphics::Gdi::BeginPaint(hwnd, &mut paint) };
    (hdc, paint)
}

pub fn end_paint(hwnd: HWND, paint: &PAINTSTRUCT) {
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::EndPaint(hwnd, paint);
    }
}

pub fn set_user_data(hwnd: HWND, value: isize) {
    unsafe {
        SetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
            value,
        );
    }
}

pub fn user_data(hwnd: HWND) -> isize {
    unsafe {
        GetWindowLongPtrW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::GWLP_USERDATA,
        )
    }
}

pub fn set_icon(hwnd: HWND, large: HICON, small: HICON) {
    unsafe {
        let _ = SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_SETICON,
            WPARAM(windows::Win32::UI::WindowsAndMessaging::ICON_BIG as usize),
            LPARAM(large.0 as isize),
        );
        let _ = SendMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_SETICON,
            WPARAM(windows::Win32::UI::WindowsAndMessaging::ICON_SMALL as usize),
            LPARAM(small.0 as isize),
        );
    }
}

pub fn cursor_position() -> POINT {
    let mut point = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut point);
    }
    point
}

pub fn is_key_down(key: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(key.0 as i32) as u16 & 0x8000) != 0 }
}

pub fn key_is_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

pub fn capture_mouse(hwnd: HWND) {
    unsafe {
        let _ = SetCapture(hwnd);
    }
}

pub fn release_mouse() {
    unsafe {
        let _ = ReleaseCapture();
    }
}

pub fn set_cursor(cursor: HCURSOR) {
    unsafe {
        let _ = SetCursor(cursor);
    }
}

pub fn standard_cursor(id: PCWSTR) -> HCURSOR {
    unsafe { LoadCursorW(windows::Win32::Foundation::HINSTANCE::default(), id).unwrap_or_default() }
}

pub fn arrow_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_ARROW)
}
pub fn cross_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_CROSS)
}
pub fn hand_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_HAND)
}
pub fn ibeam_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_IBEAM)
}
pub fn size_all_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_SIZEALL)
}
pub fn size_ns_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_SIZENS)
}
pub fn size_we_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_SIZEWE)
}
pub fn size_nwse_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_SIZENWSE)
}
pub fn size_nesw_cursor() -> HCURSOR {
    standard_cursor(windows::Win32::UI::WindowsAndMessaging::IDC_SIZENESW)
}

pub fn dpi(hwnd: HWND) -> u32 {
    let value = unsafe { GetDpiForWindow(hwnd) };
    if value == 0 {
        system_dpi()
    } else {
        value
    }
}

pub fn system_dpi() -> u32 {
    let value = unsafe { GetDpiForSystem() };
    value.max(96)
}

pub fn scale(hwnd: HWND) -> f32 {
    dpi(hwnd) as f32 / 96.0
}

pub fn scaled(value: i32, scale: f32) -> i32 {
    (value as f32 * scale).round() as i32
}

pub struct MonitorInfo {
    pub monitor: RECT,
    pub work: RECT,
    pub scale: f32,
}

pub fn monitor_info(point: POINT) -> MonitorInfo {
    unsafe {
        let handle: HMONITOR = MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !handle.is_invalid() && GetMonitorInfoW(handle, &mut info).as_bool() {
            let mut dpi_x = 96u32;
            let mut dpi_y = 96u32;
            let scale_value =
                if GetDpiForMonitor(handle, windows::Win32::UI::HiDpi::MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y).is_ok()
                    && dpi_x >= 96
                {
                    dpi_x as f32 / 96.0
                } else {
                    system_dpi() as f32 / 96.0
                };
            MonitorInfo {
                monitor: info.rcMonitor,
                work: info.rcWork,
                scale: scale_value,
            }
        } else {
            let width = GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CXVIRTUALSCREEN,
            );
            let height = GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_CYVIRTUALSCREEN,
            );
            let x = GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
            );
            let y = GetSystemMetrics(
                windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
            );
            let rect = RECT {
                left: x,
                top: y,
                right: x + width,
                bottom: y + height,
            };
            MonitorInfo {
                monitor: rect,
                work: rect,
                scale: system_dpi() as f32 / 96.0,
            }
        }
    }
}

pub fn virtual_screen_rect() -> RECT {
    unsafe {
        let x = GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_XVIRTUALSCREEN,
        );
        let y = GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_YVIRTUALSCREEN,
        );
        let width = GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CXVIRTUALSCREEN,
        );
        let height = GetSystemMetrics(
            windows::Win32::UI::WindowsAndMessaging::SM_CYVIRTUALSCREEN,
        );
        RECT {
            left: x,
            top: y,
            right: x + width,
            bottom: y + height,
        }
    }
}

pub fn set_timer(hwnd: HWND, id: usize, milliseconds: u32) {
    unsafe {
        SetTimer(hwnd, id, milliseconds, None);
    }
}

pub fn kill_timer(hwnd: HWND, id: usize) {
    unsafe {
        let _ = KillTimer(hwnd, id);
    }
}

pub fn message_box(
    owner: HWND,
    text: &str,
    caption: &str,
    flags: windows::Win32::UI::WindowsAndMessaging::MESSAGEBOX_STYLE,
) -> MESSAGEBOX_RESULT {
    let text_wide = wide(text);
    let caption_wide = wide(caption);
    unsafe { MessageBoxW(owner, pcwstr(&text_wide), pcwstr(&caption_wide), flags) }
}
pub fn adjust_window_rect(rect: &mut RECT, style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE) {
    unsafe {
        let _ = AdjustWindowRectEx(rect, style, false, ex_style);
    }
}

pub fn center_on_cursor_monitor(hwnd: HWND, width: i32, height: i32) {
    let info = monitor_info(cursor_position());
    let x = info.work.left + ((info.work.right - info.work.left) - width) / 2;
    let y = info.work.top + ((info.work.bottom - info.work.top) - height) / 2;
    move_window(hwnd, x, y, width, height);
}

pub fn screen_dc() -> HDC {
    unsafe { GetDC(HWND::default()) }
}

pub fn release_screen_dc(hdc: HDC) {
    unsafe {
        let _ = ReleaseDC(HWND::default(), hdc);
    }
}

pub fn post(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) {
    unsafe {
        let _ = PostMessageW(hwnd, message, wparam, lparam);
    }
}

pub fn send(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe { SendMessageW(hwnd, message, wparam, lparam) }
}

pub fn last_error() -> u32 {
    unsafe { windows::Win32::Foundation::GetLastError().0 }
}

pub fn layered_update(
    hwnd: HWND,
    screen: HDC,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    source: HDC,
) {
    unsafe {
        let point = POINT { x, y };
        let size = windows::Win32::Foundation::SIZE {
            cx: width,
            cy: height,
        };
        let src = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd,
            screen,
            Some(&point),
            Some(&size),
            source,
            Some(&src),
            None,
            Some(&blend),
            ULW_ALPHA,
        );
    }
}

pub fn create_popup_menu() -> windows::Win32::UI::WindowsAndMessaging::HMENU {
    unsafe { CreatePopupMenu().unwrap_or_default() }
}

pub fn append_menu(menu: windows::Win32::UI::WindowsAndMessaging::HMENU, id: usize, label: &str) {
    let buffer = wide(label);
    unsafe {
        let _ = AppendMenuW(
            menu,
            windows::Win32::UI::WindowsAndMessaging::MF_STRING,
            id,
            pcwstr(&buffer),
        );
    }
}

pub fn set_menu_default(menu: windows::Win32::UI::WindowsAndMessaging::HMENU, id: usize) {
    unsafe {
        let _ = SetMenuDefaultItem(menu, id as u32, 0);
    }
}

pub fn show_popup_menu(
    menu: windows::Win32::UI::WindowsAndMessaging::HMENU,
    hwnd: HWND,
    x: i32,
    y: i32,
) -> usize {
    unsafe {
        let _ = SetForegroundWindow(hwnd);
        let command = TrackPopupMenu(
            menu,
            windows::Win32::UI::WindowsAndMessaging::TPM_RETURNCMD
                | windows::Win32::UI::WindowsAndMessaging::TPM_RIGHTBUTTON
                | windows::Win32::UI::WindowsAndMessaging::TPM_NONOTIFY,
            x,
            y,
            0,
            hwnd,
            None,
        );
        let _ = PostMessageW(hwnd, windows::Win32::UI::WindowsAndMessaging::WM_NULL, WPARAM(0), LPARAM(0));
        command.0 as usize
    }
}

pub fn destroy_menu(menu: windows::Win32::UI::WindowsAndMessaging::HMENU) {
    unsafe {
        let _ = DestroyMenu(menu);
    }
}

pub fn key_state(key: i32) -> i16 {
    unsafe { GetKeyState(key) }
}

pub fn key_is_toggled(key: i32) -> bool {
    (key_state(key) as u16 & 0x0001) != 0
}

pub fn modifiers_from_keyboard() -> u32 {
    let mut value = 0u32;
    if (key_state(0x11) as u16 & 0x8000) != 0 {
        value |= crate::hotkeys::MOD_CONTROL_VALUE;
    }
    if (key_state(0x12) as u16 & 0x8000) != 0 {
        value |= crate::hotkeys::MOD_ALT_VALUE;
    }
    if (key_state(0x10) as u16 & 0x8000) != 0 {
        value |= crate::hotkeys::MOD_SHIFT_VALUE;
    }
    if (key_state(0x5B) as u16 & 0x8000) != 0 || (key_state(0x5C) as u16 & 0x8000) != 0 {
        value |= crate::hotkeys::MOD_WIN_VALUE;
    }
    value
}

pub fn cursor_is_down() -> bool {
    (key_state(0x01) as u16 & 0x8000) != 0
}

pub fn screen_to_client(hwnd: HWND, point: POINT) -> POINT {
    unsafe {
        let mut result = point;
        let _ = windows::Win32::Graphics::Gdi::ScreenToClient(hwnd, &mut result);
        result
    }
}

pub fn client_to_screen(hwnd: HWND, point: POINT) -> POINT {
    unsafe {
        let mut result = point;
        let _ = windows::Win32::Graphics::Gdi::ClientToScreen(hwnd, &mut result);
        result
    }
}

pub fn enable_window(hwnd: HWND, enabled: bool) {
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow(hwnd, enabled);
    }
}

pub fn destroy_window(hwnd: HWND) {
    if !hwnd.is_invalid() {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
        }
    }
}

pub fn set_capture(hwnd: HWND) {
    unsafe {
        let _ = SetCapture(hwnd);
    }
}

pub fn release_capture() {
    unsafe {
        let _ = ReleaseCapture();
    }
}

pub fn get_foreground() -> HWND {
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }
}

pub fn wide_to_string(value: &[u16]) -> String {
    let end = value.iter().position(|unit| *unit == 0).unwrap_or(value.len());
    String::from_utf16_lossy(&value[..end])
}

pub fn pwstr_to_string(value: PWSTR) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe {
        let mut length = 0usize;
        while *value.0.add(length) != 0 {
            length += 1;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            value.0, length,
        )))
    }
}

pub fn destroy_hicon(icon: HICON) {
    if !icon.is_invalid() {
        unsafe {
            let _ = DestroyIcon(icon);
        }
    }
}
