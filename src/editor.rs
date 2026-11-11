use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_BACK, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_RIGHT,
    VK_SHIFT, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, WM_CHAR, WM_DPICHANGED,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_SYSKEYDOWN, WM_TIMER,
    HWND_TOPMOST, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::capture::Captured;
use crate::gdiplus::{self, BitmapSurface};
use crate::icons::{self, Icon};
use crate::images;
use crate::settings::AppSettings;
use crate::textedit::TextEdit;
use crate::win::{self, TIMER_CARET, WM_APP};

pub const EDITOR_CLOSED: u32 = WM_APP + 5;

const HANDLE_SIZE: f32 = 10.0;
const MIN_SELECTION: f32 = 6.0;
const RESIZE_MINIMUM: f32 = 20.0;
const GAP: f32 = 6.0;
const EDGE_MARGIN: f32 = 4.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Pen,
    Line,
    Arrow,
    Rectangle,
    Highlighter,
    Text,
}

#[derive(Clone)]
struct Annotation {
    kind: Tool,
    color: u32,
    thickness: f32,
    points: Vec<(f32, f32)>,
    text: String,
    font_size: f32,
    text_position: (f32, f32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    None,
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    None,
    Toolbar(usize),
    Action(usize),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ResourceHit {
    None,
    Toolbar(usize),
    Action(usize),
    Handle(Handle),
}

pub struct EditorState {
    pub hwnd: HWND,
    owner: HWND,
    app: *mut crate::app::App,
    screen_surface: Option<BitmapSurface>,
    dim_surface: Option<BitmapSurface>,
    screen: Captured,
    scale: f32,
    settings: AppSettings,
    full_selection: bool,
    selection: (f32, f32, f32, f32),
    has_selection: bool,
    finalized: bool,
    dragging_new: bool,
    moving: bool,
    resizing: bool,
    drag_start: (f32, f32),
    drag_start_selection: (f32, f32, f32, f32),
    active_handle: Handle,
    active_tool: Option<Tool>,
    preview_annotation: Option<Annotation>,
    annotations: Vec<Annotation>,
    color: u32,
    text_edit: Option<TextEdit>,
    text_position: (f32, f32),
    committing_text: bool,
    hover: Hit,
    pressed: Hit,
    hover_handle: Handle,
    caret_visible: bool,
    backbuffer: Option<BitmapSurface>,
    tracking_mouse: bool,
}

impl EditorState {
    pub fn create(
        owner: HWND,
        app: *mut crate::app::App,
        screen: Captured,
        bounds: RECT,
        scale: f32,
        settings: AppSettings,
        full_selection: bool,
    ) -> Option<Box<Self>> {
        let class_name = "LshotEditorWindow";
        let class = win::wide(class_name);
        unsafe {
            let class_struct = windows::Win32::UI::WindowsAndMessaging::WNDCLASSW {
                style: windows::Win32::UI::WindowsAndMessaging::WNDCLASS_STYLES(0),
                lpfnWndProc: Some(editor_proc),
                hInstance: win::module_instance(),
                hCursor: win::cross_cursor(),
                lpszClassName: win::pcwstr(&class),
                ..Default::default()
            };
            let _ = windows::Win32::UI::WindowsAndMessaging::RegisterClassW(&class_struct);
        }

        let screen_surface = BitmapSurface::from_bgra(&screen.bgra, screen.width, screen.height)?;
        let mut dimmed = screen.bgra.clone();
        for pixel in dimmed.chunks_exact_mut(4) {
            pixel[0] = (pixel[0] as u32 * 128 / 255) as u8;
            pixel[1] = (pixel[1] as u32 * 128 / 255) as u8;
            pixel[2] = (pixel[2] as u32 * 128 / 255) as u8;
        }
        let dim_surface = BitmapSurface::from_bgra(&dimmed, screen.width, screen.height)?;

        let width = (bounds.right - bounds.left).max(1);
        let height = (bounds.bottom - bounds.top).max(1);
        let mut state = Box::new(Self {
            hwnd: HWND::default(),
            owner,
            app,
            screen_surface: Some(screen_surface),
            dim_surface: Some(dim_surface),
            screen,
            scale: scale.max(0.5),
            settings,
            full_selection,
            selection: (0.0, 0.0, 0.0, 0.0),
            has_selection: false,
            finalized: false,
            dragging_new: false,
            moving: false,
            resizing: false,
            drag_start: (0.0, 0.0),
            drag_start_selection: (0.0, 0.0, 0.0, 0.0),
            active_handle: Handle::None,
            active_tool: None,
            preview_annotation: None,
            annotations: Vec::new(),
            color: gdiplus::rgb(0xFF, 0x00, 0x00),
            text_edit: None,
            text_position: (0.0, 0.0),
            committing_text: false,
            hover: Hit::None,
            pressed: Hit::None,
            hover_handle: Handle::None,
            caret_visible: true,
            backbuffer: None,
            tracking_mouse: false,
        });

        let hwnd = unsafe {
            win::create_window(
                class_name,
                "",
                WS_POPUP,
                WS_EX_TOOLWINDOW,
                bounds.left,
                bounds.top,
                width,
                height,
                HWND::default(),
                None,
            )
        };
        if hwnd.is_invalid() {
            return None;
        }
        state.hwnd = hwnd;
        win::set_user_data(hwnd, &mut *state as *mut Self as isize);
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                bounds.left,
                bounds.top,
                width,
                height,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        if full_selection {
            state.selection = (0.0, 0.0, width as f32, height as f32);
            state.has_selection = true;
            state.finalized = true;
        }
        Some(state)
    }

    pub fn show(&mut self) {
        win::show(self.hwnd);
        win::activate(self.hwnd);
        win::invalidate(self.hwnd);
    }

    fn canvas_width(&self) -> f32 {
        let rect = win::client_rect(self.hwnd);
        (rect.right - rect.left).max(1) as f32
    }

    fn canvas_height(&self) -> f32 {
        let rect = win::client_rect(self.hwnd);
        (rect.bottom - rect.top).max(1) as f32
    }

    fn clamp_point(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x.clamp(0.0, self.canvas_width()),
            y.clamp(0.0, self.canvas_height()),
        )
    }

    fn clamp_point_to_selection(&self, x: f32, y: f32) -> (f32, f32) {
        let (left, top, width, height) = self.selection;
        (
            x.clamp(left, left + width),
            y.clamp(top, top + height),
        )
    }

    fn clamp_selection(&self, x: f32, y: f32, width: f32, height: f32) -> (f32, f32, f32, f32) {
        let canvas_width = self.canvas_width();
        let canvas_height = self.canvas_height();
        let left = x.clamp(0.0, canvas_width);
        let top = y.clamp(0.0, canvas_height);
        let right = (x + width).clamp(0.0, canvas_width);
        let bottom = (y + height).clamp(0.0, canvas_height);
        (left, top, (right - left).max(0.0), (bottom - top).max(0.0))
    }

    fn selection_pixels(&self) -> (i32, i32, i32, i32) {
        let (left, top, width, height) = self.selection;
        let scale_x = self.screen.width as f32 / self.canvas_width();
        let scale_y = self.screen.height as f32 / self.canvas_height();
        let mut l = (left * scale_x).round() as i32;
        let mut t = (top * scale_y).round() as i32;
        let mut r = ((left + width) * scale_x).round() as i32;
        let mut b = ((top + height) * scale_y).round() as i32;
        l = l.clamp(0, (self.screen.width - 1).max(0));
        t = t.clamp(0, (self.screen.height - 1).max(0));
        r = r.clamp(l + 1, self.screen.width);
        b = b.clamp(t + 1, self.screen.height);
        (l, t, r - l, b - t)
    }

    fn toolbar_geometry(&self) -> ((i32, i32, i32, i32), (i32, i32, i32, i32)) {
        let s = self.scale;
        let tool_width = (24.0 * s + 6.0 * s).round() as i32;
        let tool_height = (8.0 * 26.0 * s + 6.0 * s).round() as i32;
        let action_width = (3.0 * 26.0 * s + 6.0 * s).round() as i32;
        let action_height = (24.0 * s + 6.0 * s).round() as i32;
        let canvas_width = self.canvas_width() as i32;
        let canvas_height = self.canvas_height() as i32;
        let (left, top, width, height) = self.selection;
        let right = (left + width).round() as i32;
        let bottom = (top + height).round() as i32;
        let gap = (GAP * s).round() as i32;
        let margin = (EDGE_MARGIN * s).round() as i32;

        let mut tool_x = right + gap;
        if tool_x + tool_width > canvas_width - margin {
            tool_x = canvas_width - tool_width - margin;
        }
        tool_x = tool_x.clamp(margin, (canvas_width - tool_width - margin).max(margin));
        let tool_y = (bottom - tool_height).clamp(margin, (canvas_height - tool_height - margin).max(margin));

        let mut action_x = right - action_width;
        action_x = action_x.clamp(margin, (canvas_width - action_width - margin).max(margin));
        let mut action_y = bottom + gap;
        if action_y + action_height > canvas_height - margin {
            let above = top.round() as i32 - action_height - gap;
            action_y = if above >= margin {
                above
            } else {
                canvas_height - action_height - margin
            };
        }
        action_y = action_y.clamp(margin, (canvas_height - action_height - margin).max(margin));

        (
            (tool_x, tool_y, tool_width, tool_height),
            (action_x, action_y, action_width, action_height),
        )
    }

    fn toolbar_item_rect(&self, index: usize) -> (i32, i32, i32, i32) {
        let (toolbar, _) = self.toolbar_geometry();
        let s = self.scale;
        let size = (24.0 * s).round() as i32;
        let x = toolbar.0 + (3.0 * s).round() as i32;
        let y = toolbar.1 + (4.0 * s).round() as i32 + index as i32 * (26.0 * s).round() as i32;
        (x, y, size, size)
    }

    fn action_item_rect(&self, index: usize) -> (i32, i32, i32, i32) {
        let (_, action) = self.toolbar_geometry();
        let s = self.scale;
        let size = (24.0 * s).round() as i32;
        let x = action.0 + (4.0 * s).round() as i32 + index as i32 * (26.0 * s).round() as i32;
        let y = action.1 + (3.0 * s).round() as i32;
        (x, y, size, size)
    }

    fn hit_resources(&self, x: i32, y: i32) -> ResourceHit {
        if !self.finalized || !self.has_selection {
            return ResourceHit::None;
        }
        let point = POINT { x, y };
        for index in 0..8 {
            let (rx, ry, rw, rh) = self.toolbar_item_rect(index);
            if point.x >= rx && point.x < rx + rw && point.y >= ry && point.y < ry + rh {
                return ResourceHit::Toolbar(index);
            }
        }
        for index in 0..3 {
            let (rx, ry, rw, rh) = self.action_item_rect(index);
            if point.x >= rx && point.x < rx + rw && point.y >= ry && point.y < ry + rh {
                return ResourceHit::Action(index);
            }
        }
        if let Some(handle) = self.handle_at(x as f32, y as f32) {
            return ResourceHit::Handle(handle);
        }
        ResourceHit::None
    }

    fn toolbar_for_tool(tool: Tool) -> Icon {
        match tool {
            Tool::Pen => Icon::Pen,
            Tool::Line => Icon::Line,
            Tool::Arrow => Icon::Arrow,
            Tool::Rectangle => Icon::Rectangle,
            Tool::Highlighter => Icon::Highlighter,
            Tool::Text => Icon::Text,
        }
    }

    fn tool_for_index(index: usize) -> Option<Tool> {
        match index {
            0 => Some(Tool::Pen),
            1 => Some(Tool::Line),
            2 => Some(Tool::Arrow),
            3 => Some(Tool::Rectangle),
            4 => Some(Tool::Highlighter),
            5 => Some(Tool::Text),
            _ => None,
        }
    }

    fn handle_at(&self, x: f32, y: f32) -> Option<Handle> {
        if !self.finalized {
            return None;
        }
        let (left, top, width, height) = self.selection;
        let right = left + width;
        let bottom = top + height;
        let cx = left + width / 2.0;
        let cy = top + height / 2.0;
        let centers = [
            (Handle::NorthWest, left, top),
            (Handle::North, cx, top),
            (Handle::NorthEast, right, top),
            (Handle::East, right, cy),
            (Handle::SouthEast, right, bottom),
            (Handle::South, cx, bottom),
            (Handle::SouthWest, left, bottom),
            (Handle::West, left, cy),
        ];
        let radius = 7.0 * self.scale;
        for (handle, hx, hy) in centers {
            if (x - hx).abs() <= radius && (y - hy).abs() <= radius {
                return Some(handle);
            }
        }
        None
    }

    fn selection_contains(&self, x: f32, y: f32) -> bool {
        if !self.has_selection {
            return false;
        }
        let (left, top, width, height) = self.selection;
        x >= left && x <= left + width && y >= top && y <= top + height
    }

    fn mouse_down(&mut self, x: i32, y: i32) {
        let fx = x as f32;
        let fy = y as f32;

        if let Some(editor) = &self.text_edit {
            let (rect, font_size) = self.text_editor_rect_with_font(editor);
            if fx >= rect.0 && fx <= rect.0 + rect.2 && fy >= rect.1 && fy <= rect.1 + rect.3 {
                if let Some(surface) = BitmapSurface::new(4, 4) {
                    if let Some(g) = surface.graphics() {
                        let inner = fx - rect.0 - 3.0 * self.scale + editor.scroll;
                        if let Some(edit) = &mut self.text_edit {
                            edit.position_from_x(&g, inner, font_size);
                            edit.ensure_visible(&g, font_size, rect.2 - 6.0 * self.scale);
                        }
                    }
                }
                self.caret_visible = true;
                win::set_timer(self.hwnd, TIMER_CARET, 530);
                win::invalidate(self.hwnd);
                return;
            }
        }

        let resource = self.hit_resources(x, y);
        match resource {
            ResourceHit::Toolbar(index) => {
                self.commit_text();
                self.pressed = Hit::Toolbar(index);
                win::invalidate(self.hwnd);
                return;
            }
            ResourceHit::Action(index) => {
                self.commit_text();
                self.pressed = Hit::Action(index);
                win::invalidate(self.hwnd);
                return;
            }
            ResourceHit::Handle(handle) => {
                self.commit_text();
                self.active_handle = handle;
                self.resizing = true;
                self.drag_start = (fx, fy);
                self.drag_start_selection = self.selection;
                win::capture_mouse(self.hwnd);
                return;
            }
            ResourceHit::None => {}
        }

        self.commit_text();

        if self.finalized && self.has_selection {
            if self.selection_contains(fx, fy) {
                match self.active_tool {
                    Some(Tool::Text) => {
                        self.begin_text(fx, fy);
                        return;
                    }
                    Some(_) => {
                        self.begin_annotation(fx, fy);
                        return;
                    }
                    None => {
                        self.moving = true;
                        self.drag_start = (fx, fy);
                        self.drag_start_selection = self.selection;
                        win::capture_mouse(self.hwnd);
                        return;
                    }
                }
            }
        }

        self.begin_new_selection(fx, fy);
    }

    fn begin_new_selection(&mut self, x: f32, y: f32) {
        self.clear_annotations();
        self.active_tool = None;
        self.has_selection = true;
        self.finalized = false;
        self.dragging_new = true;
        self.pressed = Hit::None;
        let (cx, cy) = self.clamp_point(x, y);
        self.drag_start = (cx, cy);
        self.selection = (cx, cy, 0.0, 0.0);
        win::capture_mouse(self.hwnd);
        win::invalidate(self.hwnd);
    }

    fn mouse_move(&mut self, x: i32, y: i32) {
        let (cx, cy) = self.clamp_point(x as f32, y as f32);

        if self.dragging_new {
            let (sx, sy) = self.drag_start;
            let left = sx.min(cx);
            let top = sy.min(cy);
            self.selection = self.clamp_selection(left, top, (cx - sx).abs(), (cy - sy).abs());
            win::invalidate(self.hwnd);
            return;
        }

        if self.resizing {
            self.update_resize(cx, cy);
            return;
        }

        if self.moving {
            let dx = cx - self.drag_start.0;
            let dy = cy - self.drag_start.1;
            let (sl, st, sw, sh) = self.drag_start_selection;
            self.selection = self.clamp_selection(sl + dx, st + dy, sw, sh);
            win::invalidate(self.hwnd);
            return;
        }

        let clamped = self.clamp_point_to_selection(cx, cy);
        if let Some(annotation) = &mut self.preview_annotation {
            let (px, py) = clamped;
            if annotation.kind == Tool::Pen || annotation.kind == Tool::Highlighter {
                if let Some(last) = annotation.points.last() {
                    if (last.0 - px).abs() >= self.scale || (last.1 - py).abs() >= self.scale {
                        annotation.points.push((px, py));
                    }
                }
            } else if annotation.points.len() < 2 {
                annotation.points.push((px, py));
            } else if let Some(point) = annotation.points.get_mut(1) {
                *point = (px, py);
            }
            win::invalidate(self.hwnd);
            return;
        }

        let resource = self.hit_resources(x, y);
        let hover = match resource {
            ResourceHit::Toolbar(index) => Hit::Toolbar(index),
            ResourceHit::Action(index) => Hit::Action(index),
            _ => Hit::None,
        };
        self.hover_handle = match resource {
            ResourceHit::Handle(handle) => handle,
            _ => Handle::None,
        };
        if hover != self.hover {
            self.hover = hover;
            win::invalidate(self.hwnd);
        }

        let cursor = if self.hover_handle != Handle::None {
            match self.hover_handle {
                Handle::NorthWest | Handle::SouthEast => win::size_nwse_cursor(),
                Handle::NorthEast | Handle::SouthWest => win::size_nesw_cursor(),
                Handle::North | Handle::South => win::size_ns_cursor(),
                Handle::East | Handle::West => win::size_we_cursor(),
                _ => win::cross_cursor(),
            }
        } else {
            match self.hover {
                Hit::Toolbar(_) | Hit::Action(_) => win::hand_cursor(),
                Hit::None => {
                    if self.finalized && self.selection_contains(cx, cy) {
                        match self.active_tool {
                            Some(Tool::Text) => win::ibeam_cursor(),
                            Some(_) => win::cross_cursor(),
                            None => win::size_all_cursor(),
                        }
                    } else {
                        win::cross_cursor()
                    }
                }
            }
        };
        win::set_cursor(cursor);

        if !self.tracking_mouse {
            self.tracking_mouse = true;
            let mut track = windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<
                    windows::Win32::UI::Input::KeyboardAndMouse::TRACKMOUSEEVENT,
                >() as u32,
                dwFlags: windows::Win32::UI::Input::KeyboardAndMouse::TME_LEAVE,
                hwndTrack: self.hwnd,
                dwHoverTime: 0,
            };
            let _ = unsafe {
                windows::Win32::UI::Input::KeyboardAndMouse::TrackMouseEvent(&mut track)
            };
        }
    }

