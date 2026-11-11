use windows::Win32::Foundation::HWND;

use crate::gdiplus::Gfx;

pub struct TextEdit {
    pub text: String,
    pub caret: usize,
    pub anchor: Option<usize>,
    pub scroll: f32,
}

impl TextEdit {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            caret: text.len(),
            anchor: None,
            scroll: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn selected_range(&self) -> Option<(usize, usize)> {
        let anchor = self.anchor?;
        if anchor == self.caret {
            return None;
        }
        Some(if anchor < self.caret {
            (anchor, self.caret)
        } else {
            (self.caret, anchor)
        })
    }

    fn clear_selection(&mut self) -> bool {
        if let Some((start, end)) = self.selected_range() {
            self.text.replace_range(start..end, "");
            self.caret = start;
            self.anchor = None;
            true
        } else {
            self.anchor = None;
            false
        }
    }

    pub fn insert_str(&mut self, value: &str) {
        self.clear_selection();
        let sanitized: String = value
            .chars()
            .filter(|character| *character != '\r' && *character != '\n')
            .collect();
        if sanitized.is_empty() {
            return;
        }
        self.text.insert_str(self.caret, &sanitized);
        self.caret += sanitized.len();
        self.anchor = None;
    }

    pub fn insert_char(&mut self, character: char) {
        if character == '\r' || character == '\n' {
            return;
        }
        let mut buffer = [0u8; 4];
        self.insert_str(character.encode_utf8(&mut buffer));
    }

    pub fn backspace(&mut self) {
        if self.clear_selection() {
            return;
        }
        if self.caret == 0 {
            return;
        }
        let previous = previous_boundary(&self.text, self.caret);
        self.text.replace_range(previous..self.caret, "");
        self.caret = previous;
    }

    pub fn delete(&mut self) {
        if self.clear_selection() {
            return;
        }
        if self.caret >= self.text.len() {
            return;
        }
        let next = next_boundary(&self.text, self.caret);
        self.text.replace_range(self.caret..next, "");
    }

    pub fn move_left(&mut self, extend: bool) {
        if !extend {
            if let Some((start, _)) = self.selected_range() {
                self.caret = start;
                self.anchor = None;
                return;
            }
        }
        if self.caret > 0 {
            if extend && self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
            self.caret = previous_boundary(&self.text, self.caret);
            if !extend {
                self.anchor = None;
            }
        }
    }

    pub fn move_right(&mut self, extend: bool) {
        if !extend {
            if let Some((_, end)) = self.selected_range() {
                self.caret = end;
                self.anchor = None;
                return;
            }
        }
        if self.caret < self.text.len() {
            if extend && self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
            self.caret = next_boundary(&self.text, self.caret);
            if !extend {
                self.anchor = None;
            }
        }
    }

    pub fn home(&mut self, extend: bool) {
        if extend && self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
        self.caret = 0;
        if !extend {
            self.anchor = None;
        }
    }

    pub fn end(&mut self, extend: bool) {
        if extend && self.anchor.is_none() {
            self.anchor = Some(self.caret);
        }
        self.caret = self.text.len();
        if !extend {
            self.anchor = None;
        }
    }

    pub fn select_all(&mut self) {
        self.anchor = Some(0);
        self.caret = self.text.len();
    }

    pub fn copy(&self, owner: HWND) -> bool {
        let value = if let Some((start, end)) = self.selected_range() {
            self.text[start..end].to_string()
        } else {
            return false;
        };
        crate::clipboard::set_clipboard_text(owner, &value)
    }

    pub fn cut(&mut self, owner: HWND) -> bool {
        if self.copy(owner) {
            self.clear_selection();
            true
        } else {
            false
        }
    }

    pub fn paste(&mut self, owner: HWND) -> bool {
        if let Some(value) = crate::images::get_clipboard_text(owner) {
            self.insert_str(&value);
            true
        } else {
            false
        }
    }

    pub fn caret_x(&self, g: &Gfx, font_size: f32) -> f32 {
        g.measure_text(&self.text[..self.caret], font_size, false, 100000.0, false)
            .0
    }

    pub fn position_from_x(&mut self, g: &Gfx, x: f32, font_size: f32) {
        let mut best = 0usize;
        let mut best_distance = f32::MAX;
        let mut boundary = 0usize;
        loop {
            let width = g
                .measure_text(&self.text[..boundary], font_size, false, 100000.0, false)
                .0;
            let distance = (width - x).abs();
            if distance < best_distance {
                best_distance = distance;
                best = boundary;
            }
            if boundary >= self.text.len() {
                break;
            }
            boundary = next_boundary(&self.text, boundary);
        }
        self.caret = best;
        self.anchor = None;
    }

    pub fn ensure_visible(&mut self, g: &Gfx, font_size: f32, width: f32) {
        let caret = self.caret_x(g, font_size);
        if caret - self.scroll > width - 4.0 {
            self.scroll = caret - width + 4.0;
        }
        if caret < self.scroll {
            self.scroll = caret;
        }
        self.scroll = self.scroll.max(0.0);
    }

    pub fn selection_offsets(&self, g: &Gfx, font_size: f32) -> Option<(f32, f32)> {
        let (start, end) = self.selected_range()?;
        let start_x = g
            .measure_text(&self.text[..start], font_size, false, 100000.0, false)
            .0;
        let end_x = g
            .measure_text(&self.text[..end], font_size, false, 100000.0, false)
            .0;
        Some((start_x, end_x))
    }
}

pub fn previous_boundary(text: &str, index: usize) -> usize {
    if index == 0 {
        return 0;
    }
    let mut value = index - 1;
    while value > 0 && !text.is_char_boundary(value) {
        value -= 1;
    }
    value
}

pub fn next_boundary(text: &str, index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    let mut value = index + 1;
    while value < text.len() && !text.is_char_boundary(value) {
        value += 1;
    }
    value
}
