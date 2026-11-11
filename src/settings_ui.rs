use std::path::Path;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_LWIN,
    VK_RIGHT, VK_RWIN, VK_SHIFT, VK_SPACE, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, MINMAXINFO, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_DPICHANGED,
    WM_GETMINMAXINFO, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_MOUSEWHEEL,
    WM_SETCURSOR, WM_SYSKEYDOWN, WM_TIMER, WS_CAPTION, WS_SYSMENU,
};

use crate::gdiplus::{self, BitmapSurface};
use crate::hotkeys::{self, Gesture};
use crate::i18n;
use crate::settings::{self, AppSettings};
use crate::textedit::TextEdit;
use crate::win::{self, TIMER_CARET, WM_APP};

pub const SETTINGS_CLOSED: u32 = WM_APP + 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    None,
    Tab(usize),
    LanguageCombo,
    FormatCombo,
    LanguageItem(usize),
    FormatItem(usize),
    Notify,
    Folder,
    Browse,
    Startup,
    Hotkey(usize),
    Slider,
    Cancel,
    Save,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    None,
    Language,
    Notify,
    Folder,
    Browse,
    Startup,
    Hotkey(usize),
    Format,
    Slider,
    Cancel,
    Save,
}

#[derive(Clone)]
struct Layout {
    scale: f32,
    width: i32,
    height: i32,
    content_left: i32,
    content_right: i32,
    title_y: i32,
    title_line: i32,
    tagline_y: i32,
    tagline_line: i32,
    separator_y: i32,
    tabs_top: i32,
    tabs_height: i32,
    tab_rects: [RECT; 3],
    content_top: i32,
    label_line: i32,
    lang_label_y: i32,
    lang_combo: RECT,
    notify_y: i32,
    notify_h: i32,
    folder_label_y: i32,
    folder_box: RECT,
    browse: RECT,
    startup_y: i32,
    hotkey_label_y: [i32; 3],
    hotkey_box: [RECT; 3],
    info_rect: RECT,
    info_hint_line: i32,
    info_info_line: i32,
    format_label_y: i32,
    format_combo: RECT,
    quality_label_y: i32,
    slider: RECT,
    quality_value: RECT,
    quality_hint_y: i32,
    cancel: RECT,
    save: RECT,
    dropdown: Option<RECT>,
    drop_items: [RECT; 2],
}

pub struct SettingsWindow {
    pub hwnd: HWND,
    owner: HWND,
    app: *mut crate::app::App,
    working: AppSettings,
    gestures: [Option<Gesture>; 3],
    active_tab: usize,
    focus: Focus,
    capturing: Option<usize>,
    hotkey_message: Option<String>,
    hotkey_prompt: bool,
    language_open: bool,
    format_open: bool,
    hover: Target,
    pressed: Target,
    slider_drag: bool,
    folder: TextEdit,
    caret_visible: bool,
    backbuffer: Option<BitmapSurface>,
    layout: Option<Layout>,
    tracking_mouse: bool,
}