    fn update_resize(&mut self, x: f32, y: f32) {
        let (left, top, width, height) = self.drag_start_selection;
        let right = left + width;
        let bottom = top + height;
        let mut new_left = left;
        let mut new_top = top;
        let mut new_right = right;
        let mut new_bottom = bottom;
        match self.active_handle {
            Handle::NorthWest => {
                new_left = x;
                new_top = y;
            }
            Handle::North => new_top = y,
            Handle::NorthEast => {
                new_right = x;
                new_top = y;
            }
            Handle::East => new_right = x,
            Handle::SouthEast => {
                new_right = x;
                new_bottom = y;
            }
            Handle::South => new_bottom = y,
            Handle::SouthWest => {
                new_left = x;
                new_bottom = y;
            }
            Handle::West => new_left = x,
            Handle::None => {}
        }

        let canvas_width = self.canvas_width();
        let canvas_height = self.canvas_height();
        new_left = new_left.clamp(0.0, canvas_width);
        new_top = new_top.clamp(0.0, canvas_height);
        new_right = new_right.clamp(0.0, canvas_width);
        new_bottom = new_bottom.clamp(0.0, canvas_height);
        let minimum = RESIZE_MINIMUM * self.scale;

        if new_right - new_left < minimum {
            match self.active_handle {
                Handle::West | Handle::NorthWest | Handle::SouthWest => {
                    new_left = (new_right - minimum).max(0.0);
                }
                _ => {
                    new_right = (new_left + minimum).min(canvas_width);
                }
            }
        }
        if new_bottom - new_top < minimum {
            match self.active_handle {
                Handle::North | Handle::NorthWest | Handle::NorthEast => {
                    new_top = (new_bottom - minimum).max(0.0);
                }
                _ => {
                    new_bottom = (new_top + minimum).min(canvas_height);
                }
            }
        }

        self.selection = (
            new_left,
            new_top,
            (new_right - new_left).max(0.0),
            (new_bottom - new_top).max(0.0),
        );
        win::invalidate(self.hwnd);
    }

    fn mouse_up(&mut self, x: i32, y: i32) {
        if self.dragging_new {
            self.dragging_new = false;
            win::release_mouse();
            let minimum = MIN_SELECTION * self.scale;
            if self.selection.2 < minimum || self.selection.3 < minimum {
                self.reset_selection();
            } else {
                self.finalized = true;
            }
            win::invalidate(self.hwnd);
            return;
        }

        if self.resizing || self.moving {
            self.resizing = false;
            self.moving = false;
            self.active_handle = Handle::None;
            win::release_mouse();
            win::invalidate(self.hwnd);
            return;
        }

        if self.preview_annotation.is_some() {
            self.commit_annotation();
            win::release_mouse();
            win::invalidate(self.hwnd);
            return;
        }

        let resource = self.hit_resources(x, y);
        let pressed = self.pressed;
        self.pressed = Hit::None;
        match (pressed, resource) {
            (Hit::Toolbar(index), ResourceHit::Toolbar(hovered)) if index == hovered => {
                self.activate_tool_index(index);
            }
            (Hit::Action(index), ResourceHit::Action(hovered)) if index == hovered => {
                match index {
                    0 => self.copy_to_clipboard(),
                    1 => self.save_to_file(),
                    2 => win::destroy_window(self.hwnd),
                    _ => {}
                }
            }
            _ => {}
        }
        win::invalidate(self.hwnd);
    }