impl SettingsWindow {
    pub fn create(
        owner: HWND,
        app: *mut crate::app::App,
        current: AppSettings,
    ) -> Option<Box<Self>> {
        let class_name = "LshotSettingsWindow";
        let class = win::wide(class_name);
        unsafe {
            let class_struct = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(settings_proc),
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
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: (620.0 * scale).round() as i32,
            bottom: (600.0 * scale).round() as i32,
        };
        win::adjust_window_rect(
            &mut rect,
            WS_CAPTION | WS_SYSMENU,
            windows::Win32::UI::WindowsAndMessaging::WINDOW_EX_STYLE(0),
        );
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        let gestures = [
            hotkeys::parse(&current.region_hotkey)
                .or_else(|| hotkeys::parse(hotkeys::DEFAULT_REGION)),
            hotkeys::parse(&current.fullscreen_hotkey)
                .or_else(|| hotkeys::parse(hotkeys::DEFAULT_FULLSCREEN)),
            hotkeys::parse(&current.clipboard_hotkey)
                .or_else(|| hotkeys::parse(hotkeys::DEFAULT_CLIPBOARD)),
        ];

        let folder = TextEdit::new(&current.save_folder);
        let mut state = Box::new(Self {
            hwnd: HWND::default(),
            owner,
            app,
            working: current,
            gestures,
            active_tab: 0,
            focus: Focus::None,
            capturing: None,
            hotkey_message: None,
            hotkey_prompt: false,
            language_open: false,
            format_open: false,
            hover: Target::None,
            pressed: Target::None,
            slider_drag: false,
            folder,
            caret_visible: true,
            backbuffer: None,
            layout: None,
            tracking_mouse: false,
        });

        let hwnd = unsafe {
            win::create_window(
                class_name,
                i18n::t("SettingsTitle"),
                WS_CAPTION | WS_SYSMENU,
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
        win::set_window_text(hwnd, i18n::t("SettingsTitle"));
        win::center_on_cursor_monitor(hwnd, width, height);
        let large_icon = crate::icon::load_window_icon();
        let small_icon = crate::icon::load_application_icon();
        if !large_icon.is_invalid() && !small_icon.is_invalid() {
            win::set_icon(hwnd, large_icon, small_icon);
        }
        Some(state)
    }

    pub fn relocalize(&mut self) {
        win::set_window_text(self.hwnd, i18n::t("SettingsTitle"));
        self.layout = None;
        win::invalidate(self.hwnd);
    }

    pub fn focus_window(&self) {
        win::activate(self.hwnd);
    }

    fn display_hotkey(&self, index: usize) -> String {
        if self.capturing == Some(index) && self.hotkey_prompt {
            return i18n::t("HotkeyPressPrompt").to_string();
        }
        if self.capturing == Some(index) {
            if let Some(message) = &self.hotkey_message {
                return message.clone();
            }
        }
        self.gestures[index]
            .as_ref()
            .map(|gesture| gesture.display.clone())
            .unwrap_or_default()
    }

    fn value_for_combo(&self, combo: usize) -> usize {
        if combo == 0 {
            if self.working.language.eq_ignore_ascii_case("en") {
                1
            } else {
                0
            }
        } else if self.working.image_format.eq_ignore_ascii_case("jpeg") {
            1
        } else {
            0
        }
    }

    fn combo_item_label(&self, combo: usize, index: usize) -> &'static str {
        if combo == 0 {
            if index == 1 {
                i18n::t("LanguageEnglish")
            } else {
                i18n::t("LanguageRussian")
            }
        } else if index == 1 {
            "JPEG"
        } else {
            "PNG"
        }
    }

    fn set_language(&mut self, index: usize) {
        let language = if index == 1 { "en" } else { "ru" };
        self.working.language = language.to_string();
        i18n::set_language(language);
        self.relocalize();
        unsafe {
            if !self.app.is_null() {
                (*self.app).preview_language(language);
            }
        }
    }

    fn set_format(&mut self, index: usize) {
        self.working.image_format = if index == 1 { "jpeg" } else { "png" }.to_string();
    }

    fn layout(&self, g: &gdiplus::Gfx) -> Layout {
        let scale = win::scale(self.hwnd);
        let client = win::client_rect(self.hwnd);
        let width = (client.right - client.left).max(1);
        let height = (client.bottom - client.top).max(1);
        let content_left = (28.0 * scale).round() as i32;
        let content_right = width - content_left;

        let (_, title_line) = g.measure_text("Lshot", 28.0 * scale, true, 10000.0, false);
        let title_y = (22.0 * scale).round() as i32;
        let tagline_text = i18n::t("AppTagline");
        let tagline_max = (540.0 * scale).min((content_right - content_left) as f32);
        let (_, tagline_line) =
            g.measure_text(tagline_text, 12.5 * scale, false, tagline_max, true);
        let tagline_y = title_y + title_line.round() as i32 + (5.0 * scale).round() as i32;
        let separator_y =
            tagline_y + tagline_line.round() as i32 + (18.0 * scale).round() as i32;
        let tabs_top = separator_y + 1 + (16.0 * scale).round() as i32;

        let (_, label_line) = g.measure_text("Ag", 13.0 * scale, false, 10000.0, false);
        let tabs_height = label_line.round() as i32 + (20.0 * scale).round() as i32;
        let tab_texts = [
            i18n::t("TabGeneral"),
            i18n::t("TabHotkeys"),
            i18n::t("TabFormat"),
        ];
        let mut tab_rects = [RECT::default(); 3];
        let mut tab_x = content_left;
        for (index, text) in tab_texts.iter().enumerate() {
            let (text_width, _) = g.measure_text(text, 13.0 * scale, false, 10000.0, false);
            let tab_width =
                text_width.round() as i32 + (28.0 * scale).round() as i32;
            tab_rects[index] = RECT {
                left: tab_x,
                top: tabs_top,
                right: tab_x + tab_width,
                bottom: tabs_top + tabs_height,
            };
            tab_x += tab_width;
        }

        let content_top = tabs_top + tabs_height + 1 + (18.0 * scale).round() as i32;
        let s = |value: f32| (value * scale).round() as i32;

        let lang_label_y = content_top;
        let lang_combo = RECT {
            left: content_left,
            top: lang_label_y + label_line.round() as i32 + s(7.0),
            right: content_left + s(220.0),
            bottom: lang_label_y + label_line.round() as i32 + s(7.0) + s(36.0),
        };
        let notify_y = lang_combo.bottom + s(10.0);
        let notify_h = s(20.0);
        let folder_label_y = notify_y + notify_h + s(18.0);
        let folder_top = folder_label_y + label_line.round() as i32 + s(7.0);
        let (browse_width, _) = g.measure_text(i18n::t("Browse"), 13.0 * scale, false, 10000.0, false);
        let browse_width = browse_width.round() as i32 + s(32.0);
        let browse = RECT {
            left: content_right - browse_width,
            top: folder_top,
            right: content_right,
            bottom: folder_top + s(35.0),
        };
        let folder_box = RECT {
            left: content_left,
            top: folder_top,
            right: browse.left - s(8.0),
            bottom: folder_top + s(35.0),
        };
        let startup_y = folder_box.bottom + s(16.0);

        let mut hotkey_label_y = [0i32; 3];
        let mut hotkey_box = [RECT::default(); 3];
        let mut hotkey_y = content_top;
        for index in 0..3 {
            hotkey_label_y[index] = hotkey_y;
            let box_top = hotkey_y + label_line.round() as i32 + s(7.0);
            hotkey_box[index] = RECT {
                left: content_left,
                top: box_top,
                right: content_left + s(280.0),
                bottom: box_top + s(36.0),
            };
            hotkey_y = hotkey_box[index].bottom + s(20.0);
        }

        let hint_text = i18n::t("HotkeysHint");
        let info_text = i18n::t("HotkeysInfo");
        let info_width = content_right - content_left;
        let inner_info_width = info_width as f32 - 28.0 * scale;
        let (_, info_hint_line) =
            g.measure_text(hint_text, 12.0 * scale, false, inner_info_width, true);
        let (_, info_info_line) =
            g.measure_text(info_text, 12.0 * scale, false, inner_info_width, true);
        let info_top = hotkey_box[2].bottom + s(22.0);
        let info_height = s(12.0)
            + info_hint_line.round() as i32
            + s(7.0)
            + info_info_line.round() as i32
            + s(12.0);
        let info_rect = RECT {
            left: content_left,
            top: info_top,
            right: content_right,
            bottom: info_top + info_height,
        };

        let format_label_y = content_top;
        let format_combo = RECT {
            left: content_left,
            top: format_label_y + label_line.round() as i32 + s(7.0),
            right: content_left + s(220.0),
            bottom: format_label_y + label_line.round() as i32 + s(7.0) + s(36.0),
        };
        let quality_label_y = format_combo.bottom + s(22.0);
        let slider_row_top = quality_label_y + label_line.round() as i32 + s(3.0);
        let value_width = s(54.0);
        let slider = RECT {
            left: content_left,
            top: slider_row_top,
            right: content_right - value_width,
            bottom: slider_row_top + s(22.0),
        };
        let quality_value = RECT {
            left: content_right - value_width,
            top: slider_row_top,
            right: content_right,
            bottom: slider_row_top + s(22.0),
        };
        let quality_hint_y = slider_row_top + s(22.0) + s(9.0);

        let button_h = label_line.round() as i32 + s(16.0);
        let save = RECT {
            left: content_right - s(118.0),
            top: height - s(20.0) - button_h,
            right: content_right,
            bottom: height - s(20.0),
        };
        let cancel = RECT {
            left: save.left - s(10.0) - s(102.0),
            top: save.top,
            right: save.left - s(10.0),
            bottom: save.bottom,
        };

        let combo = if self.language_open {
            Some(0)
        } else if self.format_open {
            Some(1)
        } else {
            None
        };
        let mut dropdown = None;
        let mut drop_items = [RECT::default(); 2];
        if let Some(combo_index) = combo {
            let base = if combo_index == 0 {
                lang_combo
            } else {
                format_combo
            };
            let rect = RECT {
                left: base.left,
                top: base.bottom + s(4.0),
                right: base.right,
                bottom: base.bottom + s(4.0) + s(4.0) + s(60.0),
            };
            dropdown = Some(rect);
            for index in 0..2 {
                drop_items[index] = RECT {
                    left: rect.left + s(2.0),
                    top: rect.top + s(2.0) + index as i32 * s(30.0),
                    right: rect.right - s(2.0),
                    bottom: rect.top + s(2.0) + (index as i32 + 1) * s(30.0),
                };
            }
        }

        Layout {
            scale,
            width,
            height,
            content_left,
            content_right,
            title_y,
            title_line: title_line.round() as i32,
            tagline_y,
            tagline_line: tagline_line.round() as i32,
            separator_y,
            tabs_top,
            tabs_height,
            tab_rects,
            content_top,
            label_line: label_line.round() as i32,
            lang_label_y,
            lang_combo,
            notify_y,
            notify_h,
            folder_label_y,
            folder_box,
            browse,
            startup_y,
            hotkey_label_y,
            hotkey_box,
            info_rect,
            info_hint_line: info_hint_line.round() as i32,
            info_info_line: info_info_line.round() as i32,
            format_label_y,
            format_combo,
            quality_label_y,
            slider,
            quality_value,
            quality_hint_y,
            cancel,
            save,
            dropdown,
            drop_items,
        }
    }

    fn hit_test(&self, layout: &Layout, x: i32, y: i32) -> Target {
        let point = POINT { x, y };
        if let Some(_) = layout.dropdown {
            for index in 0..2 {
                if point_in(&layout.drop_items[index], point) {
                    return if self.language_open {
                        Target::LanguageItem(index)
                    } else {
                        Target::FormatItem(index)
                    };
                }
            }
        }
        for (index, rect) in layout.tab_rects.iter().enumerate() {
            if point_in(rect, point) {
                return Target::Tab(index);
            }
        }
        if point_in(&layout.cancel, point) {
            return Target::Cancel;
        }
        if point_in(&layout.save, point) {
            return Target::Save;
        }
        match self.active_tab {
            0 => {
                if point_in(&layout.lang_combo, point) {
                    return Target::LanguageCombo;
                }
                let notify_rect = RECT {
                    left: layout.content_left,
                    top: layout.notify_y,
                    right: layout.content_right,
                    bottom: layout.notify_y + layout.notify_h,
                };
                if point_in(&notify_rect, point) {
                    return Target::Notify;
                }
                if point_in(&layout.folder_box, point) {
                    return Target::Folder;
                }
                if point_in(&layout.browse, point) {
                    return Target::Browse;
                }
                let startup_rect = RECT {
                    left: layout.content_left,
                    top: layout.startup_y,
                    right: layout.content_right,
                    bottom: layout.startup_y + layout.notify_h,
                };
                if point_in(&startup_rect, point) {
                    return Target::Startup;
                }
            }
            1 => {
                for index in 0..3 {
                    if point_in(&layout.hotkey_box[index], point) {
                        return Target::Hotkey(index);
                    }
                }
            }
            2 => {
                if point_in(&layout.format_combo, point) {
                    return Target::FormatCombo;
                }
                if point_in(&layout.slider, point) {
                    return Target::Slider;
                }
                let value_rect = layout.quality_value;
                if point_in(&value_rect, point) {
                    return Target::Slider;
                }
            }
            _ => {}
        }
        Target::None
    }

    fn set_focus(&mut self, focus: Focus) {
        if self.focus == focus {
            if let Focus::Hotkey(index) = focus {
                self.begin_hotkey_capture(index);
            }
            return;
        }
        if matches!(self.focus, Focus::Hotkey(_)) {
            self.capturing = None;
            self.hotkey_message = None;
            self.hotkey_prompt = false;
        }
        if self.focus == Focus::Folder {
            win::kill_timer(self.hwnd, TIMER_CARET);
        }
        self.focus = focus;
        if let Focus::Hotkey(index) = focus {
            self.begin_hotkey_capture(index);
        }
        if focus == Focus::Folder {
            self.caret_visible = true;
            win::set_timer(self.hwnd, TIMER_CARET, 530);
        }
        win::invalidate(self.hwnd);
    }

    fn begin_hotkey_capture(&mut self, index: usize) {
        self.capturing = Some(index);
        self.hotkey_prompt = true;
        self.hotkey_message = None;
        self.caret_visible = false;
        win::invalidate(self.hwnd);
    }

    fn end_hotkey_capture(&mut self) {
        self.capturing = None;
        self.hotkey_message = None;
        self.hotkey_prompt = false;
        win::invalidate(self.hwnd);
    }

    fn focus_order(&self) -> Vec<Focus> {
        match self.active_tab {
            0 => vec![
                Focus::Language,
                Focus::Notify,
                Focus::Folder,
                Focus::Browse,
                Focus::Startup,
                Focus::Cancel,
                Focus::Save,
            ],
            1 => vec![
                Focus::Hotkey(0),
                Focus::Hotkey(1),
                Focus::Hotkey(2),
                Focus::Cancel,
                Focus::Save,
            ],
            2 => vec![Focus::Format, Focus::Slider, Focus::Cancel, Focus::Save],
            _ => vec![Focus::Save],
        }
    }

    fn move_focus(&mut self, reverse: bool) {
        let order = self.focus_order();
        let current = order.iter().position(|value| *value == self.focus);
        let next = match current {
            Some(index) => {
                if reverse {
                    if index == 0 {
                        order.len() - 1
                    } else {
                        index - 1
                    }
                } else {
                    (index + 1) % order.len()
                }
            }
            None => {
                if reverse {
                    order.len() - 1
                } else {
                    0
                }
            }
        };
        self.set_focus(order[next]);
    }

    fn toggle_checkbox(&mut self, target: Target) {
        if target == Target::Notify {
            self.working.show_save_notification = !self.working.show_save_notification;
        } else if target == Target::Startup {
            self.working.run_at_startup = !self.working.run_at_startup;
        }
        win::invalidate(self.hwnd);
    }

    fn set_slider_value(&mut self, x: i32, layout: &Layout) {
        let width = (layout.slider.right - layout.slider.left).max(1);
        let ratio = ((x - layout.slider.left) as f32 / width as f32).clamp(0.0, 1.0);
        self.working.quality = (1.0 + ratio * 99.0).round().clamp(1.0, 100.0) as i32;
        win::invalidate(self.hwnd);
    }

    fn mouse_down(&mut self, x: i32, y: i32) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        if let Some(dropdown) = layout.dropdown {
            let hit = self.hit_test(&layout, x, y);
            match hit {
                Target::LanguageItem(index) => {
                    self.set_language(index);
                    self.language_open = false;
                    win::invalidate(self.hwnd);
                    return;
                }
                Target::FormatItem(index) => {
                    self.set_format(index);
                    self.format_open = false;
                    win::invalidate(self.hwnd);
                    return;
                }
                _ => {}
            }
            if point_in(&dropdown, POINT { x, y }) {
                return;
            }
            if self.language_open && point_in(&layout.lang_combo, POINT { x, y }) {
                self.language_open = false;
                win::invalidate(self.hwnd);
                return;
            }
            if self.format_open && point_in(&layout.format_combo, POINT { x, y }) {
                self.format_open = false;
                win::invalidate(self.hwnd);
                return;
            }
            self.language_open = false;
            self.format_open = false;
        }

        let hit = self.hit_test(&layout, x, y);
        self.pressed = Target::None;
        match hit {
            Target::Tab(index) => {
                self.active_tab = index;
                self.set_focus(Focus::None);
            }
            Target::LanguageCombo => {
                self.set_focus(Focus::Language);
                self.language_open = !self.language_open;
                self.format_open = false;
                win::invalidate(self.hwnd);
            }
            Target::FormatCombo => {
                self.set_focus(Focus::Format);
                self.format_open = !self.format_open;
                self.language_open = false;
                win::invalidate(self.hwnd);
            }
            Target::Notify => {
                self.set_focus(Focus::Notify);
                self.toggle_checkbox(Target::Notify);
            }
            Target::Startup => {
                self.set_focus(Focus::Startup);
                self.toggle_checkbox(Target::Startup);
            }
            Target::Folder => {
                self.set_focus(Focus::Folder);
                let inner_x = x - layout.folder_box.left - (9.0 * layout.scale).round() as i32;
                let font_size = 13.0 * layout.scale;
                if let Some(surface) = BitmapSurface::new(4, 4) {
                    if let Some(g) = surface.graphics() {
                        self.folder
                            .position_from_x(&g, inner_x as f32 + self.folder.scroll, font_size);
                    }
                }
                self.caret_visible = true;
                win::set_timer(self.hwnd, TIMER_CARET, 530);
                win::invalidate(self.hwnd);
            }
            Target::Hotkey(index) => {
                self.set_focus(Focus::Hotkey(index));
            }
            Target::Slider => {
                self.set_focus(Focus::Slider);
                self.slider_drag = true;
                self.set_slider_value(x, &layout);
                win::capture_mouse(self.hwnd);
            }
            Target::Browse => {
                self.set_focus(Focus::Browse);
                self.pressed = Target::Browse;
            }
            Target::Cancel => {
                self.set_focus(Focus::Cancel);
                self.pressed = Target::Cancel;
            }
            Target::Save => {
                self.set_focus(Focus::Save);
                self.pressed = Target::Save;
            }
            _ => {
                let _ = win::focus(self.hwnd);
            }
        }
        win::invalidate(self.hwnd);
    }