    fn activate_tool_index(&mut self, index: usize) {
        if let Some(tool) = Self::tool_for_index(index) {
            self.active_tool = if self.active_tool == Some(tool) {
                None
            } else {
                Some(tool)
            };
        } else if index == 6 {
            self.choose_color();
        } else if index == 7 {
            self.undo();
        }
        win::invalidate(self.hwnd);
    }

    fn choose_color(&mut self) {
        unsafe {
            let mut custom = [windows::Win32::Foundation::COLORREF(0); 16];
            let mut dialog = windows::Win32::UI::Controls::Dialogs::CHOOSECOLORW {
                lStructSize: std::mem::size_of::<
                    windows::Win32::UI::Controls::Dialogs::CHOOSECOLORW,
                >() as u32,
                hwndOwner: self.hwnd,
                rgbResult: gdiplus::colorref(self.color),
                lpCustColors: custom.as_mut_ptr(),
                Flags: windows::Win32::UI::Controls::Dialogs::CC_ANYCOLOR
                    | windows::Win32::UI::Controls::Dialogs::CC_FULLOPEN
                    | windows::Win32::UI::Controls::Dialogs::CC_RGBINIT,
                ..Default::default()
            };
            if windows::Win32::UI::Controls::Dialogs::ChooseColorW(&mut dialog).as_bool() {
                let value = dialog.rgbResult.0;
                let red = (value & 0xFF) as u8;
                let green = ((value >> 8) & 0xFF) as u8;
                let blue = ((value >> 16) & 0xFF) as u8;
                self.color = gdiplus::rgb(red, green, blue);
            }
        }
        win::activate(self.hwnd);
        win::invalidate(self.hwnd);
    }

    fn undo(&mut self) {
        if self.text_edit.is_some() {
            self.cancel_text();
            return;
        }
        if self.preview_annotation.is_some() {
            self.preview_annotation = None;
            win::release_mouse();
            win::invalidate(self.hwnd);
            return;
        }
        self.annotations.pop();
        win::invalidate(self.hwnd);
    }

    fn clear_annotations(&mut self) {
        self.annotations.clear();
        self.preview_annotation = None;
    }

    fn reset_selection(&mut self) {
        self.has_selection = false;
        self.finalized = false;
        self.dragging_new = false;
        self.selection = (0.0, 0.0, 0.0, 0.0);
        win::invalidate(self.hwnd);
    }

    fn begin_annotation(&mut self, x: f32, y: f32) {
        let Some(tool) = self.active_tool else {
            return;
        };
        let (px, py) = self.clamp_point_to_selection(x, y);
        let thickness = if tool == Tool::Highlighter {
            16.0 * self.scale
        } else {
            3.0 * self.scale
        };
        let annotation = Annotation {
            kind: tool,
            color: self.color,
            thickness,
            points: vec![(px, py)],
            text: String::new(),
            font_size: 0.0,
            text_position: (0.0, 0.0),
        };
        self.preview_annotation = Some(annotation);
        win::capture_mouse(self.hwnd);
        win::invalidate(self.hwnd);
    }

    fn commit_annotation(&mut self) {
        let Some(annotation) = self.preview_annotation.take() else {
            return;
        };
        let mut keep = true;
        if annotation.kind != Tool::Pen && annotation.kind != Tool::Highlighter {
            if annotation.points.len() < 2 {
                keep = false;
            } else {
                let dx = annotation.points[1].0 - annotation.points[0].0;
                let dy = annotation.points[1].1 - annotation.points[0].1;
                keep = dx.abs() >= 2.0 * self.scale || dy.abs() >= 2.0 * self.scale;
            }
        }
        if keep {
            self.annotations.push(annotation);
        }
    }

    fn text_editor_rect_with_font(&self, editor: &TextEdit) -> ((f32, f32, f32, f32), f32) {
        let font_size = 20.0 * self.scale;
        let (left, top, width, _) = self.selection;
        let max_width = (left + width - self.text_position.0 - 6.0 * self.scale)
            .max(120.0 * self.scale);
        let measured = if editor.text.is_empty() {
            0.0
        } else {
            measure_text(&editor.text, font_size).0
        };
        let width = (measured + 10.0 * self.scale)
            .max(120.0 * self.scale)
            .min(max_width);
        let height = font_size * 1.35 + 8.0 * self.scale;
        (
            (self.text_position.0, self.text_position.1, width, height),
            font_size,
        )
    }

    fn begin_text(&mut self, x: f32, y: f32) {
        self.commit_text();
        let (px, py) = self.clamp_point_to_selection(x, y);
        self.text_position = (px, py);
        self.text_edit = Some(TextEdit::new(""));
        self.caret_visible = true;
        win::set_timer(self.hwnd, TIMER_CARET, 530);
        win::invalidate(self.hwnd);
    }

    fn commit_text(&mut self) {
        if self.committing_text {
            return;
        }
        let Some(editor) = self.text_edit.take() else {
            return;
        };
        self.committing_text = true;
        let text = editor.text.trim().to_string();
        if !text.is_empty() {
            self.annotations.push(Annotation {
                kind: Tool::Text,
                color: self.color,
                thickness: 0.0,
                points: Vec::new(),
                text,
                font_size: 20.0 * self.scale,
                text_position: self.text_position,
            });
        }
        win::kill_timer(self.hwnd, TIMER_CARET);
        self.committing_text = false;
        win::focus(self.hwnd);
        win::invalidate(self.hwnd);
    }

    fn cancel_text(&mut self) {
        if self.text_edit.take().is_some() {
            win::kill_timer(self.hwnd, TIMER_CARET);
            win::focus(self.hwnd);
            win::invalidate(self.hwnd);
        }
    }

    fn handle_key(&mut self, vk: u32, _system: bool) {
        if let Some(editor) = &mut self.text_edit {
            let control = win::key_is_down(VK_CONTROL.0 as u32);
            let shift = win::key_is_down(VK_SHIFT.0 as u32);
            match vk {
                v if v == VK_ESCAPE.0 as u32 => {
                    self.cancel_text();
                    return;
                }
                0x0D => {
                    self.commit_text();
                    return;
                }
                v if v == VK_BACK.0 as u32 => editor.backspace(),
                v if v == VK_DELETE.0 as u32 => editor.delete(),
                v if v == VK_LEFT.0 as u32 => editor.move_left(shift),
                v if v == VK_RIGHT.0 as u32 => editor.move_right(shift),
                v if v == VK_HOME.0 as u32 => editor.home(shift),
                v if v == VK_END.0 as u32 => editor.end(shift),
                v if v == VK_UP.0 as u32 => editor.home(shift),
                v if v == VK_DOWN.0 as u32 => editor.end(shift),
                _ => {
                    if control {
                        match vk {
                            0x41 => editor.select_all(),
                            0x43 => {
                                editor.copy(self.hwnd);
                            }
                            0x58 => {
                                editor.cut(self.hwnd);
                            }
                            0x56 => {
                                editor.paste(self.hwnd);
                            }
                            _ => {}
                        }
                    }
                }
            }
            if let Some(edit) = &self.text_edit {
                let (rect, font_size) = self.text_editor_rect_with_font(edit);
                if let Some(surface) = BitmapSurface::new(4, 4) {
                    if let Some(g) = surface.graphics() {
                        if let Some(edit) = &mut self.text_edit {
                            edit.ensure_visible(&g, font_size, rect.2 - 6.0 * self.scale);
                        }
                    }
                }
            }
            self.caret_visible = true;
            win::set_timer(self.hwnd, TIMER_CARET, 530);
            win::invalidate(self.hwnd);
            return;
        }

        if vk == VK_ESCAPE.0 as u32 {
            if self.active_tool.is_some() {
                self.active_tool = None;
                win::invalidate(self.hwnd);
            } else {
                win::destroy_window(self.hwnd);
            }
            return;
        }
        let control = win::key_is_down(VK_CONTROL.0 as u32);
        if control && vk == 0x43 {
            if self.has_selection && self.finalized {
                self.copy_to_clipboard();
            }
            return;
        }
        if control && vk == 0x5A {
            self.undo();
        }
    }

    fn handle_char(&mut self, character: char) {
        if let Some(editor) = &mut self.text_edit {
            if character >= ' ' && character != '\u{7f}' {
                editor.insert_char(character);
                if let Some(edit) = &self.text_edit {
                    let (rect, font_size) = self.text_editor_rect_with_font(edit);
                    if let Some(surface) = BitmapSurface::new(4, 4) {
                        if let Some(g) = surface.graphics() {
                            if let Some(edit) = &mut self.text_edit {
                                edit.ensure_visible(&g, font_size, rect.2 - 6.0 * self.scale);
                            }
                        }
                    }
                }
                self.caret_visible = true;
                win::set_timer(self.hwnd, TIMER_CARET, 530);
                win::invalidate(self.hwnd);
            }
        }
    }