    fn mouse_up(&mut self, x: i32, y: i32) {
        if self.slider_drag {
            self.slider_drag = false;
            win::release_mouse();
            return;
        }
        let Some(layout) = self.layout.clone() else {
            return;
        };
        let hit = self.hit_test(&layout, x, y);
        let pressed = self.pressed;
        self.pressed = Target::None;
        if hit != pressed {
            return;
        }
        match hit {
            Target::Browse => self.browse(),
            Target::Cancel => win::destroy_window(self.hwnd),
            Target::Save => self.save(),
            _ => {}
        }
    }

    fn mouse_move(&mut self, x: i32, y: i32) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        if self.slider_drag {
            self.set_slider_value(x, &layout);
            return;
        }
        let hit = self.hit_test(&layout, x, y);
        if hit != self.hover {
            self.hover = hit;
            let cursor = match hit {
                Target::None => win::arrow_cursor(),
                Target::Folder => win::ibeam_cursor(),
                Target::Slider => win::arrow_cursor(),
                _ => win::hand_cursor(),
            };
            win::set_cursor(cursor);
            win::invalidate(self.hwnd);
        }
    }

    fn browse(&mut self) {
        let current = self.folder.text.trim().to_string();
        let current_path = Path::new(&current);
        let initial = if current_path.is_dir() {
            Some(current_path)
        } else {
            None
        };
        if let Some(folder) = crate::dialogs::pick_folder(
            self.hwnd,
            i18n::t("FolderBrowseDescription"),
            initial,
        ) {
            self.folder = TextEdit::new(&folder.to_string_lossy());
            self.set_focus(Focus::Folder);
        } else {
            win::activate(self.hwnd);
        }
        win::invalidate(self.hwnd);
    }

    fn save(&mut self) {
        let folder_text = self.folder.text.trim().trim_matches('"').to_string();
        if folder_text.trim().is_empty() {
            self.show_warning(i18n::t("FolderRequired"));
            self.set_focus(Focus::Folder);
            return;
        }
        let folder = settings::expand_environment_variables(&folder_text);
        if let Err(error) = std::fs::create_dir_all(&folder) {
            self.show_warning(&i18n::t1("FolderCreateError", &error.to_string()));
            return;
        }
        if self.gestures.iter().any(|gesture| gesture.is_none()) {
            self.show_warning(i18n::t("HotkeyInvalid"));
            return;
        }
        let first = self.gestures[0].as_ref().unwrap();
        let second = self.gestures[1].as_ref().unwrap();
        let third = self.gestures[2].as_ref().unwrap();
        if (first.modifiers == second.modifiers && first.vk == second.vk)
            || (first.modifiers == third.modifiers && first.vk == third.vk)
            || (second.modifiers == third.modifiers && second.vk == third.vk)
        {
            self.show_warning(i18n::t("HotkeyDuplicate"));
            return;
        }

        let mut candidate = self.working.clone();
        candidate.save_folder = folder;
        candidate.show_save_notification = self.working.show_save_notification;
        candidate.run_at_startup = self.working.run_at_startup;
        candidate.region_hotkey = first.display.clone();
        candidate.fullscreen_hotkey = second.display.clone();
        candidate.clipboard_hotkey = third.display.clone();
        candidate.image_format = self.working.image_format.clone();
        candidate.quality = self.working.quality.clamp(1, 100);
        candidate.language = self.working.language.clone();
        candidate.normalize();

        let result = unsafe {
            if self.app.is_null() {
                Err("The application state is unavailable.".to_string())
            } else {
                (*self.app).apply_settings(candidate)
            }
        };
        match result {
            Ok(()) => {
                win::destroy_window(self.hwnd);
            }
            Err(error) => {
                self.show_warning(&error);
            }
        }
    }

    fn show_warning(&self, message: &str) {
        crate::dialogs::show_warning(self.hwnd, message);
    }

    fn handle_key(&mut self, vk: u32, system: bool) {
        let control = win::key_is_down(VK_CONTROL.0 as u32);
        let shift = win::key_is_down(VK_SHIFT.0 as u32);
        let is_modifier = matches!(
            vk,
            0x10 | 0x11 | 0x12 | 0x5B | 0x5C | 0xA0 | 0xA1 | 0xA2 | 0xA3 | 0xA4 | 0xA5
        );

        if self.focus == Focus::Folder {
            match vk {
                v if v == VK_BACK.0 as u32 => {
                    self.folder.backspace();
                }
                v if v == VK_DELETE.0 as u32 => {
                    self.folder.delete();
                }
                v if v == VK_LEFT.0 as u32 => {
                    self.folder.move_left(shift);
                }
                v if v == VK_RIGHT.0 as u32 => {
                    self.folder.move_right(shift);
                }
                v if v == VK_HOME.0 as u32 => {
                    self.folder.home(shift);
                }
                v if v == VK_END.0 as u32 => {
                    self.folder.end(shift);
                }
                v if v == VK_TAB.0 as u32 => {
                    self.move_focus(shift);
                    return;
                }
                v if v == VK_ESCAPE.0 as u32 => {
                    win::destroy_window(self.hwnd);
                    return;
                }
                _ => {
                    if control {
                        match vk {
                            0x41 => self.folder.select_all(),
                            0x43 => {
                                self.folder.copy(self.hwnd);
                            }
                            0x58 => {
                                self.folder.cut(self.hwnd);
                            }
                            0x56 => {
                                self.folder.paste(self.hwnd);
                            }
                            _ => {}
                        }
                    } else if vk == 0x0D {
                        self.save();
                        return;
                    }
                }
            }
            self.ensure_folder_visible();
            self.caret_visible = true;
            win::set_timer(self.hwnd, TIMER_CARET, 530);
            win::invalidate(self.hwnd);
            return;
        }

        if let Some(index) = self.capturing {
            match vk {
                v if v == VK_TAB.0 as u32 => {
                    self.end_hotkey_capture();
                    self.move_focus(shift);
                    return;
                }
                v if v == VK_ESCAPE.0 as u32 || v == 0x0D => {
                    self.end_hotkey_capture();
                    return;
                }
                v if v == VK_BACK.0 as u32 || v == VK_DELETE.0 as u32 => {
                    let fallback = match index {
                        1 => hotkeys::DEFAULT_FULLSCREEN,
                        2 => hotkeys::DEFAULT_CLIPBOARD,
                        _ => hotkeys::DEFAULT_REGION,
                    };
                    self.gestures[index] = hotkeys::parse(fallback);
                    self.hotkey_message = None;
                    self.hotkey_prompt = false;
                    win::invalidate(self.hwnd);
                    return;
                }
                _ => {}
            }
            if is_modifier {
                return;
            }
            let modifiers = win::modifiers_from_keyboard();
            match hotkeys::from_key(vk, modifiers) {
                Some(gesture) => {
                    self.gestures[index] = Some(gesture);
                    self.hotkey_message = None;
                    self.hotkey_prompt = false;
                }
                None => {
                    self.hotkey_message = Some(
                        if modifiers == 0 {
                            i18n::t("HotkeyModifierRequired")
                        } else {
                            i18n::t("HotkeyInvalid")
                        }
                        .to_string(),
                    );
                    self.hotkey_prompt = false;
                }
            }
            win::invalidate(self.hwnd);
            return;
        }

        match vk {
            v if v == VK_TAB.0 as u32 => self.move_focus(shift),
            v if v == VK_ESCAPE.0 as u32 => {
                if self.language_open || self.format_open {
                    self.language_open = false;
                    self.format_open = false;
                    win::invalidate(self.hwnd);
                } else {
                    win::destroy_window(self.hwnd);
                }
            }
            v if v == VK_SPACE.0 as u32 || v == 0x0D => match self.focus {
                Focus::Notify => self.toggle_checkbox(Target::Notify),
                Focus::Startup => self.toggle_checkbox(Target::Startup),
                Focus::Language => {
                    self.language_open = !self.language_open;
                    self.format_open = false;
                    win::invalidate(self.hwnd);
                }
                Focus::Format => {
                    self.format_open = !self.format_open;
                    self.language_open = false;
                    win::invalidate(self.hwnd);
                }
                Focus::Browse => self.browse(),
                Focus::Cancel => win::destroy_window(self.hwnd),
                Focus::Save => self.save(),
                _ => {}
            },
            v if v == VK_UP.0 as u32 => match self.focus {
                Focus::Language => {
                    self.set_language(1 - self.value_for_combo(0));
                }
                Focus::Format => {
                    self.set_format(1 - self.value_for_combo(1));
                }
                Focus::Slider => {
                    self.working.quality = (self.working.quality + 1).clamp(1, 100);
                    win::invalidate(self.hwnd);
                }
                _ => {}
            },
            v if v == VK_DOWN.0 as u32 => match self.focus {
                Focus::Language => {
                    self.set_language(1 - self.value_for_combo(0));
                }
                Focus::Format => {
                    self.set_format(1 - self.value_for_combo(1));
                }
                Focus::Slider => {
                    self.working.quality = (self.working.quality - 1).clamp(1, 100);
                    win::invalidate(self.hwnd);
                }
                _ => {}
            },
            v if v == VK_LEFT.0 as u32 => {
                if self.focus == Focus::Slider {
                    self.working.quality = (self.working.quality - 1).clamp(1, 100);
                    win::invalidate(self.hwnd);
                }
            }
            v if v == VK_RIGHT.0 as u32 => {
                if self.focus == Focus::Slider {
                    self.working.quality = (self.working.quality + 1).clamp(1, 100);
                    win::invalidate(self.hwnd);
                }
            }
            _ => {
                let _ = system;
            }
        }
    }

    fn ensure_folder_visible(&mut self) {
        let Some(layout) = self.layout.clone() else {
            return;
        };
        if let Some(surface) = BitmapSurface::new(4, 4) {
            if let Some(g) = surface.graphics() {
                let font_size = 13.0 * layout.scale;
                self.folder.ensure_visible(
                    &g,
                    font_size,
                    (layout.folder_box.right - layout.folder_box.left) as f32 - 18.0 * layout.scale,
                );
            }
        }
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
        self.layout = Some(layout.clone());
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;

        g.text(
            "Lshot",
            layout.content_left as f32,
            layout.title_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.title_line as f32 + 2.0,
            28.0 * scale,
            true,
            gdiplus::rgb(0xF2, 0xF3, 0xF5),
        );
        g.text_styled(
            i18n::t("AppTagline"),
            layout.content_left as f32,
            layout.tagline_y as f32,
            540.0 * scale,
            layout.tagline_line as f32 + 4.0,
            12.5 * scale,
            false,
            gdiplus::rgb(0x9C, 0xA3, 0xAF),
            0,
            0,
            true,
            false,
        );
        g.fill_rect(
            gdiplus::rgb(0x2C, 0x2F, 0x36),
            layout.content_left,
            layout.separator_y,
            layout.content_right - layout.content_left,
            1,
        );

        let tab_texts = [
            i18n::t("TabGeneral"),
            i18n::t("TabHotkeys"),
            i18n::t("TabFormat"),
        ];
        for (index, rect) in layout.tab_rects.iter().enumerate() {
            let selected = index == self.active_tab;
            g.text(
                tab_texts[index],
                (rect.left + s(2.0)) as f32,
                (rect.top + s(7.0)) as f32,
                (rect.right - rect.left - s(24.0)) as f32,
                layout.label_line as f32 + 2.0,
                13.0 * scale,
                false,
                if selected {
                    gdiplus::rgb(0xFF, 0xFF, 0xFF)
                } else {
                    gdiplus::rgb(0x9C, 0xA3, 0xAF)
                },
            );
            if selected {
                g.fill_rect(
                    gdiplus::rgb(0x3B, 0x82, 0xF6),
                    rect.left,
                    rect.bottom - s(2.0),
                    rect.right - rect.left,
                    s(2.0).max(1),
                );
            }
        }
        g.fill_rect(
            gdiplus::rgb(0x2C, 0x2F, 0x36),
            layout.content_left,
            layout.tabs_top + layout.tabs_height,
            layout.content_right - layout.content_left,
            1,
        );

        match self.active_tab {
            0 => self.paint_general(&g, &layout),
            1 => self.paint_hotkeys(&g, &layout),
            2 => self.paint_format(&g, &layout),
            _ => {}
        }

        self.paint_buttons(&g, &layout);
        if let Some(_) = layout.dropdown {
            self.paint_dropdown(&g, &layout);
        }
        drop(g);
        let (hdc, paint) = win::begin_paint(self.hwnd);
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::BitBlt(
                hdc,
                0,
                0,
                layout.width,
                layout.height,
                backbuffer.hdc,
                0,
                0,
                windows::Win32::Graphics::Gdi::SRCCOPY,
            );
        }
        win::end_paint(self.hwnd, &paint);
        self.backbuffer = surface;
    }

    fn paint_general(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let muted = gdiplus::rgb(0x9C, 0xA3, 0xAF);
        let primary = gdiplus::rgb(0xF2, 0xF3, 0xF5);

        g.text(
            i18n::t("Language"),
            layout.content_left as f32,
            layout.lang_label_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.label_line as f32 + 2.0,
            13.0 * scale,
            false,
            muted,
        );
        self.paint_combo(
            g,
            &layout.lang_combo,
            self.combo_item_label(0, self.value_for_combo(0)),
            self.language_open,
            self.focus == Focus::Language,
        );
        self.paint_checkbox(
            g,
            layout.content_left,
            layout.notify_y,
            i18n::t("ShowNotification"),
            self.working.show_save_notification,
            self.focus == Focus::Notify,
            layout,
        );
        g.text(
            i18n::t("SaveFolder"),
            layout.content_left as f32,
            layout.folder_label_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.label_line as f32 + 2.0,
            13.0 * scale,
            false,
            muted,
        );
        self.paint_folder(g, layout);
        self.paint_button(
            g,
            &layout.browse,
            i18n::t("Browse"),
            false,
            gdiplus::rgb(0x2A, 0x2D, 0x34),
            gdiplus::rgb(0x3B, 0x3F, 0x48),
            primary,
            self.hover == Target::Browse || self.focus == Focus::Browse,
            self.pressed == Target::Browse,
            false,
        );
        self.paint_checkbox(
            g,
            layout.content_left,
            layout.startup_y,
            i18n::t("Autostart"),
            self.working.run_at_startup,
            self.focus == Focus::Startup,
            layout,
        );
        let _ = s(0.0);
    }

    fn paint_hotkeys(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let muted = gdiplus::rgb(0x9C, 0xA3, 0xAF);
        let labels = [
            i18n::t("HotkeyRegion"),
            i18n::t("HotkeyFullscreen"),
            i18n::t("HotkeyClipboard"),
        ];
        for index in 0..3 {
            g.text(
                labels[index],
                layout.content_left as f32,
                layout.hotkey_label_y[index] as f32,
                (layout.content_right - layout.content_left) as f32,
                layout.label_line as f32 + 2.0,
                13.0 * scale,
                false,
                muted,
            );
            let rect = layout.hotkey_box[index];
            let focused = self.focus == Focus::Hotkey(index) || self.capturing == Some(index);
            g.fill_round_rect(gdiplus::rgb(0x23, 0x25, 0x2B), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, s(6.0));
            g.stroke_round_rect(
                if focused {
                    gdiplus::rgb(0x3B, 0x82, 0xF6)
                } else {
                    gdiplus::rgb(0x34, 0x38, 0x41)
                },
                (1.0 * scale).max(1.0),
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                s(6.0),
            );
            g.text(
                &self.display_hotkey(index),
                (rect.left + s(9.0)) as f32,
                (rect.top + s(9.0)) as f32,
                (rect.right - rect.left - s(18.0)) as f32,
                (rect.bottom - rect.top - s(18.0)) as f32,
                13.0 * scale,
                false,
                gdiplus::rgb(0xF4, 0xF5, 0xF7),
            );
        }

        let info = layout.info_rect;
        g.fill_round_rect(gdiplus::rgb(0x1F, 0x21, 0x26), info.left, info.top, info.right - info.left, info.bottom - info.top, s(8.0));
        g.stroke_round_rect(
            gdiplus::rgb(0x2C, 0x30, 0x37),
            (1.0 * scale).max(1.0),
            info.left,
            info.top,
            info.right - info.left,
            info.bottom - info.top,
            s(8.0),
        );
        let text_x = (info.left + s(14.0)) as f32;
        let text_width = (info.right - info.left - s(28.0)) as f32;
        let hint_y = (info.top + s(12.0)) as f32;
        g.text_styled(
            i18n::t("HotkeysHint"),
            text_x,
            hint_y,
            text_width,
            layout.info_hint_line as f32 + 2.0,
            12.0 * scale,
            false,
            muted,
            0,
            0,
            true,
            false,
        );
        g.text_styled(
            i18n::t("HotkeysInfo"),
            text_x,
            hint_y + layout.info_hint_line as f32 + s(7.0) as f32,
            text_width,
            layout.info_info_line as f32 + 2.0,
            12.0 * scale,
            false,
            muted,
            0,
            0,
            true,
            false,
        );
    }

    fn paint_format(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let muted = gdiplus::rgb(0x9C, 0xA3, 0xAF);
        g.text(
            i18n::t("ImageFormat"),
            layout.content_left as f32,
            layout.format_label_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.label_line as f32 + 2.0,
            13.0 * scale,
            false,
            muted,
        );
        self.paint_combo(
            g,
            &layout.format_combo,
            self.combo_item_label(1, self.value_for_combo(1)),
            self.format_open,
            self.focus == Focus::Format,
        );
        g.text(
            i18n::t("Quality"),
            layout.content_left as f32,
            layout.quality_label_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.label_line as f32 + 2.0,
            13.0 * scale,
            false,
            muted,
        );

        let slider = layout.slider;
        let center_y = (slider.top + slider.bottom) / 2;
        let track_height = s(4.0).max(2);
        let width = (slider.right - slider.left).max(1);
        let ratio = ((self.working.quality.clamp(1, 100) - 1) as f32) / 99.0;
        g.fill_round_rect(
            gdiplus::rgb(0x2A, 0x2D, 0x34),
            slider.left,
            center_y - track_height / 2,
            width,
            track_height,
            track_height / 2,
        );
        g.fill_round_rect(
            gdiplus::rgb(0x3B, 0x82, 0xF6),
            slider.left,
            center_y - track_height / 2,
            (width as f32 * ratio).round() as i32,
            track_height,
            track_height / 2,
        );
        let thumb_x = slider.left + (width as f32 * ratio).round() as i32;
        let thumb_radius = s(6.5).max(5);
        g.fill_circle(gdiplus::rgb(0xF2, 0xF3, 0xF5), thumb_x as f32, center_y as f32, thumb_radius as f32);
        g.fill_circle(gdiplus::rgb(0x3B, 0x82, 0xF6), thumb_x as f32, center_y as f32, (thumb_radius - s(2.0).max(1)) as f32);
        g.text_right(
            &format!("{}%", self.working.quality.clamp(1, 100)),
            layout.quality_value.left as f32,
            layout.quality_value.top as f32,
            (layout.quality_value.right - layout.quality_value.left) as f32,
            (layout.quality_value.bottom - layout.quality_value.top) as f32,
            13.0 * scale,
            false,
            muted,
        );
        g.text(
            i18n::t("QualityHint"),
            layout.content_left as f32,
            layout.quality_hint_y as f32,
            (layout.content_right - layout.content_left) as f32,
            layout.label_line as f32 + 2.0,
            12.0 * scale,
            false,
            muted,
        );
    }

    fn paint_checkbox(
        &self,
        g: &gdiplus::Gfx,
        x: i32,
        y: i32,
        label: &str,
        checked: bool,
        focused: bool,
        layout: &Layout,
    ) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let box_size = s(16.0).max(12);
        let box_top = y + (layout.notify_h - box_size) / 2;
        g.fill_round_rect(
            if checked {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else {
                gdiplus::rgb(0x23, 0x25, 0x2B)
            },
            x,
            box_top,
            box_size,
            box_size,
            s(3.0),
        );
        g.stroke_round_rect(
            if focused {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else if checked {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else {
                gdiplus::rgb(0x5A, 0x5F, 0x69)
            },
            (1.0 * scale).max(1.0),
            x,
            box_top,
            box_size,
            box_size,
            s(3.0),
        );
        if checked {
            let p = |px: f32, py: f32| {
                (
                    x as f32 + px * scale,
                    box_top as f32 + py * scale,
                )
            };
            let a = p(4.0, 8.2);
            let b = p(7.0, 11.0);
            let c = p(12.0, 4.8);
            g.stroke_polyline(gdiplus::rgb(0xFF, 0xFF, 0xFF), (1.8 * scale).max(1.2), &[a, b, c], false);
        }
        g.text(
            label,
            (x + box_size + s(9.0)) as f32,
            y as f32,
            (layout.content_right - x - box_size - s(9.0)) as f32,
            layout.notify_h as f32,
            13.0 * scale,
            false,
            gdiplus::rgb(0xE6, 0xE8, 0xEC),
        );
    }

    fn paint_combo(
        &self,
        g: &gdiplus::Gfx,
        rect: &RECT,
        label: &str,
        open: bool,
        focused: bool,
    ) {
        let scale = win::scale(self.hwnd);
        let s = |value: f32| (value * scale).round() as i32;
        g.fill_round_rect(gdiplus::rgb(0x23, 0x25, 0x2B), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, s(6.0));
        g.stroke_round_rect(
            if open || focused {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else {
                gdiplus::rgb(0x34, 0x38, 0x41)
            },
            (1.0 * scale).max(1.0),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            s(6.0),
        );
        g.text(
            label,
            (rect.left + s(9.0)) as f32,
            (rect.top + (rect.bottom - rect.top - s(18.0)) / 2) as f32,
            (rect.right - rect.left - s(40.0)) as f32,
            s(18.0) as f32,
            13.0 * scale,
            false,
            gdiplus::rgb(0xF4, 0xF5, 0xF7),
        );
        let cx = rect.right - s(16.0);
        let cy = (rect.top + rect.bottom) / 2;
        let half = s(4.0);
        let color = if open || focused {
            gdiplus::rgb(0x3B, 0x82, 0xF6)
        } else {
            gdiplus::rgb(0xF2, 0xF3, 0xF5)
        };
        g.line(
            color,
            (1.5 * scale).max(1.0),
            (cx - half) as f32,
            (cy - half / 2) as f32,
            cx as f32,
            (cy + half / 2) as f32,
        );
        g.line(
            color,
            (1.5 * scale).max(1.0),
            cx as f32,
            (cy + half / 2) as f32,
            (cx + half) as f32,
            (cy - half / 2) as f32,
        );
    }

    fn paint_dropdown(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let Some(rect) = layout.dropdown else {
            return;
        };
        g.fill_round_rect(gdiplus::rgb(0x1D, 0x1F, 0x24), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, s(6.0));
        g.stroke_round_rect(
            gdiplus::rgb(0x34, 0x38, 0x41),
            (1.0 * scale).max(1.0),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            s(6.0),
        );
        let combo = if self.language_open { 0 } else { 1 };
        let selected = self.value_for_combo(combo);
        for index in 0..2 {
            let item = layout.drop_items[index];
            let is_selected = index == selected;
            let is_hover = self.hover == if combo == 0 {
                Target::LanguageItem(index)
            } else {
                Target::FormatItem(index)
            };
            if is_selected {
                g.fill_round_rect(gdiplus::rgb(0x3B, 0x82, 0xF6), item.left, item.top, item.right - item.left, item.bottom - item.top, s(4.0));
            } else if is_hover {
                g.fill_round_rect(gdiplus::rgb(0x2C, 0x30, 0x37), item.left, item.top, item.right - item.left, item.bottom - item.top, s(4.0));
            }
            g.text(
                self.combo_item_label(combo, index),
                (item.left + s(10.0)) as f32,
                (item.top + s(6.0)) as f32,
                (item.right - item.left - s(20.0)) as f32,
                (item.bottom - item.top - s(12.0)) as f32,
                13.0 * scale,
                false,
                gdiplus::rgb(0xF2, 0xF3, 0xF5),
            );
        }
    }

    fn paint_folder(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let scale = layout.scale;
        let s = |value: f32| (value * scale).round() as i32;
        let rect = layout.folder_box;
        g.fill_round_rect(gdiplus::rgb(0x23, 0x25, 0x2B), rect.left, rect.top, rect.right - rect.left, rect.bottom - rect.top, s(6.0));
        g.stroke_round_rect(
            if self.focus == Focus::Folder {
                gdiplus::rgb(0x3B, 0x82, 0xF6)
            } else {
                gdiplus::rgb(0x34, 0x38, 0x41)
            },
            (1.0 * scale).max(1.0),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            s(6.0),
        );
        g.push_clip_round_rect(
            rect.left + 1,
            rect.top + 1,
            rect.right - rect.left - 2,
            rect.bottom - rect.top - 2,
            s(6.0),
        );
        let font_size = 13.0 * scale;
        let text_x = rect.left as f32 + 9.0 * scale - self.folder.scroll;
        let line_height = layout.label_line as f32;
        let text_y = rect.top as f32 + ((rect.bottom - rect.top) as f32 - line_height) / 2.0;
        if let Some((start, end)) = self.folder.selection_offsets(g, font_size) {
            let selection_x = text_x + start;
            let selection_width = (end - start).max(1.0);
            g.fill_rect(
                gdiplus::rgb(0x3B, 0x82, 0xF6),
                selection_x.round() as i32,
                (rect.top as f32 + 5.0 * scale) as i32,
                selection_width.round() as i32,
                (rect.bottom - rect.top) as i32 - (10.0 * scale) as i32,
            );
        }
        g.text(
            &self.folder.text,
            text_x,
            text_y,
            (rect.right - rect.left) as f32 + self.folder.scroll,
            line_height + 4.0,
            font_size,
            false,
            gdiplus::rgb(0xF4, 0xF5, 0xF7),
        );
        if self.focus == Focus::Folder && self.caret_visible {
            let caret = text_x + self.folder.caret_x(g, font_size);
            g.fill_rect(
                gdiplus::rgb(0xFF, 0xFF, 0xFF),
                caret.round() as i32,
                (rect.top as f32 + 6.0 * scale) as i32,
                (1.0 * scale).max(1.0) as i32,
                (rect.bottom - rect.top) as i32 - (12.0 * scale) as i32,
            );
        }
        g.reset_clip();
    }

    fn paint_buttons(&self, g: &gdiplus::Gfx, layout: &Layout) {
        let primary = gdiplus::rgb(0xF2, 0xF3, 0xF5);
        self.paint_button(
            g,
            &layout.cancel,
            i18n::t("Cancel"),
            false,
            gdiplus::rgb(0x2A, 0x2D, 0x34),
            gdiplus::rgb(0x3B, 0x3F, 0x48),
            primary,
            self.hover == Target::Cancel || self.focus == Focus::Cancel,
            self.pressed == Target::Cancel,
            false,
        );
        self.paint_button(
            g,
            &layout.save,
            i18n::t("Save"),
            true,
            gdiplus::rgb(0x3B, 0x82, 0xF6),
            gdiplus::rgb(0x5B, 0x97, 0xF8),
            gdiplus::rgb(0xFF, 0xFF, 0xFF),
            self.hover == Target::Save || self.focus == Focus::Save,
            self.pressed == Target::Save,
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_button(
        &self,
        g: &gdiplus::Gfx,
        rect: &RECT,
        label: &str,
        _accent: bool,
        background: u32,
        border: u32,
        text_color: u32,
        hovered: bool,
        pressed: bool,
        bold: bool,
    ) {
        let scale = win::scale(self.hwnd);
        let s = |value: f32| (value * scale).round() as i32;
        let factor = if pressed {
            0.72
        } else if hovered {
            0.88
        } else {
            1.0
        };
        g.fill_round_rect(
            gdiplus::opacity(background, factor),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            s(6.0),
        );
        g.stroke_round_rect(
            gdiplus::opacity(border, factor),
            (1.0 * scale).max(1.0),
            rect.left,
            rect.top,
            rect.right - rect.left,
            rect.bottom - rect.top,
            s(6.0),
        );
        g.text_center(
            label,
            rect.left as f32,
            rect.top as f32,
            (rect.right - rect.left) as f32,
            (rect.bottom - rect.top) as f32,
            13.0 * scale,
            bold,
            gdiplus::opacity(text_color, factor),
        );
        if hovered && self.focus == Focus::Save {
            let _ = s(0.0);
        }
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        win::kill_timer(self.hwnd, TIMER_CARET);
    }
}

fn point_in(rect: &RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

extern "system" fn settings_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let pointer = win::user_data(hwnd) as *mut SettingsWindow;
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
            WM_GETMINMAXINFO => {
                let info = &mut *(lparam.0 as *mut MINMAXINFO);
                let scale = win::scale(hwnd);
                info.ptMinTrackSize.x = (620.0 * scale).round() as i32;
                info.ptMinTrackSize.y = (600.0 * scale).round() as i32;
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_CARET {
                    state.caret_visible = !state.caret_visible;
                    win::invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                state.mouse_move(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                if !state.tracking_mouse {
                    state.tracking_mouse = true;
                    let mut track =
                        windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<
                                windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT,
                            >() as u32,
                            dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,
                            hwndTrack: hwnd,
                            dwHoverTime: 0,
                        };
                    let _ =
                        windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut track);
                }
                LRESULT(0)
            }
            win::WM_MOUSELEAVE_MSG => {
                state.tracking_mouse = false;
                if state.hover != Target::None {
                    state.hover = Target::None;
                    win::invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                state.mouse_down(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                win::focus(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                state.mouse_up(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let _ = wparam;
                LRESULT(0)
            }
            WM_SETCURSOR => {
                if win::loword(lparam.0 as usize) as u32
                    == windows::Win32::UI::WindowsAndMessaging::HTCLIENT
                {
                    let cursor = match state.hover {
                        Target::None => win::arrow_cursor(),
                        Target::Folder => win::ibeam_cursor(),
                        Target::Slider => win::arrow_cursor(),
                        _ => win::hand_cursor(),
                    };
                    win::set_cursor(cursor);
                    return LRESULT(1);
                }
                DefWindowProcW(hwnd, message, wparam, lparam)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                state.handle_key(win::key_from_wparam(wparam), message == WM_SYSKEYDOWN);
                LRESULT(0)
            }
            windows::Win32::UI::WindowsAndMessaging::WM_CHAR => {
                if state.focus == Focus::Folder {
                    let code = wparam.0 as u32;
                    if let Some(character) = char::from_u32(code) {
                        if character >= ' ' && character != '\u{7f}' {
                            if !win::key_is_down(VK_CONTROL.0 as u32) {
                                state.folder.insert_char(character);
                                state.ensure_folder_visible();
                                state.caret_visible = true;
                                win::set_timer(hwnd, TIMER_CARET, 530);
                                win::invalidate(hwnd);
                            }
                        }
                    }
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
                    SETTINGS_CLOSED,
                    WPARAM(pointer as usize),
                    LPARAM(0),
                );
                result
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