    fn compose(&self) -> Result<Vec<u8>, String> {
        if !self.has_selection || self.selection.2 < 1.0 || self.selection.3 < 1.0 {
            return Err("No selection.".to_string());
        }
        let (left, top, width, height) = self.selection_pixels();
        let source_stride = self.screen.width as usize * 4;
        let row_bytes = width as usize * 4;
        let mut buffer = vec![0u8; row_bytes * height as usize];
        for row in 0..height as usize {
            let source = (top as usize + row) * source_stride + left as usize * 4;
            let destination = row * row_bytes;
            buffer[destination..destination + row_bytes]
                .copy_from_slice(&self.screen.bgra[source..source + row_bytes]);
            for pixel in buffer[destination..destination + row_bytes].chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }

        let mut surface = BitmapSurface::new(width, height)
            .ok_or_else(|| "The image could not be prepared.".to_string())?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                buffer.as_ptr(),
                surface.bits(),
                buffer.len(),
            );
        }
        {
            let g = surface
                .graphics()
                .ok_or_else(|| "The drawing surface could not be created.".to_string())?;
            g.push_clip_rect(0, 0, width, height);
            g.set_transform(
                1.0,
                0.0,
                0.0,
                1.0,
                -(left as f32),
                -(top as f32),
            );
            for annotation in &self.annotations {
                self.draw_annotation(&g, annotation);
            }
            g.reset_transform();
            g.reset_clip();
        }
        unsafe {
            let bits = surface.bits();
            let mut output = vec![0u8; buffer.len()];
            std::ptr::copy_nonoverlapping(bits as *const u8, output.as_mut_ptr(), output.len());
            for pixel in output.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
            Ok(output)
        }
    }

    fn draw_annotation(&self, g: &gdiplus::Gfx, annotation: &Annotation) {
        let points = &annotation.points;
        match annotation.kind {
            Tool::Pen => {
                if points.is_empty() {
                    return;
                }
                if points.len() == 1 {
                    g.fill_circle(
                        annotation.color,
                        points[0].0,
                        points[0].1,
                        (annotation.thickness / 2.0).max(1.0),
                    );
                } else {
                    g.stroke_polyline(
                        annotation.color,
                        annotation.thickness,
                        points,
                        false,
                    );
                }
            }
            Tool::Highlighter => {
                if points.is_empty() {
                    return;
                }
                let color = gdiplus::with_alpha(annotation.color, 82);
                if points.len() == 1 {
                    g.fill_circle(
                        color,
                        points[0].0,
                        points[0].1,
                        (annotation.thickness / 2.0).max(1.0),
                    );
                } else {
                    g.stroke_polyline(color, annotation.thickness.max(10.0 * self.scale), points, true);
                }
            }
            Tool::Line => {
                if points.len() >= 2 {
                    g.line(
                        annotation.color,
                        annotation.thickness,
                        points[0].0,
                        points[0].1,
                        points[1].0,
                        points[1].1,
                    );
                }
            }
            Tool::Arrow => {
                if points.len() < 2 {
                    return;
                }
                let start = points[0];
                let end = points[1];
                let dx = end.0 - start.0;
                let dy = end.1 - start.1;
                let length = (dx * dx + dy * dy).sqrt();
                if length < 0.5 {
                    return;
                }
                let dir = (dx / length, dy / length);
                let perpendicular = (-dir.1, dir.0);
                let head_length = (11.0 * self.scale).max(annotation.thickness * 4.0);
                let base = (
                    end.0 - dir.0 * head_length,
                    end.1 - dir.1 * head_length,
                );
                g.line(
                    annotation.color,
                    annotation.thickness,
                    start.0,
                    start.1,
                    base.0,
                    base.1,
                );
                let half = head_length * 0.48;
                let triangle = [
                    end,
                    (base.0 + perpendicular.0 * half, base.1 + perpendicular.1 * half),
                    (base.0 - perpendicular.0 * half, base.1 - perpendicular.1 * half),
                ];
                g.fill_polygon(annotation.color, &triangle);
            }
            Tool::Rectangle => {
                if points.len() >= 2 {
                    let left = points[0].0.min(points[1].0);
                    let top = points[0].1.min(points[1].1);
                    let right = points[0].0.max(points[1].0);
                    let bottom = points[0].1.max(points[1].1);
                    g.stroke_round_rect(
                        annotation.color,
                        annotation.thickness,
                        left.round() as i32,
                        top.round() as i32,
                        (right - left).round() as i32,
                        (bottom - top).round() as i32,
                        0,
                    );
                }
            }
            Tool::Text => {
                if annotation.text.is_empty() {
                    return;
                }
                let shadow = gdiplus::argb(150, 0, 0, 0);
                g.text(
                    &annotation.text,
                    annotation.text_position.0 + 1.5 * self.scale,
                    annotation.text_position.1 + 1.5 * self.scale,
                    100000.0,
                    annotation.font_size * 1.4,
                    annotation.font_size,
                    false,
                    shadow,
                );
                g.text(
                    &annotation.text,
                    annotation.text_position.0,
                    annotation.text_position.1,
                    100000.0,
                    annotation.font_size * 1.4,
                    annotation.font_size,
                    false,
                    annotation.color,
                );
            }
        }
    }

    fn copy_to_clipboard(&mut self) {
        self.commit_text();
        let image = match self.compose() {
            Ok(image) => image,
            Err(error) => {
                crate::dialogs::show_error(
                    self.hwnd,
                    &crate::i18n::t1("ExportFailed", &error),
                );
                return;
            }
        };
        let (_, _, width, height) = self.selection_pixels();
        match images::set_clipboard_image(self.hwnd, &image, width, height) {
            Ok(()) => {
                crate::clipboard::save(&image, width, height);
                win::destroy_window(self.hwnd);
            }
            Err(error) => {
                crate::dialogs::show_error(
                    self.hwnd,
                    &crate::i18n::t1("CopyFailed", &error),
                );
            }
        }
    }

    fn save_to_file(&mut self) {
        self.commit_text();
        let image = match self.compose() {
            Ok(image) => image,
            Err(error) => {
                crate::dialogs::show_error(
                    self.hwnd,
                    &crate::i18n::t1("ExportFailed", &error),
                );
                return;
            }
        };
        let jpeg_default = self.settings.image_format.eq_ignore_ascii_case("jpeg");
        let initial = if Path::new(&self.settings.save_folder).is_dir() {
            PathBuf::from(&self.settings.save_folder)
        } else {
            crate::settings::pictures_folder()
        };
        let Some(path) = crate::dialogs::save_file(
            self.hwnd,
            crate::i18n::t("SaveDialogTitle"),
            &initial,
            &crate::dialogs::default_file_name(),
            jpeg_default,
        ) else {
            return;
        };
        let (_, _, width, height) = self.selection_pixels();
        if let Err(error) =
            images::save_image_file(&image, width, height, &path, self.settings.quality)
        {
            crate::dialogs::show_error(
                self.hwnd,
                &crate::i18n::t1("SaveFailed", &error),
            );
            return;
        }
        let path_string = path.to_string_lossy().to_string();
        win::destroy_window(self.hwnd);
        unsafe {
            if !self.app.is_null() {
                (*self.app).notify_saved(&path_string);
            }
        }
    }

    fn paint(&mut self) {
        let width = self.canvas_width() as i32;
        let height = self.canvas_height() as i32;
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
        let base = self.dim_surface.as_ref().or(self.screen_surface.as_ref());
        if let Some(base) = base {
            unsafe {
                use windows::Win32::Graphics::Gdi::{BitBlt, StretchBlt, SRCCOPY};
                if base.width == width && base.height == height {
                    let _ = BitBlt(backbuffer.hdc, 0, 0, width, height, base.hdc, 0, 0, SRCCOPY);
                } else {
                    let _ = windows::Win32::Graphics::Gdi::SetStretchBltMode(
                        backbuffer.hdc,
                        windows::Win32::Graphics::Gdi::HALFTONE,
                    );
                    let _ = StretchBlt(
                        backbuffer.hdc,
                        0,
                        0,
                        width,
                        height,
                        base.hdc,
                        0,
                        0,
                        base.width,
                        base.height,
                        SRCCOPY,
                    );
                }
            }
        }
        let (sel_left, sel_top, sel_width, sel_height) = self.selection;
        if self.has_selection && sel_width > 0.0 && sel_height > 0.0 {
            if let Some(screen_surface) = &self.screen_surface {
                let left = sel_left.round() as i32;
                let top = sel_top.round() as i32;
                let selection_width = sel_width.round() as i32;
                let selection_height = sel_height.round() as i32;
                unsafe {
                    use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};
                    let _ = BitBlt(
                        backbuffer.hdc,
                        left,
                        top,
                        selection_width,
                        selection_height,
                        screen_surface.hdc,
                        left,
                        top,
                        SRCCOPY,
                    );
                }
            }
        }
        let Some(g) = backbuffer.graphics() else {
            self.backbuffer = surface;
            return;
        };

        let (sel_left, sel_top, sel_width, sel_height) = self.selection;

        if self.has_selection && sel_width > 0.0 && sel_height > 0.0 {
            g.push_clip_rect(
                sel_left.round() as i32,
                sel_top.round() as i32,
                sel_width.round() as i32,
                sel_height.round() as i32,
            );
            for annotation in &self.annotations {
                self.draw_annotation(&g, annotation);
            }
            if let Some(annotation) = &self.preview_annotation {
                self.draw_annotation(&g, annotation);
            }
            g.reset_clip();
        }

        if self.has_selection && sel_width > 0.0 && sel_height > 0.0 {
            let width_px = (sel_width / self.canvas_width() * self.screen.width as f32).round() as i32;
            let height_px =
                (sel_height / self.canvas_height() * self.screen.height as f32).round() as i32;
            g.dashed_rect(
                gdiplus::rgb(0xFF, 0xFF, 0xFF),
                (1.5 * self.scale).max(1.0),
                (sel_left, sel_top, sel_width, sel_height),
                (4.0 * self.scale).max(1.0),
                (3.0 * self.scale).max(1.0),
            );
            self.paint_badge(&g, width_px, height_px);
            if self.finalized {
                self.paint_handles(&g);
                self.paint_toolbar(&g);
                self.paint_action_bar(&g);
            }
        }

        if !self.has_selection && !self.full_selection {
            self.paint_hint(&g);
        }

        if let Some(editor) = &self.text_edit {
            self.paint_text_editor(&g, editor);
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

    fn paint_badge(&self, g: &gdiplus::Gfx, width_px: i32, height_px: i32) {
        let scale = self.scale;
        let text = format!("{}x{}", width_px.max(0), height_px.max(0));
        let font_size = 12.0 * scale;
        let (text_width, text_height) = measure_text(&text, font_size);
        let box_width = text_width.round() as i32 + (16.0 * scale).round() as i32;
        let box_height = text_height.round() as i32 + (6.0 * scale).round() as i32;
        let (sel_left, sel_top, sel_width, _) = self.selection;
        let canvas_width = self.canvas_width();
        let canvas_height = self.canvas_height();
        let margin = (4.0 * scale).round() as i32;
        let badge_x = (sel_left.round() as i32).clamp(
            margin,
            ((canvas_width as i32 - box_width - margin).max(margin)),
        );
        let mut badge_y = sel_top.round() as i32 - box_height - (6.0 * scale).round() as i32;
        if badge_y < margin {
            badge_y = (sel_top.round() as i32 + (6.0 * scale).round() as i32)
                .min(canvas_height as i32 - box_height - margin);
        }
        badge_y = badge_y.max(margin);
        let _ = sel_width;
        g.fill_round_rect(
            gdiplus::argb(0xE6, 0x10, 0x11, 0x14),
            badge_x,
            badge_y,
            box_width,
            box_height,
            (8.0 * scale).round() as i32,
        );
        g.text_center(
            &text,
            badge_x as f32,
            badge_y as f32,
            box_width as f32,
            box_height as f32,
            font_size,
            true,
            gdiplus::rgb(0xFF, 0xFF, 0xFF),
        );
    }

    fn paint_handles(&self, g: &gdiplus::Gfx) {
        let scale = self.scale;
        let size = (HANDLE_SIZE * scale).round() as i32;
        let (left, top, width, height) = self.selection;
        let right = left + width;
        let bottom = top + height;
        let cx = left + width / 2.0;
        let cy = top + height / 2.0;
        let centers = [
            (left, top),
            (cx, top),
            (right, top),
            (right, cy),
            (right, bottom),
            (cx, bottom),
            (left, bottom),
            (left, cy),
        ];
        for (x, y) in centers {
            let handle_x = (x - size as f32 / 2.0).round() as i32;
            let handle_y = (y - size as f32 / 2.0).round() as i32;
            g.fill_rect(gdiplus::rgb(0xFF, 0xFF, 0xFF), handle_x, handle_y, size, size);
            g.stroke_round_rect(
                gdiplus::rgb(0, 0, 0),
                (1.0 * scale).max(1.0),
                handle_x,
                handle_y,
                size,
                size,
                0,
            );
        }
    }

    fn paint_toolbar(&self, g: &gdiplus::Gfx) {
        let scale = self.scale;
        let (toolbar, _) = self.toolbar_geometry();
        g.fill_round_rect(
            gdiplus::argb(0xF2, 0x16, 0x17, 0x1A),
            toolbar.0,
            toolbar.1,
            toolbar.2,
            toolbar.3,
            (4.0 * scale).round() as i32,
        );
        g.stroke_round_rect(
            gdiplus::argb(0x44, 0xFF, 0xFF, 0xFF),
            (1.0 * scale).max(1.0),
            toolbar.0,
            toolbar.1,
            toolbar.2,
            toolbar.3,
            (4.0 * scale).round() as i32,
        );
        for index in 0..8 {
            let (rx, ry, rw, rh) = self.toolbar_item_rect(index);
            let active = index < 6
                && Self::tool_for_index(index)
                    .map(|tool| self.active_tool == Some(tool))
                    .unwrap_or(false);
            let hovered = self.hover == Hit::Toolbar(index);
            if active {
                g.fill_round_rect(
                    gdiplus::rgb(0x3B, 0x82, 0xF6),
                    rx,
                    ry,
                    rw,
                    rh,
                    (3.0 * scale).round() as i32,
                );
            } else if hovered {
                g.fill_round_rect(
                    gdiplus::argb(0x24, 0xFF, 0xFF, 0xFF),
                    rx,
                    ry,
                    rw,
                    rh,
                    (3.0 * scale).round() as i32,
                );
            }
            let center_x = rx as f32 + rw as f32 / 2.0;
            let center_y = ry as f32 + rh as f32 / 2.0;
            if index < 6 {
                let icon = Self::toolbar_for_tool(Self::tool_for_index(index).unwrap());
                icons::draw(g, icon, center_x, center_y, 14.0 * scale, gdiplus::rgb(0xFF, 0xFF, 0xFF));
            } else if index == 6 {
                let swatch = (14.0 * scale).round() as i32;
                let swatch_x = rx + (rw - swatch) / 2;
                let swatch_y = ry + (rh - swatch) / 2;
                g.fill_round_rect(
                    self.color,
                    swatch_x,
                    swatch_y,
                    swatch,
                    swatch,
                    (3.0 * scale).round() as i32,
                );
                g.stroke_round_rect(
                    gdiplus::rgb(0xFF, 0xFF, 0xFF),
                    (1.0 * scale).max(1.0),
                    swatch_x,
                    swatch_y,
                    swatch,
                    swatch,
                    (3.0 * scale).round() as i32,
                );
            } else {
                let color = if self.annotations.is_empty() {
                    gdiplus::argb(0x61, 0xFF, 0xFF, 0xFF)
                } else {
                    gdiplus::rgb(0xFF, 0xFF, 0xFF)
                };
                icons::draw(g, Icon::Undo, center_x, center_y, 14.0 * scale, color);
            }
        }
    }

    fn paint_action_bar(&self, g: &gdiplus::Gfx) {
        let scale = self.scale;
        let (_, action) = self.toolbar_geometry();
        g.fill_round_rect(
            gdiplus::argb(0xF2, 0x16, 0x17, 0x1A),
            action.0,
            action.1,
            action.2,
            action.3,
            (4.0 * scale).round() as i32,
        );
        g.stroke_round_rect(
            gdiplus::argb(0x44, 0xFF, 0xFF, 0xFF),
            (1.0 * scale).max(1.0),
            action.0,
            action.1,
            action.2,
            action.3,
            (4.0 * scale).round() as i32,
        );
        let action_icons = [Icon::Copy, Icon::Save, Icon::Close];
        for (index, icon) in action_icons.iter().enumerate() {
            let (rx, ry, rw, rh) = self.action_item_rect(index);
            if self.hover == Hit::Action(index) {
                g.fill_round_rect(
                    gdiplus::argb(0x24, 0xFF, 0xFF, 0xFF),
                    rx,
                    ry,
                    rw,
                    rh,
                    (3.0 * scale).round() as i32,
                );
            }
            icons::draw(
                g,
                *icon,
                rx as f32 + rw as f32 / 2.0,
                ry as f32 + rh as f32 / 2.0,
                14.0 * scale,
                gdiplus::rgb(0xFF, 0xFF, 0xFF),
            );
        }
    }

    fn paint_hint(&self, g: &gdiplus::Gfx) {
        let scale = self.scale;
        let text = crate::i18n::t("DragHint");
        let font_size = 13.0 * scale;
        let (text_width, text_height) = measure_text(text, font_size);
        let box_width = text_width.round() as i32 + (32.0 * scale).round() as i32;
        let box_height = text_height.round() as i32 + (16.0 * scale).round() as i32;
        let x = (self.canvas_width() as i32 - box_width) / 2;
        let y = (28.0 * scale).round() as i32;
        g.fill_round_rect(
            gdiplus::argb(0xE6, 0x10, 0x11, 0x14),
            x,
            y,
            box_width,
            box_height,
            (7.0 * scale).round() as i32,
        );
        g.stroke_round_rect(
            gdiplus::argb(0x44, 0xFF, 0xFF, 0xFF),
            (1.0 * scale).max(1.0),
            x,
            y,
            box_width,
            box_height,
            (7.0 * scale).round() as i32,
        );
        g.text_center(
            text,
            x as f32,
            y as f32,
            box_width as f32,
            box_height as f32,
            font_size,
            false,
            gdiplus::rgb(0xFF, 0xFF, 0xFF),
        );
    }

    fn paint_text_editor(&self, g: &gdiplus::Gfx, editor: &TextEdit) {
        let scale = self.scale;
        let (rect, font_size) = self.text_editor_rect_with_font(editor);
        let left = rect.0.round() as i32;
        let top = rect.1.round() as i32;
        let width = rect.2.round() as i32;
        let height = rect.3.round() as i32;
        g.fill_rect(gdiplus::argb(0x66, 0x00, 0x00, 0x00), left, top, width, height);
        g.stroke_round_rect(
            gdiplus::argb(0x88, 0xFF, 0xFF, 0xFF),
            (1.0 * scale).max(1.0),
            left,
            top,
            width,
            height,
            0,
        );
        g.push_clip_rect(left, top, width, height);
        let text_x = left as f32 + 3.0 * scale - editor.scroll;
        let text_y = top as f32 + (height as f32 - font_size * 1.35) / 2.0;
        if let Some((start, end)) = editor.selection_offsets(g, font_size) {
            g.fill_rect(
                gdiplus::argb(0x66, 0x3B, 0x82, 0xF6),
                (text_x + start).round() as i32,
                top + (3.0 * scale).round() as i32,
                (end - start).round() as i32,
                height - (6.0 * scale).round() as i32,
            );
        }
        g.text(
            &editor.text,
            text_x,
            text_y,
            width as f32 + editor.scroll,
            font_size * 1.35,
            font_size,
            false,
            self.color,
        );
        if self.caret_visible {
            let caret = text_x + editor.caret_x(g, font_size);
            g.fill_rect(
                self.color,
                caret.round() as i32,
                top + (4.0 * scale).round() as i32,
                (1.0 * scale).max(1.0) as i32,
                height - (8.0 * scale).round() as i32,
            );
        }
        g.reset_clip();
    }
}

impl Drop for EditorState {
    fn drop(&mut self) {
        win::kill_timer(self.hwnd, TIMER_CARET);
    }
}

fn measure_text(text: &str, size: f32) -> (f32, f32) {
    let Some(surface) = BitmapSurface::new(4, 4) else {
        return (0.0, size * 1.35);
    };
    let Some(g) = surface.graphics() else {
        return (0.0, size * 1.35);
    };
    let measured = g.measure_text(text, size, true, 100000.0, false);
    (measured.0, measured.1)
}

fn point_in(rect: (i32, i32, i32, i32), x: i32, y: i32) -> bool {
    x >= rect.0 && x < rect.0 + rect.2 && y >= rect.1 && y < rect.1 + rect.3
}

extern "system" fn editor_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if message == windows::Win32::UI::WindowsAndMessaging::WM_NCCREATE {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let pointer = win::user_data(hwnd) as *mut EditorState;
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
            WM_MOUSEMOVE => {
                state.mouse_move(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            win::WM_MOUSELEAVE_MSG => {
                state.tracking_mouse = false;
                state.hover = Hit::None;
                state.hover_handle = Handle::None;
                win::invalidate(hwnd);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                state.mouse_down(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                state.mouse_up(win::get_x_lparam(lparam), win::get_y_lparam(lparam));
                LRESULT(0)
            }
            WM_TIMER => {
                if wparam.0 == TIMER_CARET {
                    state.caret_visible = !state.caret_visible;
                    win::invalidate(hwnd);
                }
                LRESULT(0)
            }
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                state.handle_key(win::key_from_wparam(wparam), message == WM_SYSKEYDOWN);
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(character) = char::from_u32(wparam.0 as u32) {
                    if !win::key_is_down(VK_CONTROL.0 as u32) {
                        state.handle_char(character);
                    }
                }
                LRESULT(0)
            }
            WM_DPICHANGED => {
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
                    EDITOR_CLOSED,
                    WPARAM(pointer as usize),
                    LPARAM(0),
                );
                result
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
